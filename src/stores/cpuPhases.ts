// ─────────────────────────────────────────────────────────────────────────────
// Phased single-cycle execution — six visual micro-steps per instruction.
//
// Each panel phase runs the matching Rust micro-phases (sim/src/sim/
// cpu_state.rs) and then lights the buses / control lines that phase drives,
// with the values the Rust core computed:
//   fetch    = IrqCheck + Fetch      decode  = Decode + Control
//   regread  = Imm + RegRead         execute = Alu
//   memwb    = Mem + Wb              pcupdate = PcUpdate
// Loads / stores are sized (B / H / W through the load-store unit); MUL /
// MULH / MULHSU / MULHU run on the MUL block (Zmmul) into WB Sel input 4.
// Previous phases' values stay; only the current phase's buses are "active".
// ─────────────────────────────────────────────────────────────────────────────

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { useCpuState, type Micro } from './cpuState'
import { OPCODE_BY_VALUE } from '../data/controlData'

export type Phase =
  | 'idle'
  | 'fetch'
  | 'decode'
  | 'regread'
  | 'execute'
  | 'memwb'
  | 'pcupdate'

const PHASE_ORDER: Phase[] = [
  'fetch', 'decode', 'regread', 'execute', 'memwb', 'pcupdate'
]
/** Rust micro-phases per panel phase */
const MICRO_STEPS: Record<Phase, number> = { idle: 0, fetch: 2, decode: 2, regread: 2, execute: 1, memwb: 2, pcupdate: 1 }

const PHASE_LABELS: Record<Phase, string> = {
  idle:      'Ready',
  fetch:     '1. Fetch — PC → ROM → Instruction',
  decode:    '2. Decode — Control signals',
  regread:   '3. Register Read + Imm Gen',
  execute:   '4. Execute — ALU',
  memwb:     '5. Memory + Write Back',
  pcupdate:  '6. PC Update — Next PC',
}

const u = (v: number | null | undefined) => (v ?? 0) >>> 0

