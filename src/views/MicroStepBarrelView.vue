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
  mtvec: number
  mepc: number
  mcause: number
  mtval: number
  mscratch: number
  mip: number
  mie: number
  satp: number
}
interface BCache { hits: bigint | number; misses: bigint | number; busy: boolean }
interface BSnapshot {
  cycle: bigint
  halted: boolean
  sched_mode: number
  current_thread: number
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
  snapshot(): BSnapshot
}
type Disasmer = { disasm(instr: number): DisasmJson }

const cpu = shallowRef<Cpu | null>(null)
const dis = shallowRef<Disasmer | null>(null)
const ready = ref(false)
const renderTick = ref(0)

const REG_NAMES = [
  'zero','ra','sp','gp','tp','t0','t1','t2','s0','s1',
  'a0','a1','a2','a3','a4','a5','a6','a7','s2','s3','s4','s5','s6','s7',
  's8','s9','s10','s11','t3','t4','t5','t6',
]
const hex = (n: number, w = 8) => '0x' + (n >>> 0).toString(16).padStart(w, '0')

// Default barrel demo program: 8 threads each running a small loop.
function buildDemo(): Uint32Array {
  const NOP = 0x00000013
  const rom: number[] = new Array(64).fill(NOP)
  // Each thread starts at byte (slot * 0x10) and runs:
  //   addi tN, tN, 1     ; bump its own counter
  //   jal  x0, -4        ; loop back
  // tN = x5 + slot for slots 0..2, but to keep things simple
  // every thread uses x5.
  const jal_back = 0xffdff06f // jal x0, -4
  for (let s = 0; s < 8; s++) {
    const base = s * 4
    rom[base + 0] = (1 << 20) | (5 << 15) | (0 << 12) | (5 << 7) | 0x13 // addi x5, x5, 1
    rom[base + 1] = jal_back
  }
  return new Uint32Array(rom)
}
const program = buildDemo()

onMounted(async () => {
  const mod = await import('../wasm/cpu_sim.js')
  await mod.default()
  const c = new (mod as any).JsBarrelState() as Cpu
  c.load_program(program)
  c.prewarm_icache()
  c.activate_all()
  for (let s = 0; s < 8; s++) c.set_thread_pc(s, s * 0x10)
  cpu.value = c
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
  cpu.value.activate_all()
  for (let s = 0; s < 8; s++) cpu.value.set_thread_pc(s, s * 0x10)
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
    </div>

    <template v-if="ready && snap">
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

      <!-- 8 thread cards -->
      <div class="thread-grid">
        <div v-for="(t, i) in snap.threads" :key="i"
             class="thread-card"
             :class="{ inactive: !t.active, current: i === snap.current_thread, selected: i === selectedThread }"
             :style="{ borderColor: threadColor(i) }"
             @click="selectThread(i)">
          <div class="thread-head">
            <span class="thread-id" :style="{ background: threadColor(i) }">T{{ i }}</span>
            <span class="thread-status">
              <span v-if="!t.active">●idle</span>
              <span v-else-if="t.waiting >= 0">⏸join({{ t.waiting }})</span>
              <span v-else-if="t.miss_pending">⏳miss</span>
              <span v-else-if="t.instr_access_fault">⚠fault</span>
              <span v-else>●run</span>
            </span>
            <span class="thread-pri">pri {{ t.priority }}</span>
            <button class="irq-btn" @click.stop="injectIrq(i)" title="Inject IRQ on this slot">IRQ</button>
          </div>
          <div class="thread-row"><span class="k">pc</span><span class="v">{{ hex(t.pc) }}</span></div>
          <div class="thread-row"><span class="k">age</span><span class="v">{{ t.age }}</span></div>
          <div class="thread-regs">
            <div v-for="r in [5,6,7,8,10,18]" :key="r"
                 class="reg-mini" v-show="t.regs[r] !== 0">
              <span class="rname">{{ REG_NAMES[r] }}</span>
              <span class="rval">{{ hex(t.regs[r]) }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Per-thread CSR strip (selected thread) -->
      <div class="csr-strip">
        <span class="csr-label">CSRs T{{ selectedThread }}:</span>
        <span v-for="c in CSR_DISPLAY" :key="c.addr" class="csr-cell"
              :title="`addr 0x${c.addr.toString(16)}`">
          <span class="csr-name">{{ c.name }}</span>
          <span class="csr-val">{{ hex(csrValForSelected(c.addr)) }}</span>
        </span>
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

.bar { display: flex; align-items: center; gap: .4rem; margin-bottom: .8rem; }
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

.thread-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: .5rem; }
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
.irq-btn {
  background: #c0392b; color: #fff; border: 0;
  padding: 1px 6px; border-radius: 2px; font: inherit;
  font-size: .65rem; cursor: pointer; margin-left: auto;
}
.irq-btn:hover { background: #e74c3c; }

.csr-strip {
  display: flex; align-items: center; gap: 4px; margin-top: .8rem;
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
