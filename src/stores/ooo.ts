// ─────────────────────────────────────────────────────────────────────────────
// Out-of-order panel — Tomasulo + reorder buffer core, all in Rust
// (sim/src/sim/ooo, wasm JsOoo). This store steps it a stage or a cycle
// at a time and hands the panel its snapshot and this cycle's events.
// ─────────────────────────────────────────────────────────────────────────────

import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'

export type Unit = 'Alu' | 'Mul' | 'Br' | 'Lsu' | 'Sys'
export type Stage = 'Commit' | 'Resolve' | 'Memory' | 'Issue' | 'Writeback' | 'Dispatch' | 'Fetch'
export const STAGES: Stage[] = ['Commit', 'Resolve', 'Memory', 'Issue', 'Writeback', 'Dispatch', 'Fetch']

export interface Operand { value: number; tag?: number | null }
export interface Prediction { taken: boolean; next: number; btb_hit: boolean; counter: number }
export interface Decoded { op: unknown; unit: Unit; rd: number; rs1: number; rs2: number; imm: number }
export interface Times { fetch: number; dispatch?: number | null; issue?: number | null; complete?: number | null; commit?: number | null }
export interface RobEntry {
  seq: number; pc: number; instr: number; d: Decoded; value: number; done: boolean
  exc?: [number, number] | null; pred: Prediction; store?: [number, number, number] | null; src1: number; t: Times
}
export interface RsEntry { seq: number; rob: number; pc: number; d: Decoded; j: Operand; k: Operand; pred: Prediction }
export type LsqState = 'Address' | 'Ready' | 'Memory' | 'Done'
export interface LsqEntry {
  seq: number; rob: number; store: boolean; size: number; signed: boolean; base: Operand; data: Operand
  imm: number; addr?: number | null; addr_at: number; state: LsqState
}
export interface Exec { seq: number; rob: number; unit: Unit; fu: number; issued: number; finish: number; value: number; br?: { taken: boolean; next: number; resolved: boolean } | null }
export interface BtbEntry { valid: boolean; pc: number; target: number; jump: boolean }
export interface Retired { seq: number; pc: number; instr: number; t: Times; squashed: boolean }
export interface OooStats {
  cycles: number; committed: number; branches: number; mispredicts: number; squashed: number; cdb_waits: number
  stall_rob: number; stall_rs: number; stall_serialize: number; forwards: number; loads: number; stores: number; traps: number
}
export interface OooSnap {
  cycle: number; stage: number; pc: number; rf: number[]; rat: (number | null | undefined)[]
  iq: { seq: number; pc: number; instr: number; pred: Prediction; fetched: number }[]
  fetching: { pc: number; instr: number; ready: number }[]
  rob: { tag: number; e: RobEntry }[]; rob_head: number; rob_len: number
  alu_rs: RsEntry[]; mul_rs: RsEntry[]; br_rs: RsEntry[]; lsq: LsqEntry[]; exec: Exec[]
  bht: number[]; btb: BtbEntry[]; stats: OooStats; history: Retired[]
  halted: boolean; serializing: boolean; caches: boolean
}
export type OooEvent =
  | { kind: 'Fetch'; seq: number; pc: number; instr: number; taken: boolean; next: number; btb_hit: boolean; latency: number }
  | { kind: 'Dispatch'; seq: number; rob: number; pc: number; unit: Unit; j_tag?: number | null; k_tag?: number | null }
  | { kind: 'Stall'; reason: 'RobFull' | 'RsFull' | 'LsqFull' | 'Serialize'; unit: Unit }
  | { kind: 'Issue'; seq: number; rob: number; unit: Unit; fu: number }
  | { kind: 'Address'; seq: number; rob: number; addr: number }
  | { kind: 'Forward'; seq: number; rob: number; from_rob: number; addr: number; value: number }
  | { kind: 'MemRead'; seq: number; rob: number; addr: number; latency: number }
  | { kind: 'StoreReady'; seq: number; rob: number; addr: number; data: number }
  | { kind: 'Resolve'; seq: number; rob: number; pc: number; taken: boolean; next: number; mispredict: boolean }
  | { kind: 'Broadcast'; bus: number; seq: number; rob: number; value: number }
  | { kind: 'CdbWait'; seq: number; rob: number }
  | { kind: 'Wakeup'; rob: number; tag: number }
  | { kind: 'Flush'; after_seq: number; squashed: number; pc: number }
  | { kind: 'Commit'; seq: number; rob: number; pc: number; rd: number; value: number }
  | { kind: 'StoreCommit'; seq: number; addr: number; value: number; size: number }
  | { kind: 'Trap'; seq: number; pc: number; cause: number; handler: number }
  | { kind: 'Mret'; seq: number; target: number }
  | { kind: 'Halt'; pc: number }

interface Core {
  examples(): { name: string; about: string }[]
  load_example(i: number, caches: boolean): boolean
  program(): Uint32Array
  step_stage(): number
  step(): void
  run(n: number): boolean
  readonly cycle: number
  readonly halted: boolean
  events(): { events: OooEvent[]; stages: Stage[] }
  snapshot(): OooSnap
}
interface Disasm { disasm(w: number): { mnemonic: string; operands: string } | null }

const hex = (v: number, w = 8) => (v >>> 0).toString(16).padStart(w, '0')