export const useCpuPhases = defineStore('cpuPhases', () => {

  const cpu = useCpuState()

  const currentPhase = ref<Phase>('idle')
  const phaseIndex = ref(-1)
  const animating = ref(false)
  const autoRunning = ref(false)
  const animDuration = ref(400)

  const phaseLabel = computed(() => {
    if (!cpu.ready) return cpu.loadError ? `Rust core not loaded (reload the page): ${cpu.loadError}` : 'Loading the Rust core…'
    return PHASE_LABELS[currentPhase.value]
  })

  // Resume where a reloaded page left off (the Rust core restores mid-instruction)
  cpu.whenReady().then(() => {
    const i = cpu.restoredPhaseIndex
    if (i >= 0 && i < PHASE_ORDER.length - 1) {
      phaseIndex.value = i
      currentPhase.value = PHASE_ORDER[i]!
    }
  }).catch(() => { /* shown in the label */ })

  /** The instruction's control signals (as decoded by the core) */
  const sig = (m: Micro) => ({
    regWrite: !!m.ctrl_reg_write, aluSrc: !!m.ctrl_alu_src,
    memRead: !!m.ctrl_mem_read, memWrite: !!m.ctrl_mem_write,
    aluOp: m.ctrl_alu_op ?? 0, wbSel: m.ctrl_wb_sel ?? 0, pcSel: m.ctrl_pc_sel ?? 0, immSel: m.ctrl_imm_sel ?? 5,
  })
  const isCsrOp = (m: Micro) => m.opcode === 0b1110011 && ((m.funct3 ?? 0) & 3) !== 0 && !!OPCODE_BY_VALUE.get(m.opcode)

  // ── Phase functions: run the core, then light what the phase drives ──────

  function phaseFetch() {
    cpu.clearActiveFlags()
    cpu.clearBusStates()
    const m = cpu.runMicro(MICRO_STEPS.fetch)
    if (!m) return
    cpu.lastInstr = u(m.instr)
    cpu.setBus('pc_out', u(m.pc_out))
    cpu.setBus('instr', u(m.instr))
    cpu.markRom(u(m.pc_out))
  }

  function phaseDecode() {
    const m = cpu.runMicro(MICRO_STEPS.decode)
    if (!m) return
    const entry = OPCODE_BY_VALUE.get(m.opcode ?? 0)
    cpu.lastOpEntry = entry
    if (!entry) return            // unknown opcode: NOP, nothing driven
    const s = sig(m)
    cpu.setCtrl('ctrl_reg_write', s.regWrite)
    cpu.setCtrl('ctrl_alu_src', s.aluSrc)
    cpu.setCtrl('ctrl_mem_read', s.memRead)
    cpu.setCtrl('ctrl_mem_write', s.memWrite)
    cpu.setBus('ctrl_alu_op', s.aluOp)
    cpu.setBus('ctrl_wb_sel', s.wbSel)
    cpu.setBus('ctrl_imm_sel', s.immSel)
    cpu.setBus('ctrl_pc_sel', s.pcSel)
  }

  function phaseRegRead() {
    const m = cpu.runMicro(MICRO_STEPS.regread)
    if (!m) return
    const s = sig(m)
    cpu.setBus('immediate', u(m.immediate))
    if ((m.rs1_num ?? 0) !== 0 || !s.aluSrc) cpu.markReg(m.rs1_num ?? 0, 'readA')
    if (!s.aluSrc) cpu.markReg(m.rs2_num ?? 0, 'readB')
    cpu.setBus('rs1_data', u(m.rs1_data))
    cpu.setBus('rs2_data', u(m.rs2_data))
    cpu.setBus('alu_b', u(m.alu_b))
  }

  function phaseExecute() {
    const m = cpu.runMicro(MICRO_STEPS.execute)
    if (!m) return
    cpu.setBus('alu_b', u(m.alu_b))
    cpu.setBus('alu_control', m.alu_control ?? 0)
    cpu.setBus('alu_result', u(m.alu_result))
    cpu.setCtrl('zero_flag', !!m.zero_flag)
    if (isCsrOp(m)) cpu.setBus('csr_read_data', u(m.csr_read_data))
    if (m.is_mul) cpu.setBus('mul_result', u(m.mul_result))
  }

  function phaseMemWb() {
    const m = cpu.runMicro(MICRO_STEPS.memwb)
    if (!m) return
    const s = sig(m)
    const addr = u(m.alu_result)
    cpu.setBus('alu_result', addr)
    cpu.setBus('rs2_data', u(m.rs2_data))
    if (m.exc) return                // misaligned access: no memory access, no write-back
    // Byte lanes touched (B / H; a word access leaves the whole row marked)
    const f3 = (m.funct3 ?? 2) & 3
    const lanes = s.memWrite ? (m.mem_bw ?? 0xf) : f3 >= 2 ? 0xf : ((f3 === 0 ? 1 : 3) << (addr & (f3 === 0 ? 3 : 2)))
    if (s.memRead) {
      cpu.setBus('ctrl_mem_read', 1)
      if (cpu.isIoAddress(addr)) cpu.ioOp = 'read'
      else cpu.markRam(addr, lanes)
      cpu.setBus('mem_read_data', u(m.mem_read_data))
    }
    if (s.memWrite) {
      cpu.setBus('ctrl_mem_write', 1)
      if (cpu.isIoAddress(addr)) cpu.ioOp = 'write'
      else cpu.markRam(addr, lanes)
    }
    cpu.setBus('write_data', u(m.write_data))
    if (s.regWrite && (m.rd ?? 0) !== 0) cpu.markReg(m.rd!, 'active')
  }

  function phasePcUpdate() {
    const m = cpu.runMicro(MICRO_STEPS.pcupdate)
    if (!m) return
    const taken = !!m.offset_sel
    const isJalr = !!m.is_jalr
    if (taken) {
      cpu.setBus('instr', u(m.instr))
      cpu.setBus('immediate', u(m.immediate))
    }
    cpu.setCtrl('ctrl_offset_sel', taken)
    cpu.setBus('pc_offset', u(m.pc_offset))
    cpu.setBus('pc_adder_result', u(m.pc_adder_result))
    cpu.setBus('jalr_target', u(m.jalr_target))
    if (isJalr) cpu.setBus('alu_result', u(m.alu_result))
    cpu.setCtrl('ctrl_is_jalr', isJalr)
    cpu.setBus('ctrl_pc_sel', isJalr ? 2 : (taken ? 1 : 0))
    cpu.setBus('next_pc', u(m.next_pc))
    cpu.nextPc = m.next_pc === null ? cpu.pc : u(m.next_pc)
  }

  const PHASE_FNS: Record<Phase, (() => void) | null> = {
    idle: null,
    fetch: phaseFetch,
    decode: phaseDecode,
    regread: phaseRegRead,
    execute: phaseExecute,
    memwb: phaseMemWb,
    pcupdate: phasePcUpdate,
  }

  /** Run the next panel phase (wraps after pcupdate) */
  function advance() {
    phaseIndex.value = (phaseIndex.value + 1) % PHASE_ORDER.length
    const phase = PHASE_ORDER[phaseIndex.value]!
    currentPhase.value = phase
    if (phaseIndex.value > 0) cpu.clearBusVisuals()
    PHASE_FNS[phase]?.()
  }

  // ── Controls ─────────────────────────────────────────────────────────────

  /** Advance one micro-phase (after pcupdate: back to Ready, then fetch) */
  function microStep() {
    if (animating.value || cpu.halted || !cpu.ready) return
    if (phaseIndex.value >= PHASE_ORDER.length - 1) {
      phaseIndex.value = -1
      currentPhase.value = 'idle'
      cpu.clearBusVisuals()
      return
    }
    advance()
    cpu.saveState({ phase: currentPhase.value, phaseIndex: phaseIndex.value })
    animating.value = true
    setTimeout(() => { animating.value = false }, animDuration.value)
  }

  /** Play the rest of the current instruction (or a whole one) with animation */
  async function animatedStep() {
    if (animating.value || cpu.halted || !cpu.ready) return
    animating.value = true
    if (phaseIndex.value >= PHASE_ORDER.length - 1) phaseIndex.value = -1
    do {
      advance()
      await delay(animDuration.value)
    } while (phaseIndex.value < PHASE_ORDER.length - 1)
    phaseIndex.value = -1
    currentPhase.value = 'idle'
    cpu.clearBusVisuals()
    cpu.saveState({ phase: currentPhase.value, phaseIndex: phaseIndex.value })
    animating.value = false
  }

  /** Run one named phase right away if it is the next one (tests, tools) */
  function runPhase(phase: Phase) {
    if (cpu.halted || phase === 'idle' || !cpu.ready) return
    const next = PHASE_ORDER[(phaseIndex.value + 1) % PHASE_ORDER.length]
    if (next !== phase) return
    advance()
    cpu.saveState({ phase: currentPhase.value, phaseIndex: phaseIndex.value })
  }

  /** Instant step — finish the current instruction (or run a whole one) */
  function instantStep() {
    if (cpu.halted || !cpu.ready) return
    if (phaseIndex.value >= PHASE_ORDER.length - 1) phaseIndex.value = -1
    do advance(); while (phaseIndex.value < PHASE_ORDER.length - 1)
    currentPhase.value = 'idle'
    phaseIndex.value = -1
    cpu.saveState({ phase: currentPhase.value, phaseIndex: phaseIndex.value })
  }

  /** Auto-run: animated steps continuously */
  function startAutoRun() {
    if (autoRunning.value) return
    autoRunning.value = true
    const runNext = async () => {
      if (!autoRunning.value || cpu.halted) { stopAutoRun(); return }
      await animatedStep()
      await delay(animDuration.value) // pause between instructions
      if (autoRunning.value) runNext()
    }
    runNext()
  }

  function stopAutoRun() { autoRunning.value = false }

  function toggleAutoRun() {
    if (autoRunning.value) stopAutoRun()
    else startAutoRun()
  }

  function reset() {
    stopAutoRun()
    cpu.reset()
    currentPhase.value = 'idle'
    phaseIndex.value = -1
    animating.value = false
    cpu.saveState({ phase: 'idle', phaseIndex: -1 })
  }

  return {
    currentPhase,
    phaseIndex,
    phaseLabel,
    animating,
    autoRunning,
    animDuration,
    microStep,
    runPhase,
    animatedStep,
    instantStep,
    startAutoRun,
    stopAutoRun,
    toggleAutoRun,
    reset,
  }
})

function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms))
}
