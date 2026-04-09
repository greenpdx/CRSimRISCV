<script setup lang="ts">
//
// MicroStepBarrelView — table view for the 8-thread barrel processor.
// Six pipeline stages (LT/IF/DI/EX/MA/WB), 8 thread cards, scheduler
// mode dropdown, schedule history strip, cache stats.
//

import { computed, onMounted, ref, shallowRef } from 'vue'

const emit = defineEmits<{ 'switch-cpu': [] }>()

interface BCtrl {
  reg_write: boolean
  mem_read: boolean
  mem_write: boolean
  custom_op: number
}
interface BLtIf  { valid: boolean; thread: number; pc: number }
interface BIfDi  { valid: boolean; thread: number; pc: number; instr: number; src: number }
interface BDiEx  {
  valid: boolean; thread: number; pc: number; instr: number;
  rs1_num: number; rs2_num: number; rd_num: number; ctrl: BCtrl;
}
interface BExMa  {
  valid: boolean; thread: number; instr: number;
  alu_result: number; rd_num: number;
  branch_taken: boolean; branch_target: number; ctrl: BCtrl;
}
interface BMaWb  {
  valid: boolean; thread: number; instr: number;
  alu_result: number; mem_data: number; rd_num: number; ctrl: BCtrl;
}
interface BThread {
  pc: number
  regs: number[]
  active: boolean
  waiting: number
  priority: number
  age: number
  miss_pending: boolean
  instr_access_fault: boolean
  priv_mode: number
  mstatus: number
  mepc: number
  mcause: number
  mtval: number
  mscratch: number
  satp: number
}
interface BCache { hits: bigint | number; misses: bigint | number; busy: boolean }
interface BSnapshot {
  cycle: bigint
  halted: boolean
  sched_mode: number
  current_thread: number
  mtvec: number
  mie: number
  mip: number
  threads: BThread[]
  lt_if: BLtIf
  if_di: BIfDi
  di_ex: BDiEx
  ex_ma: BExMa
  ma_wb: BMaWb
  last_wb: BMaWb
  sched_history: number[]
  icache: BCache
  dcache: BCache
  store_buf_count: number
  bus: { state: number }
}

interface DisasmJson { class: string; mnemonic: string; operands: string; desc: string }

type Cpu = {
  reset(): void
  step(): void
  step_n(n: number): void
  load_program(words: Uint32Array): void
  load_isram(slot: number, words: Uint32Array): void
  prewarm_icache(): void
  set_thread_active(slot: number, active: boolean): void
  set_thread_pc(slot: number, pc: number): void
  set_thread_priority(slot: number, prio: number): void
  activate_all(): void
  set_sched_mode(m: number): void
  inject_irq(slot: number): void
  read_csr(slot: number, addr: number): number
  write_csr(slot: number, addr: number, val: number): void
  read_isram_word(slot: number, byte_off: number): number
  ext_imem(): Uint32Array
  ext_dmem(): Uint32Array
  snapshot(): BSnapshot
}
type Disasmer = { disasm(instr: number): DisasmJson }

const cpu = shallowRef<Cpu | null>(null)
const dis = shallowRef<Disasmer | null>(null)
const romCache = ref<number[]>([])
const ramCache = computed<number[]>(() => {
  renderTick.value
  if (!cpu.value) return []
  return Array.from(cpu.value.ext_dmem())
})
const ready = ref(false)
const renderTick = ref(0)

const REG_NAMES = [
  'zero','ra','sp','gp','tp','t0','t1','t2','s0','s1',
  'a0','a1','a2','a3','a4','a5','a6','a7','s2','s3','s4','s5','s6','s7',
  's8','s9','s10','s11','t3','t4','t5','t6',
]
const hex = (n: number, w = 8) => '0x' + (n >>> 0).toString(16).padStart(w, '0')