/** One line for the event list */
export function describe(e: OooEvent): string {
  const R = (t: number) => `ROB ${t}`
  switch (e.kind) {
    case 'Fetch': return `fetch ${hex(e.pc)} → predict ${e.taken ? 'taken → ' + hex(e.next) : 'next ' + hex(e.next)}${e.btb_hit ? ' (BTB hit)' : ''} · ${e.latency} cyc`
    case 'Dispatch': return `dispatch ${hex(e.pc)} → ${R(e.rob)}, ${e.unit} RS${e.j_tag != null ? ` · waits ${R(e.j_tag)}` : ''}${e.k_tag != null ? ` · waits ${R(e.k_tag)}` : ''}`
    case 'Stall': return `dispatch stalls: ${{ RobFull: 'ROB full', RsFull: `${e.unit} RS full`, LsqFull: 'LSQ full', Serialize: 'waiting for an empty ROB (CSR / system)' }[e.reason]}`
    case 'Issue': return `issue ${R(e.rob)} → ${e.unit === 'Alu' ? 'ALU ' + e.fu : e.unit === 'Lsu' ? 'address unit' : e.unit === 'Br' ? 'branch unit' : 'MUL'}`
    case 'Address': return `${R(e.rob)} address = ${hex(e.addr)}`
    case 'Forward': return `${R(e.rob)} load ${hex(e.addr)} ← forwarded from store ${R(e.from_rob)}: ${hex(e.value)}`
    case 'MemRead': return `${R(e.rob)} load ${hex(e.addr)} from memory · ${e.latency} cyc`
    case 'StoreReady': return `${R(e.rob)} store ready: [${hex(e.addr)}] ← ${hex(e.data)} (written at commit)`
    case 'Resolve': return `branch ${hex(e.pc)} ${e.taken ? 'taken' : 'not taken'} → ${hex(e.next)}${e.mispredict ? ' · MISPREDICT' : ' · predicted right'}`
    case 'Broadcast': return `CDB ${e.bus}: ${R(e.rob)} = ${hex(e.value)}`
    case 'CdbWait': return `${R(e.rob)} done, both CDBs busy — waits`
    case 'Wakeup': return `${R(e.rob)} took ${R(e.tag)}'s value`
    case 'Flush': return `FLUSH ${e.squashed} younger instruction(s) · fetch from ${hex(e.pc)}`
    case 'Commit': return `commit ${R(e.rob)} ${hex(e.pc)}${e.rd ? ` · x${e.rd} ← ${hex(e.value)}` : ''}`
    case 'StoreCommit': return `store commits: [${hex(e.addr)}] ← ${hex(e.value)}`
    case 'Trap': return `TRAP cause ${e.cause} at ${hex(e.pc)} → handler ${hex(e.handler)}`
    case 'Mret': return `MRET → ${hex(e.target)}`
    case 'Halt': return `j . committed — program done`
  }
}

export const useOoo = defineStore('ooo', () => {
  let core: Core | null = null
  let dis: Disasm | null = null
  const ready = ref(false)
  const error = ref<string | null>(null)
  const examples = ref<{ name: string; about: string }[]>([])
  const exampleIndex = ref(0)
  const caches = ref(false)
  const tick = ref(0)
  const running = ref(false)
  const speed = ref(250)
  /** Stage just run (index into STAGES), or -1 at a cycle boundary */
  const lastStage = ref(-1)
  const program = shallowRef<number[]>([])
  let loading: Promise<void> | null = null

  function init(): Promise<void> {
    if (core) return Promise.resolve()
    if (!loading) loading = (async () => {
      for (let attempt = 1; ; attempt++) {
        try {
          const mod = await import('../wasm/cpu_sim.js')
          await mod.default()
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const m = mod as any
          core = new m.JsOoo(false) as Core
          dis = new m.JsCpuState() as Disasm
          examples.value = core.examples()
          error.value = null
          ready.value = true
          loadExample(exampleIndex.value)
          return
        } catch (e) {
          error.value = String(e)
          if (attempt >= 5) { loading = null; throw e }
          await new Promise(r => setTimeout(r, 1000 * attempt))
        }
      }
    })()
    return loading
  }

  function loadExample(i: number) {
    if (!core) return
    running.value = false
    exampleIndex.value = i
    core.load_example(i, caches.value)
    program.value = Array.from(core.program())
    lastStage.value = -1
    tick.value++
  }
  function setCaches(on: boolean) { caches.value = on; loadExample(exampleIndex.value) }

  /** Run the next stage of the cycle */
  function stepStage() {
    if (!core || core.halted) return
    lastStage.value = core.step_stage()
    tick.value++
  }
  /** Finish the cycle (or run one whole cycle) */
  function stepCycle() {
    if (!core || core.halted) return
    core.step()
    lastStage.value = 6
    tick.value++
  }
  async function toggleRun() {
    if (running.value) { running.value = false; return }
    running.value = true
    while (running.value && core && !core.halted) {
      stepCycle()
      await new Promise(r => setTimeout(r, speed.value))
    }
    running.value = false
  }
  function runToEnd() { if (!core) return; core.run(5000); lastStage.value = 6; tick.value++ }

  function asm(w: number): string {
    const d = dis?.disasm(w >>> 0)
    return d ? `${d.mnemonic} ${d.operands}`.trim().toLowerCase() : hex(w)
  }

  const snap = computed<OooSnap | null>(() => { void tick.value; return core ? core.snapshot() : null })
  const cycleEvents = computed(() => {
    void tick.value
    if (!core) return [] as { e: OooEvent; stage: Stage }[]
    const r = core.events()
    return r.events.map((e, i) => ({ e, stage: r.stages[i]! }))
  })

  return {
    ready, error, examples, exampleIndex, caches, running, speed, lastStage, program, snap, cycleEvents, tick,
    init, loadExample, setCaches, stepStage, stepCycle, toggleRun, runToEnd, asm, hex,
  }
})
