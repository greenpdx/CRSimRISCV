// ─────────────────────────────────────────────────────────────────────────────
// Single-cycle CPU state — backed by the Rust core (wasm SingleCycle.panel()).
//
// Rust does all execution (sim/src/sim/cpu_state.rs: 10 micro-phases, traps
// and CSRs via the shared system unit). This store mirrors the core's state
// into reactive refs for the panel, and keeps the presentation state the
// panel draws: which buses / control lines are lit, active registers and
// memory words. cpuPhases.ts drives it one panel phase at a time.
// ─────────────────────────────────────────────────────────────────────────────

import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'

import type { Register, MemoryWord, BusState, ControlSignalState } from '../types/cpu'
import { REGISTER_DEFS } from '../data/registerData'
import { OPCODE_BY_VALUE } from '../data/controlData'
import { RV32I_NETLIST } from '../data/netlist'
import { DEFAULT_PROGRAM } from '../data/defaultProgram'
import { useCpuConfig } from './cpuConfig'
import { irqMask, regionWords } from './wasmCore'
import type { CpuState, MicroState, SingleCycle } from '../wasm/cpu_sim'

const ROM_WORDS = 64
const RAM_WORDS = 64
/** Until the core reports its map (`regions()`) */
const RAM_BASE_DEFAULT = 0x00010000
const STORAGE_KEY = 'cpuvis_state'

type PrivMode = 'M' | 'S' | 'U'

/** MicroState fields from the Rust core (sim/src/sim/micro.rs), undefined until driven */
export type Micro = MicroState
type Core = SingleCycle

/**
 * A saved core state (`SingleCycle.save()` after JSON.parse) in today's shape.
 * Saves from before the memory moved into `mem` have rom / ram / io_data /
 * io_base / ram_extra at the top level; they move into `mem` here.
 */
export function migrateSave(state: Record<string, unknown>): CpuState {
  if (!('mem' in state)) {
    const mem: Record<string, unknown> = {}
    for (const k of ['rom', 'ram', 'io_data', 'io_base', 'ram_extra']) {
      if (k in state) { mem[k] = state[k]; delete state[k] }
    }
    state.mem = mem
  }
  return state as unknown as CpuState
}

const mkRegisters = (): Register[] =>
  REGISTER_DEFS.map(d => ({ name: d.name, xNum: d.xNum, value: d.resetVal, active: false, readA: false, readB: false }))

const mkBusStates = (): Record<string, BusState> =>
  Object.fromEntries(RV32I_NETLIST.nets.map(n => [n.id, { netId: n.id, bits: n.bits, value: 0, state: 'idle' } satisfies BusState]))

const mkControlStates = (): Record<string, ControlSignalState> =>
  Object.fromEntries(RV32I_NETLIST.nets.filter(n => n.bits === 1)
    .map(n => [n.id, { netId: n.id, asserted: false } satisfies ControlSignalState]))

const mkRom = (entries: { value: number; asm?: string }[]): MemoryWord[] =>
  Array.from({ length: ROM_WORDS }, (_, i) => ({
    address: i * 4,
    value: (entries[i]?.value ?? 0x00000013) >>> 0,
    asm: i < entries.length ? entries[i]!.asm : 'nop',
    active: false,
  }))