// ── Instruction encoders ───────────────────────────────────────
const NOP = 0x00000013
function enc_addi(rd: number, rs1: number, imm: number) {
  return (((imm & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13) >>> 0
}
function enc_lui(rd: number, imm: number) {
  return ((imm << 12) | (rd << 7) | 0x37) >>> 0
}
function enc_sw(rs1: number, rs2: number, off: number) {
  const imm = off & 0xfff
  return ((((imm >> 5) & 0x7f) << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | (((imm) & 0x1f) << 7) | 0x23) >>> 0
}
function enc_fork(rd: number, rs1: number, rs2: number) {
  return ((0x00 << 25) | (rs2 << 20) | (rs1 << 15) | (rd << 7) | 0x0b) >>> 0
}
function enc_done() { return ((0x02 << 25) | 0x0b) >>> 0 }
const JAL_SPIN = 0x0000006f
const JAL_BACK4 = 0xffdff06f  // jal x0, -4

// DMA register addresses
const DMA_BASE = 0x40001000

function dmaSeq(srcByte: number, dstSlot: number, count: number): number[] {
  return [
    enc_lui(28, 0x40001),           // t3 = DMA_BASE
    enc_addi(29, 0, srcByte),       // t4 = src
    enc_sw(28, 29, 0x00),           // DMA_SRC
    enc_addi(29, 0, dstSlot),       // t4 = slot
    enc_sw(28, 29, 0x04),           // DMA_DST
    enc_addi(29, 0, count),         // t4 = count
    enc_sw(28, 29, 0x08),           // DMA_COUNT
    enc_sw(28, 0, 0x0C),            // DMA_TRIGGER
  ]
}

// Boot demo: T0 inits, DMAs scratchpads, FORKs threads.
function buildDemo(): Uint32Array {
  const rom: number[] = new Array(256).fill(NOP)

  // Handler code for slot 1 at ext ROM word 64 = byte 0x100
  rom[64] = enc_addi(6, 0, 0x11)   // x6 = 0x11 marker
  rom[65] = enc_addi(5, 5, 1)      // x5++ loop
  rom[66] = JAL_BACK4

  // Handler code for slot 2 at ext ROM word 80 = byte 0x140
  rom[80] = enc_addi(6, 0, 0x22)
  rom[81] = enc_addi(5, 5, 1)
  rom[82] = JAL_BACK4

  // Worker code for slots 3..7 at ext ROM word 128 = byte 0x200
  rom[128] = enc_addi(5, 5, 1)
  rom[129] = JAL_BACK4

  // ── T0 boot code at 0x000 ──
  const boot: number[] = []

  // 1. Global init (placeholder)
  boot.push(NOP)

  // 3. DMA slot 1 I-SRAM from ext ROM @ 0x100
  boot.push(...dmaSeq(0x100, 1, 3))

  // 4. DMA slot 2 I-SRAM from ext ROM @ 0x140
  boot.push(...dmaSeq(0x140, 2, 3))

  // 5. Wait (instant DMA, just a NOP)
  boot.push(NOP)

  // 6. FORK pinned slots 1 and 2
  boot.push(enc_lui(10, 0xF0004))        // a0 = 0xF0004000 (slot 1 ISRAM)
  boot.push(enc_addi(11, 0, 0))
  boot.push(enc_fork(5, 10, 11))

  boot.push(enc_lui(10, 0xF0008))        // a0 = 0xF0008000 (slot 2 ISRAM)
  boot.push(enc_addi(11, 0, 0))
  boot.push(enc_fork(5, 10, 11))

  // 2. DMA slot 0 I-SRAM (T0's own — skipped in this demo)

  // 8. Enable IRQs (placeholder)
  boot.push(NOP)

  // 7. FORK general workers 3..7 at ext ROM 0x200
  for (let s = 3; s <= 7; s++) {
    boot.push(enc_addi(10, 0, 0x200))
    boot.push(enc_addi(11, 0, s))        // arg = thread index
    boot.push(enc_fork(5, 10, 11))
  }

  // 9. T0 workload: increment counter loop
  boot.push(enc_addi(5, 5, 1))
  boot.push(JAL_BACK4)

  for (let i = 0; i < boot.length && i < rom.length; i++) rom[i] = boot[i]
  return new Uint32Array(rom)
}
const program = buildDemo()

onMounted(async () => {
  const mod = await import('../wasm/cpu_sim.js')
  await mod.default()
  const c = new (mod as any).JsBarrelState() as Cpu
  c.load_program(program)
  c.prewarm_icache()
  // Boot mode: only T0 active at pc=0.
  c.set_thread_active(0, true)
  c.set_thread_pc(0, 0)
  c.set_thread_priority(0, 3)
  cpu.value = c
  romCache.value = Array.from(c.ext_imem())
  const d = new (mod as any).JsCpuState() as unknown as Disasmer
  dis.value = d
  ready.value = true
})

function step()    { cpu.value?.step(); renderTick.value++ }
function step8()   { cpu.value?.step_n(8); renderTick.value++ }
function step32()  { cpu.value?.step_n(32); renderTick.value++ }
function reset()   {
  if (!cpu.value) return
  cpu.value.reset()
  cpu.value.load_program(program)
  cpu.value.prewarm_icache()
  cpu.value.set_thread_active(0, true)
  cpu.value.set_thread_pc(0, 0)
  cpu.value.set_thread_priority(0, 3)
  romCache.value = Array.from(cpu.value.ext_imem())
  renderTick.value++
}

const snap = computed<BSnapshot | null>(() => {
  renderTick.value
  if (!cpu.value) return null
  return cpu.value.snapshot()
})

function disasmShort(instr: number): string {
  if (!dis.value || instr === 0) return ''
  const d = dis.value.disasm(instr >>> 0)
  return `${d.mnemonic.padEnd(6)} ${d.operands}`
}

const SCHED_NAMES = ['RoundRobin', 'Priority', 'AgedPriority', 'TwoClass']

function setSchedMode(e: Event) {
  if (!cpu.value) return
  const v = Number((e.target as HTMLSelectElement).value)
  cpu.value.set_sched_mode(v)
  renderTick.value++
}

function injectIrq(slot: number) {
  if (!cpu.value) return
  cpu.value.inject_irq(slot)
  renderTick.value++
}

const selectedThread = ref<number>(0)
function selectThread(s: number) { selectedThread.value = s }

// I-SRAM address constants (must match Rust state.rs)
const ISRAM_BASE = 0xF0000000
const ISRAM_BYTES_PER_SLOT = 0x4000
const ISRAM_TOP = ISRAM_BASE + 3 * ISRAM_BYTES_PER_SLOT

function isramSlotOf(pc: number): number | null {
  if (pc >= ISRAM_BASE && pc < ISRAM_TOP) {
    return Math.floor((pc - ISRAM_BASE) / ISRAM_BYTES_PER_SLOT)
  }
  return null
}

// Read one instruction word, routing between I-SRAM and ext ROM.
function readInstr(addr: number): number {
  const slot = isramSlotOf(addr)
  if (slot !== null && cpu.value) {
    const off = addr - ISRAM_BASE - slot * ISRAM_BYTES_PER_SLOT
    return cpu.value.read_isram_word(slot, off)
  }
  const idx = addr >>> 2
  return romCache.value[idx] ?? 0
}

// Instructions around a thread's PC (context window of 8 lines).
function instrWindow(pc: number): { addr: number; word: number; disasm: string; isPC: boolean }[] {
  const slot = isramSlotOf(pc)
  const rows = []
  if (slot !== null) {
    // PC is in I-SRAM range — show I-SRAM content around this offset.
    const base = ISRAM_BASE + slot * ISRAM_BYTES_PER_SLOT
    const off = pc - base
    const wordOff = off >>> 2
    const start = Math.max(0, wordOff - 3)
    const end = Math.min(ISRAM_BYTES_PER_SLOT / 4, start + 8)
    for (let i = start; i < end; i++) {
      const addr = base + i * 4
      const w = readInstr(addr)
      rows.push({ addr, word: w, disasm: disasmShort(w), isPC: addr === pc })
    }
  } else {
    // PC is in external ROM.
    const wordIdx = pc >>> 2
    const start = Math.max(0, wordIdx - 3)
    const end = Math.min(romCache.value.length, start + 8)
    for (let i = start; i < end; i++) {
      const w = romCache.value[i] ?? 0
      rows.push({ addr: i * 4, word: w, disasm: disasmShort(w), isPC: i * 4 === pc })
    }
  }
  return rows
}

const CSR_DISPLAY: Array<{ addr: number; name: string }> = [
  { addr: 0x300, name: 'mstatus'  },
  { addr: 0x305, name: 'mtvec'    },
  { addr: 0x341, name: 'mepc'     },
  { addr: 0x342, name: 'mcause'   },
  { addr: 0x343, name: 'mtval'    },
  { addr: 0x344, name: 'mip'      },
  { addr: 0x304, name: 'mie'      },
  { addr: 0x340, name: 'mscratch' },
  { addr: 0x180, name: 'satp'     },
]
function csrValForSelected(addr: number): number {
  if (!snap.value) return 0
  const t = snap.value.threads[selectedThread.value]
  switch (addr) {
    case 0x300: return t.mstatus
    case 0x305: return t.mtvec
    case 0x341: return t.mepc
    case 0x342: return t.mcause
    case 0x343: return t.mtval
    case 0x344: return t.mip
    case 0x304: return t.mie
    case 0x340: return t.mscratch
    case 0x180: return t.satp
  }
  return 0
}

// 8 distinct colors for thread tags
const THREAD_COLORS = [
  '#e74c3c', '#3498db', '#2ecc71', '#f39c12',
  '#9b59b6', '#1abc9c', '#e67e22', '#34495e',
]
function threadColor(t: number) { return THREAD_COLORS[t & 7] }

// Helpers for stage cards
function stageLabel(r: { valid: boolean; instr: number; thread: number }): string {
  if (!r.valid) return '— bubble —'
  return disasmShort(r.instr)
}
function stageBadge(r: { valid: boolean; thread: number }) {
  if (!r.valid) return null
  return r.thread
}
</script>

<template>
  <div class="bv-root">
    <h1 class="title-row">
      <button class="back-btn" @click="emit('switch-cpu')">← CpuLayout</button>
      <span class="bv-name">8-thread barrel</span>
      <template v-if="ready && snap">
        <span class="pill">cycle {{ Number(snap.cycle) }}</span>
        <span class="pill">cur T{{ snap.current_thread }}</span>
        <label class="sched-sel">
          sched:
          <select :value="snap.sched_mode" @change="setSchedMode">
            <option v-for="(name, i) in SCHED_NAMES" :key="i" :value="i">{{ name }}</option>
          </select>
        </label>
        <span class="pill">i$ h{{ Number(snap.icache.hits) }}/m{{ Number(snap.icache.misses) }}</span>
        <span class="pill">d$ h{{ Number(snap.dcache.hits) }}/m{{ Number(snap.dcache.misses) }}</span>
        <span class="pill">stbuf {{ snap.store_buf_count }}/4</span>
      </template>
    </h1>

    <div class="bar">
      <button class="btn alt" @click="step">Step</button>
      <button class="btn"     @click="step8">Step 8</button>
      <button class="btn"     @click="step32">Step 32</button>
      <button class="btn"     @click="reset">Reset</button>
      <span class="bar-gap"></span>
      <button class="btn btn-irq" @click="injectIrq(0)" title="Activate slot 0 IRQ handler">IRQ T0</button>
      <button class="btn btn-irq" @click="injectIrq(1)" title="Activate slot 1 IRQ handler">IRQ T1</button>
      <button class="btn btn-irq" @click="injectIrq(2)" title="Activate slot 2 IRQ handler">IRQ T2</button>
    </div>

    <template v-if="ready && snap">
      <!-- Global CSRs (shared across all threads) -->
      <div class="csr-strip">
        <span class="csr-label">Global:</span>
        <span class="csr-cell"><span class="csr-name">mtvec</span><span class="csr-val">{{ hex(snap.mtvec) }}</span></span>
        <span class="csr-cell"><span class="csr-name">mip</span><span class="csr-val">{{ hex(snap.mip) }}</span></span>
        <span class="csr-cell"><span class="csr-name">mie</span><span class="csr-val">{{ hex(snap.mie) }}</span></span>
      </div>

      <!-- Pipeline stages, 6 cards -->
      <div class="stage-row">
        <div class="stage-card" v-for="(s, i) in [
            { name: 'LT', r: { valid: snap.lt_if.valid, thread: snap.lt_if.thread, instr: 0 } },
            { name: 'IF', r: snap.if_di },
            { name: 'DI', r: snap.di_ex },
            { name: 'EX', r: snap.ex_ma },
            { name: 'MA', r: snap.ma_wb },
            { name: 'WB', r: snap.last_wb },
          ]" :key="i" :class="{ 'stage-empty': !s.r.valid }">
          <h3>{{ s.name }}</h3>
          <span class="thread-badge" v-if="s.r.valid"
                :style="{ background: threadColor(s.r.thread) }">T{{ s.r.thread }}</span>
          <div class="stage-instr">{{ s.r.valid ? (s.name === 'LT' ? hex(snap.lt_if.pc, 4) : disasmShort((s.r as any).instr)) : '— bubble —' }}</div>
        </div>
      </div>

      <!-- Schedule history strip -->
      <div class="hist-strip">
        <span class="hist-label">last 32:</span>
        <span v-for="(t, i) in snap.sched_history" :key="i"
              class="hist-cell"
              :style="{ background: t === 255 ? '#222' : threadColor(t),
                        color: t === 255 ? '#666' : '#fff' }">
          {{ t === 255 ? '·' : t }}
        </span>
      </div>

      <!-- 8 thread columns -->
      <div class="thread-grid">
        <div v-for="(t, i) in snap.threads" :key="i"
             class="thread-card"
             :class="{ inactive: !t.active, current: i === snap.current_thread, selected: i === selectedThread }"
             :style="{ borderColor: threadColor(i) }"
             @click="selectThread(i)">
          <div class="thread-head">
            <span class="thread-id" :style="{ background: threadColor(i) }">T{{ i }}</span>
            <span class="thread-status">
              <span v-if="!t.active">idle</span>
              <span v-else-if="t.waiting >= 0">join({{ t.waiting }})</span>
              <span v-else-if="t.miss_pending">miss</span>
              <span v-else-if="t.instr_access_fault">fault</span>
              <span v-else>run</span>
            </span>
            <span class="thread-pri">p{{ t.priority }}</span>
          </div>
          <div class="thread-row"><span class="k">pc</span><span class="v">{{ hex(t.pc, 4) }}</span></div>

          <!-- Register file: 8 visible rows, scrollable -->
          <div class="rbox">
            <div v-for="r in 32" :key="r-1" class="rrow">
              <span class="rname">x{{ r-1 }} {{ REG_NAMES[r-1] }}</span>
              <span class="rval">{{ hex(t.regs[r-1]) }}</span>
            </div>
          </div>

          <!-- RAM (shared ext_dmem): 8 visible rows, scrollable -->
          <div class="mbox">
            <div v-for="(v, j) in ramCache" :key="j" class="mrow" v-show="v !== 0 || j < 8">
              <span class="maddr">{{ hex(0x10000000 + j * 4, 4) }}</span>
              <span class="mval">{{ hex(v) }}</span>
            </div>
          </div>

          <!-- Instruction window (ROM): 8 visible rows, scrollable -->
          <div class="ibox">
            <div v-for="row in instrWindow(t.pc)" :key="row.addr"
                 class="irow" :class="{ ipc: row.isPC }">
              <span class="iaddr">{{ hex(row.addr, 3) }}</span>
              <span class="idis">{{ row.disasm }}</span>
            </div>
          </div>

          <!-- Per-thread CSRs -->
          <div class="tcsr">
            <div class="tcsr-row"><span class="tcsr-n">priv</span><span class="tcsr-v">{{ t.priv_mode === 3 ? 'M' : t.priv_mode === 1 ? 'S' : 'U' }}</span></div>
            <div class="tcsr-row"><span class="tcsr-n">mstatus</span><span class="tcsr-v">{{ hex(t.mstatus) }}</span></div>
            <div class="tcsr-row"><span class="tcsr-n">mepc</span><span class="tcsr-v">{{ hex(t.mepc) }}</span></div>
            <div class="tcsr-row"><span class="tcsr-n">mcause</span><span class="tcsr-v">{{ hex(t.mcause) }}</span></div>
            <div class="tcsr-row"><span class="tcsr-n">mtval</span><span class="tcsr-v">{{ hex(t.mtval) }}</span></div>
            <div class="tcsr-row"><span class="tcsr-n">mscratch</span><span class="tcsr-v">{{ hex(t.mscratch) }}</span></div>
            <div class="tcsr-row"><span class="tcsr-n">satp</span><span class="tcsr-v">{{ hex(t.satp) }}</span></div>
          </div>
        </div>
      </div>

    </template>
    <p v-else style="padding:2rem; color:#888;">Loading wasm…</p>
  </div>
</template>

<style scoped>
.bv-root {
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  background: #1e1e1e; color: #ddd; padding: 1rem;
  font-size: 12px; box-sizing: border-box;
  position: fixed; inset: 0;
  display: flex; flex-direction: column; overflow: auto;
}
.title-row { display: flex; align-items: center; gap: .4rem; margin: 0 0 .8rem; font-size: 1.1rem; color: #4ec3d5; flex-wrap: wrap; }
.back-btn { background: #444; color: #fff; border: 0; padding: .35rem .7rem; cursor: pointer; border-radius: 3px; font: inherit; }
.bv-name { margin-right: .6rem; }
.pill { background: #2a2a4a; color: #cce; padding: .25rem .55rem; border-radius: 3px; font-size: .75rem; }
.sched-sel { font-size: .75rem; color: #cce; display: inline-flex; align-items: center; gap: .25rem; }
.sched-sel select { background: #1a1a2e; color: #cce; border: 1px solid #4a4a6a; font: inherit; font-size: .7rem; padding: 1px 4px; border-radius: 3px; }

.bar { display: flex; align-items: center; gap: .4rem; margin-bottom: .3rem; }
.btn { background: #2aacbe; color: #000; border: 0; padding: .4rem .8rem; cursor: pointer; font: inherit; border-radius: 3px; }
.btn:hover { background: #4ec3d5; }
.btn.alt { background: #cc8800; }

.stage-row { display: grid; grid-template-columns: repeat(6, 1fr); gap: .5rem; margin-bottom: .8rem; }
.stage-card {
  background: #111; border: 1px solid #333; border-radius: 4px;
  padding: .55rem .65rem; min-height: 70px;
  position: relative;
}
.stage-card.stage-empty { opacity: .5; }
.stage-card h3 { margin: 0 0 .35rem; color: #4ec3d5; font-size: 1rem; }
.thread-badge {
  position: absolute; top: .35rem; right: .35rem;
  color: #fff; font-size: .65rem; padding: 1px 5px; border-radius: 2px; font-weight: bold;
}
.stage-instr { color: #ffd84d; font-size: .85rem; font-weight: bold;
  font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

.hist-strip { display: flex; align-items: center; gap: 2px; margin-bottom: .8rem; }
.hist-label { color: #888; font-size: .75rem; margin-right: .35rem; }
.hist-cell {
  display: inline-block; width: 18px; height: 18px;
  text-align: center; line-height: 18px; font-size: 10px;
  font-weight: bold; border-radius: 2px;
}

.thread-grid { display: grid; grid-template-columns: repeat(8, 1fr); gap: .35rem; }
.thread-card {
  background: #111; border: 2px solid #333; border-radius: 4px;
  padding: .45rem .55rem; font-size: .75rem;
}
.thread-card.inactive { opacity: .4; }
.thread-card.current { box-shadow: 0 0 8px currentColor; }
.thread-head { display: flex; align-items: center; gap: .4rem; margin-bottom: .35rem; }
.thread-id { color: #fff; font-weight: bold; padding: 1px 6px; border-radius: 3px; font-size: .75rem; }
.thread-status { color: #aaa; font-size: .7rem; }
.thread-pri { color: #888; font-size: .7rem; margin-left: auto; }
.thread-row { display: flex; justify-content: space-between; padding: 1px 0; }
.thread-row .k { color: #0099cc; }
.thread-row .v { color: #fff; font-variant-numeric: tabular-nums; }
.thread-regs { display: grid; grid-template-columns: 1fr 1fr; gap: 0 6px; margin-top: .25rem; }
.reg-mini { display: flex; justify-content: space-between; font-size: .65rem; }
.rname { color: #888; }
.rval { color: #fff; font-variant-numeric: tabular-nums; }

.thread-card.selected { box-shadow: 0 0 0 2px #4ec3d5 inset; }
.thread-card { cursor: pointer; }

.bar-gap { display: inline-block; width: 1.5rem; }
.btn-irq { background: #c0392b; color: #fff; }
.btn-irq:hover { background: #e74c3c; }

/* RAM (shared data memory) per thread */
.mbox {
  height: calc(8 * 1.15em);
  overflow-y: auto; overflow-x: hidden;
  background: #0a0a0a; border: 1px solid #2a2a2a;
  border-radius: 2px; margin: .2rem 0;
  font-size: .6rem; line-height: 1.15;
}
.mrow { display: flex; gap: 3px; padding: 0 2px; white-space: nowrap; }
.maddr { color: #cc8800; min-width: 3.5em; }
.mval  { color: #fff; font-variant-numeric: tabular-nums; }

/* Instruction window per thread */
.ibox {
  height: calc(8 * 1.15em);
  overflow-y: auto; overflow-x: hidden;
  background: #0a0a0a; border: 1px solid #2a2a2a;
  border-radius: 2px; margin: .2rem 0;
  font-size: .6rem; line-height: 1.15;
}
.irow { display: flex; gap: 3px; padding: 0 2px; white-space: nowrap; }
.irow.ipc { background: #2a4a2a; color: #00ff66; }
.iaddr { color: #555; min-width: 3.5em; }
.idis  { color: #ccc; overflow: hidden; text-overflow: ellipsis; }

/* Register file per thread */
.rbox {
  height: calc(8 * 1.15em);
  overflow-y: auto; overflow-x: hidden;
  background: #0a0a0a; border: 1px solid #2a2a2a;
  border-radius: 2px; margin-top: .2rem;
  font-size: .6rem; line-height: 1.15;
}
.rrow { display: flex; justify-content: space-between; padding: 0 2px; white-space: nowrap; }

/* Per-thread CSR block (below ROM in each card) */
.tcsr { margin-top: .2rem; font-size: .6rem; line-height: 1.15; }
.tcsr-row { display: flex; justify-content: space-between; padding: 0 2px; }
.tcsr-n { color: #c080e0; }
.tcsr-v { color: #ffd84d; font-variant-numeric: tabular-nums; }

.csr-strip {
  display: flex; align-items: center; gap: 4px; margin-top: 0; margin-bottom: .4rem;
  flex-wrap: wrap;
}
.csr-label { color: #888; font-size: .75rem; margin-right: .35rem; }
.csr-cell {
  display: inline-flex; align-items: center; gap: 4px;
  background: #15101a; border: 1px solid #3a2050;
  padding: 2px 6px; font-size: .7rem;
  font-variant-numeric: tabular-nums;
}
.csr-name { color: #aaa; }
.csr-val  { color: #ffd84d; }
</style>