export const useCpuState = defineStore('cpu', () => {
  // ── Reactive mirror of the core ───────────────────────────────────────────
  const registers = ref<Register[]>(mkRegisters())
  const pc     = ref<number>(0x00000000)
  const nextPc = ref<number>(0x00000004)
  const romWords = ref<MemoryWord[]>(mkRom(DEFAULT_PROGRAM))
  const ramWords = ref<MemoryWord[]>(
    Array.from({ length: RAM_WORDS }, (_, i) => ({ address: RAM_BASE_DEFAULT + i * 4, value: 0, active: false })))
  const ioBaseAddress = ref<number>(0x40020000)
  const ioData        = ref<number>(0x00000000)
  const ioOp          = ref<'idle' | 'read' | 'write'>('idle')
  const csrRegisters  = ref<Record<number, number>>({})

  const trapsEnabled = ref<boolean>(false)
  const privMode    = ref<PrivMode>('M')
  const pendingIrq  = ref<number>(0)
  const trapDepth   = ref<number>(0)
  const lastTrap    = ref<{
    cycle: number; fromMode: PrivMode; toMode: PrivMode
    cause: number; isInterrupt: boolean; epc: number; tval: number
  } | null>(null)

  // ── Presentation state (what the panel lights) ────────────────────────────
  const busStates     = ref<Record<string, BusState>>(mkBusStates())
  const controlStates = ref<Record<string, ControlSignalState>>(mkControlStates())
  const lastInstr    = ref<number>(0x00000000)
  const lastOpEntry  = ref<ReturnType<typeof OPCODE_BY_VALUE.get>>(undefined)
  const halted       = ref<boolean>(false)
  const cycleCount   = ref<number>(0)
  /** Panel phase index restored from storage (-1 = between instructions) */
  const restoredPhaseIndex = ref<number>(-1)

  const currentInstruction = computed(() => romWords.value.find(w => w.address === pc.value)?.value ?? 0)

  // ── Rust core ─────────────────────────────────────────────────────────────
  let core: Core | null = null
  const config = useCpuConfig()
  const ready = ref(false)
  const loadError = ref<string | null>(null)
  let loading: Promise<void> | null = null

  /** Load the wasm core (retries a few times, e.g. while the package is rebuilt) */
  function whenReady(): Promise<void> {
    if (core) return Promise.resolve()
    if (!loading) loading = (async () => {
      for (let attempt = 1; ; attempt++) {
        try {
          const mod = await import('../wasm/cpu_sim.js')
          await mod.default()
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          core = mod.SingleCycle.panel()
          loadError.value = null
          ready.value = true
          if (!restoreState().restored) {
            core.set_rom(new Uint32Array(romWords.value.map(w => w.value)))
            core.set_io_base(ioBaseAddress.value)
          }
          core.set_zmmul(config.enableZmmul)     // the config option wins over a saved core
          sync()
          return
        } catch (e) {
          loadError.value = String(e)
          console.warn(`single-cycle core: load attempt ${attempt} failed`, e)
          if (attempt >= 5) { loading = null; throw e }
          await new Promise(r => setTimeout(r, 1000 * attempt))
        }
      }
    })()
    return loading
  }

  /** Copy the core's architectural state into the reactive mirror */
  function sync() {
    if (!core) return
    const regs = core.regs()
    registers.value.forEach((r, i) => { r.value = (regs[i] ?? 0) >>> 0 })
    pc.value = core.pc >>> 0
    const r = core.regions().find(x => x.name === 'ram')
    const ram = r ? core.mem(r.base, r.words) : new Uint32Array(0)
    for (let i = 0; i < RAM_WORDS; i++) {
      const w = ramWords.value[i]
      if (w) { if (r) w.address = (r.base + i * 4) >>> 0; w.value = (ram[i] ?? 0) >>> 0 }
    }
    const extra = core.ram_extra()
    for (let k = 0; k < extra.length; k += 2) {
      const address = extra[k]! >>> 0, value = extra[k + 1]! >>> 0
      const w = ramWords.value.find(x => x.address === address)
      if (w) w.value = value
      else ramWords.value.push({ address, value, active: false })
    }
    ioData.value = core.io_data >>> 0
    const csrs: Record<number, number> = {}
    for (const c of core.csr_snapshot()) csrs[c.addr] = c.value >>> 0
    csrRegisters.value = csrs
    cycleCount.value = csrs[0xC00] ?? 0
    privMode.value = (core.priv_mode as PrivMode) || 'M'
    pendingIrq.value = core.pending_irq >>> 0
    trapDepth.value = core.trap_depth
    const t = core.last_trap()
    lastTrap.value = t ? {
      cycle: Number(t.cycle), fromMode: t.from as PrivMode, toMode: t.to as PrivMode,
      cause: t.cause, isInterrupt: t.is_interrupt, epc: t.epc >>> 0, tval: t.tval >>> 0,
    } : null
  }

  /** Run `n` Rust micro-phases; returns the core's MicroState afterwards */
  function runMicro(n: number): Micro | null {
    if (!core) return null
    for (let i = 0; i < n; i++) core.micro_step()
    const m = core.micro_raw()
    sync()
    return m
  }

  /** Core's MicroState without stepping */
  const micro = (): Micro | null => core?.micro_raw() ?? null

  // ── Presentation helpers ──────────────────────────────────────────────────
  function clearActiveFlags() {
    registers.value.forEach(r => { r.active = false; r.readA = false; r.readB = false })
    romWords.value.forEach(w => { w.active = false })
    ramWords.value.forEach(w => { w.active = false; delete w.lanes })
    ioOp.value = 'idle'
  }
  function clearBusStates() {
    for (const s of Object.values(busStates.value))     s.state    = 'idle'
    for (const s of Object.values(controlStates.value)) s.asserted = false
  }
  /** Clear only bus visual states (idle), preserve control signal values */
  function clearBusVisuals() {
    for (const s of Object.values(busStates.value)) s.state = 'idle'
  }
  function setBus(netId: string, value: number) {
    const b = busStates.value[netId]
    if (b) { b.value = value; b.state = 'active' }
  }
  function setCtrl(netId: string, asserted: boolean) {
    const c = controlStates.value[netId]
    if (c) c.asserted = asserted
  }
  function markRom(address: number) {
    const w = romWords.value.find(x => x.address === address)
    if (w) w.active = true
  }
  /** Mark the RAM word holding `address`; `lanes` = bytes a sub-word access touched */
  function markRam(address: number, lanes?: number) {
    const w = ramWords.value.find(x => x.address === (address & ~3) >>> 0)
    if (w) { w.active = true; if (lanes !== undefined && lanes !== 0xf) w.lanes = lanes }
  }
  function markReg(xNum: number, kind: 'active' | 'readA' | 'readB') {
    const r = registers.value[xNum]
    if (r) r[kind] = true
  }
  function isIoAddress(address: number): boolean {
    return (address >>> 0) >= (ioBaseAddress.value >>> 0)
  }
  function regRead(xNum: number): number { return registers.value[xNum]?.value ?? 0 }

  // ── Actions ───────────────────────────────────────────────────────────────
  function reset() {
    core?.reset_keep_rom()
    nextPc.value = 0x00000004
    ioOp.value = 'idle'
    halted.value = false
    busStates.value = mkBusStates()
    controlStates.value = mkControlStates()
    lastInstr.value = 0
    lastOpEntry.value = undefined
    clearActiveFlags()
    ramWords.value.forEach(w => { w.value = 0 })
    if (!core) {
      registers.value = mkRegisters()
      pc.value = 0
      ioData.value = 0
    }
    sync()
  }

  function setRom(entries: { value: number; asm?: string }[]) {
    romWords.value = mkRom(entries)
    core?.set_rom(new Uint32Array(romWords.value.map(w => w.value)))
    reset()
  }

  /** Load a program into ROM (instruction words from address 0) */
  function loadProgram(words: number[]) {
    setRom(words.map(value => ({ value, asm: undefined })))
  }

  /** Load with pre-decoded ASM labels */
  function loadProgramWithAsm(entries: { value: number; asm: string }[]) {
    setRom(entries)
  }

  /** Write to I/O from outside (input bits / panel) */
  function setIoData(value: number) {
    core?.set_io_data(value >>> 0)
    ioData.value = value >>> 0
  }

  function raiseIrq(kind: 'mti' | 'mei' | 'msi' | 'sti' | 'sei' | 'ssi') { core?.raise_irq(irqMask(kind)); sync() }
  function clearIrqs() { core?.clear_irq(); sync() }
  function csrRead(addr: number): number { return core ? core.csr_read(addr) >>> 0 : (csrRegisters.value[addr] ?? 0) }
  function csrWrite(addr: number, value: number) { core?.csr_write(addr, value >>> 0); sync() }

  watch(trapsEnabled, on => core?.set_traps_enabled(on))
  watch(() => config.enableZmmul, on => core?.set_zmmul(on))
  watch(ioBaseAddress, b => core?.set_io_base(b >>> 0))

  // ── Persistence (localStorage): the whole Rust core state + ROM labels ────
  function saveState(phaseData?: { phase: string; phaseIndex: number }) {
    if (!core) return
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({
        version: 2,
        core: JSON.stringify(core.save()),
        asm: romWords.value.map(w => w.asm ?? null),
        nextPc: nextPc.value,
        lastInstr: lastInstr.value,
        phaseIndex: phaseData?.phaseIndex ?? -1,
      }))
    } catch { /* ignore storage errors */ }
  }

  function restoreState(): { restored: boolean; phaseIndex?: number } {
    if (!core) return { restored: false }
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (!raw) return { restored: false }
      const s = JSON.parse(raw)
      if (s.version === 2 && typeof s.core === 'string' && core.restore(migrateSave(JSON.parse(s.core)))) {
        const rom = regionWords(core, 'rom')
        romWords.value = Array.from(rom, (value: number, i: number) => ({
          address: i * 4, value: value >>> 0, asm: s.asm?.[i] ?? undefined, active: false,
        }))
        trapsEnabled.value = core.traps_enabled
        ioBaseAddress.value = core.io_base >>> 0
        nextPc.value = s.nextPc ?? 4
        lastInstr.value = s.lastInstr ?? 0
        restoredPhaseIndex.value = s.phaseIndex ?? -1
        return { restored: true, phaseIndex: restoredPhaseIndex.value }
      }
      // Older format (TypeScript engine): keep its program, start fresh
      if (Array.isArray(s.romWords)) {
        romWords.value = mkRom(s.romWords.map((w: { value: number; asm?: string }) => ({ value: w.value, asm: w.asm })))
        core.set_rom(new Uint32Array(romWords.value.map(w => w.value)))
        if (typeof s.ioBaseAddress === 'number') { ioBaseAddress.value = s.ioBaseAddress; core.set_io_base(s.ioBaseAddress) }
        return { restored: true, phaseIndex: -1 }
      }
      return { restored: false }
    } catch {
      return { restored: false }
    }
  }

  function clearSavedState() { localStorage.removeItem(STORAGE_KEY) }

  // Load the core (and restore the saved state) on first use
  whenReady().catch(() => { /* loadError is shown by the panel */ })

  return {
    // State
    registers, pc, nextPc, romWords, ramWords,
    ioBaseAddress, ioData, ioOp, busStates, controlStates,
    lastInstr, lastOpEntry, halted, cycleCount, csrRegisters,
    trapsEnabled, privMode, pendingIrq, lastTrap, trapDepth,
    ready, loadError, restoredPhaseIndex,
    // Getters
    currentInstruction,
    // Core
    whenReady, runMicro, micro, sync,
    // Presentation helpers (cpuPhases)
    clearActiveFlags, clearBusStates, clearBusVisuals, setBus, setCtrl, markRom, markRam, markReg,
    isIoAddress, regRead,
    // Actions
    reset, loadProgram, loadProgramWithAsm, setIoData,
    raiseIrq, clearIrqs, csrRead, csrWrite,
    saveState, restoreState, clearSavedState,
  }
})
