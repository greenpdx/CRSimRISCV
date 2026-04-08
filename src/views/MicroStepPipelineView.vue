<script setup lang="ts">
//
// MicroStepPipelineView — table view for the 5-stage pipeline simulator.
// Loads JsPipelineState from the wasm package and renders one column per
// stage (IF/ID/EX/MA/WB) showing the pipeline register contents and the
// active components, plus registers/RAM/ROM panels on the side.
//

import { computed, onMounted, ref, shallowRef } from 'vue'

const emit = defineEmits<{ 'switch-cpu': [] }>()

interface CtrlJson {
  reg_write: boolean; alu_src: boolean; alu_op: number;
  mem_read: boolean; mem_write: boolean;
  wb_sel: number; pc_sel: number; branch: boolean; jump: boolean;
}
interface IfId  { valid: boolean; pc: number; instr: number; pc_plus4: number }
interface IdEx  {
  valid: boolean; pc: number; pc_plus4: number; instr: number;
  rs1_data: number; rs2_data: number; imm: number;
  rs1_num: number; rs2_num: number; rd_num: number;
  funct3: number; funct7b30: number; opcode: number;
  ctrl: CtrlJson;
}
interface ExMa {
  valid: boolean; instr: number;
  alu_result: number; rs2_data: number; pc_plus4: number;
  rd_num: number; funct3: number;
  branch_taken: boolean; branch_target: number;
  ctrl: CtrlJson;
}
interface MaWb {
  valid: boolean; instr: number;
  alu_result: number; mem_data: number; pc_plus4: number;
  rd_num: number; ctrl: CtrlJson;
}
interface Snapshot {
  cycle: bigint;
  pc: number;
  halted: boolean;
  registers: Uint32Array;
  if_id: IfId;
  id_ex: IdEx;
  ex_ma: ExMa;
  ma_wb: MaWb;
  last_wb: MaWb;
  hazard:  { stall: boolean; flush: boolean };
  forward: { a: string; b: string };
  active_components: { IF: string[]; ID: string[]; EX: string[]; MA: string[]; WB: string[] };
}

interface DisasmJson { class: string; mnemonic: string; operands: string; desc: string }

type Cpu = {
  pc: number
  cycle: bigint
  halted: boolean
  reset(): void
  load_program(words: Uint32Array): void
  step(): void
  read_register(idx: number): number
  registers(): Uint32Array
  ram(): Uint32Array
  rom(): Uint32Array
  if_id(): IfId
  id_ex(): IdEx
  ex_ma(): ExMa
  ma_wb(): MaWb
  snapshot(): Snapshot
  poke_register(idx: number, val: number): void
  poke_ram(addr: number, val: number): void
  poke_rom(addr: number, val: number): void
  // The wasm module also exports JsCpuState which has disasm — we use that.
}

type Disasmer = { disasm(instr: number): DisasmJson }

const cpu  = shallowRef<Cpu | null>(null)
const dis  = shallowRef<Disasmer | null>(null)
const ready = ref(false)
const renderTick = ref(0)

const REG_NAMES = [
  'zero','ra','sp','gp','tp','t0','t1','t2','s0','s1',
  'a0','a1','a2','a3','a4','a5','a6','a7','s2','s3','s4','s5','s6','s7',
  's8','s9','s10','s11','t3','t4','t5','t6',
]

const hex = (n: number, w = 8) => '0x' + (n >>> 0).toString(16).padStart(w, '0')

// Default program: covers ALU, RAW hazard, load-use, branch, function call.
const program = new Uint32Array([
  0x00500293, // addi x5, x0, 5
  0x00128313, // addi x6, x5, 1            (RAW: forwards x5)
  0x000102b7, // lui  x5, 0x10             (x5 = 0x10000)
  0x0062a023, // sw   x6, 0(x5)            (MEM[0x10000] = 6)
  0x0002a383, // lw   x7, 0(x5)            (load)
  0x00738433, // add  x8, x7, x7           (load-use → bubble)
  0x00540863, // beq  x8, x5, 16           (12 != 0x10000, not taken)
  0x02a00913, // addi s2, x0, 42           (executes; x18=42)
  0x0000006f, // jal  x0, 0                (spin)
])

onMounted(async () => {
  const mod = await import('../wasm/cpu_sim.js')
  await mod.default()
  const c = new mod.JsPipelineState() as Cpu
  c.load_program(program)
  cpu.value = c
  // Reuse the JsCpuState's disasm helper for ROM display
  const d = new mod.JsCpuState() as unknown as Disasmer
  dis.value = d
  ready.value = true
})

function step()    { cpu.value?.step(); renderTick.value++ }
function step5()   { for (let i = 0; i < 5; i++) cpu.value?.step(); renderTick.value++ }
function step20()  { for (let i = 0; i < 20; i++) cpu.value?.step(); renderTick.value++ }
function reset()   {
  if (!cpu.value) return
  cpu.value.reset()
  cpu.value.load_program(program)
  renderTick.value++
}

const snap = computed<Snapshot | null>(() => {
  renderTick.value
  if (!cpu.value) return null
  return cpu.value.snapshot()
})

const regList = computed<number[]>(() => {
  renderTick.value
  return cpu.value ? Array.from(cpu.value.registers()) : []
})
const ramList = computed<number[]>(() => {
  renderTick.value
  return cpu.value ? Array.from(cpu.value.ram()) : []
})
const romList = computed<number[]>(() => {
  renderTick.value
  return cpu.value ? Array.from(cpu.value.rom()) : []
})

function disasmShort(instr: number): string {
  if (!dis.value || instr === 0) return ''
  const d = dis.value.disasm(instr >>> 0)
  return `${d.mnemonic.padEnd(6)} ${d.operands}`
}

function disasmFull(instr: number): DisasmJson | null {
  if (!dis.value || instr === 0) return null
  return dis.value.disasm(instr >>> 0)
}

function edit(label: string, current: number, write: (n: number) => void) {
  const v = window.prompt(`new value for ${label} (hex or dec)`, hex(current))
  if (v === null) return
  const n = v.startsWith('0x') ? parseInt(v.slice(2), 16) : Number(v)
  if (Number.isNaN(n)) return
  write(n >>> 0)
  renderTick.value++
}

// Format helpers for the stage cards
function ctrlPretty(c: CtrlJson): string[] {
  return [
    `regWrite=${c.reg_write ? 1 : 0}`,
    `aluSrc=${c.alu_src ? 1 : 0}`,
    `aluOp=${c.alu_op}`,
    `memRead=${c.mem_read ? 1 : 0}`,
    `memWrite=${c.mem_write ? 1 : 0}`,
    `wbSel=${c.wb_sel}`,
    `branch=${c.branch ? 1 : 0}`,
    `jump=${c.jump ? 1 : 0}`,
  ]
}
</script>

<template>
  <div class="ms-root">
    <h1 class="title-row">
      <button class="back-btn" @click="emit('switch-cpu')">← CpuLayout</button>
      <span class="ms-name">5-stage pipeline</span>
      <template v-if="ready && snap">
        <span class="pill">cycle {{ Number(snap.cycle) }}</span>
        <span class="pill">pc {{ hex(snap.pc) }}</span>
        <span class="pill" :class="snap.hazard.stall ? 'pill-warn' : ''">
          stall: {{ snap.hazard.stall ? 'yes' : 'no' }}
        </span>
        <span class="pill" :class="snap.hazard.flush ? 'pill-warn' : ''">
          flush: {{ snap.hazard.flush ? 'yes' : 'no' }}
        </span>
        <span class="pill" :class="snap.forward.a !== 'regfile' ? 'pill-fwd' : ''">
          fwd_a: {{ snap.forward.a }}
        </span>
        <span class="pill" :class="snap.forward.b !== 'regfile' ? 'pill-fwd' : ''">
          fwd_b: {{ snap.forward.b }}
        </span>
      </template>
    </h1>

    <div class="bar">
      <button class="btn alt" @click="step">Step</button>
      <button class="btn"     @click="step5">Step 5</button>
      <button class="btn"     @click="step20">Step 20</button>
      <button class="btn"     @click="reset">Reset</button>
    </div>

    <template v-if="ready && snap">
      <div class="main-area">
      <!-- Five stage cards in a row (left side) -->
      <div class="stage-row">
        <!-- IF -->
        <div class="stage-card stage-if" :class="{ 'stage-empty': !snap.if_id.valid }">
          <h3>IF</h3>
          <div class="stage-instr">{{ snap.if_id.valid ? disasmShort(snap.if_id.instr) : '— bubble —' }}</div>
          <div class="stage-sub">Instruction Fetch</div>
          <div v-if="snap.if_id.valid" class="stage-body">
            <div><span class="k">pc</span><span class="v">{{ hex(snap.if_id.pc) }}</span></div>
            <div><span class="k">instr</span><span class="v">{{ hex(snap.if_id.instr) }}</span></div>
          </div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.IF" :key="c">{{ c }}</li>
          </ul>
        </div>

        <!-- ID -->
        <div class="stage-card stage-id" :class="{ 'stage-empty': !snap.id_ex.valid }">
          <h3>ID</h3>
          <div class="stage-instr">{{ snap.id_ex.valid ? disasmShort(snap.id_ex.instr) : '— bubble —' }}</div>
          <div class="stage-sub">Decode + RegRead</div>
          <div v-if="snap.id_ex.valid" class="stage-body">
            <div><span class="k">pc</span><span class="v">{{ hex(snap.id_ex.pc) }}</span></div>
            <div><span class="k">rs1 (x{{ snap.id_ex.rs1_num }})</span><span class="v">{{ hex(snap.id_ex.rs1_data) }}</span></div>
            <div><span class="k">rs2 (x{{ snap.id_ex.rs2_num }})</span><span class="v">{{ hex(snap.id_ex.rs2_data) }}</span></div>
            <div><span class="k">imm</span><span class="v">{{ hex(snap.id_ex.imm) }}</span></div>
            <div><span class="k">rd</span><span class="v">x{{ snap.id_ex.rd_num }}</span></div>
            <div class="ctrl">
              <span v-for="c in ctrlPretty(snap.id_ex.ctrl)" :key="c">{{ c }}</span>
            </div>
          </div>
          <div v-else class="bubble">— bubble —</div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.ID" :key="c">{{ c }}</li>
          </ul>
        </div>

        <!-- EX -->
        <div class="stage-card stage-ex" :class="{ 'stage-empty': !snap.ex_ma.valid }">
          <h3>EX</h3>
          <div class="stage-instr">{{ snap.ex_ma.valid ? disasmShort(snap.ex_ma.instr) : '— bubble —' }}</div>
          <div class="stage-sub">Execute (ALU)</div>
          <div v-if="snap.ex_ma.valid" class="stage-body">
            <div><span class="k">alu_result</span><span class="v">{{ hex(snap.ex_ma.alu_result) }}</span></div>
            <div><span class="k">rs2_data</span><span class="v">{{ hex(snap.ex_ma.rs2_data) }}</span></div>
            <div><span class="k">rd</span><span class="v">x{{ snap.ex_ma.rd_num }}</span></div>
            <div v-if="snap.ex_ma.branch_taken">
              <span class="k">branch →</span><span class="v hot">{{ hex(snap.ex_ma.branch_target) }}</span>
            </div>
            <div class="ctrl">
              <span v-for="c in ctrlPretty(snap.ex_ma.ctrl)" :key="c">{{ c }}</span>
            </div>
          </div>
          <div v-else class="bubble">— bubble —</div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.EX" :key="c">{{ c }}</li>
          </ul>
        </div>

        <!-- MA -->
        <div class="stage-card stage-ma" :class="{ 'stage-empty': !snap.ma_wb.valid }">
          <h3>MA</h3>
          <div class="stage-instr">{{ snap.ma_wb.valid ? disasmShort(snap.ma_wb.instr) : '— bubble —' }}</div>
          <div class="stage-sub">Memory Access</div>
          <div v-if="snap.ma_wb.valid" class="stage-body">
            <div><span class="k">alu_result</span><span class="v">{{ hex(snap.ma_wb.alu_result) }}</span></div>
            <div><span class="k">mem_data</span><span class="v">{{ hex(snap.ma_wb.mem_data) }}</span></div>
            <div><span class="k">rd</span><span class="v">x{{ snap.ma_wb.rd_num }}</span></div>
            <div class="ctrl">
              <span v-for="c in ctrlPretty(snap.ma_wb.ctrl)" :key="c">{{ c }}</span>
            </div>
          </div>
          <div v-else class="bubble">— bubble —</div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.MA" :key="c">{{ c }}</li>
          </ul>
        </div>

        <!-- WB -->
        <div class="stage-card stage-wb" :class="{ 'stage-empty': !snap.last_wb.valid }">
          <h3>WB</h3>
          <div class="stage-instr">{{ snap.last_wb.valid ? disasmShort(snap.last_wb.instr) : '— bubble —' }}</div>
          <div class="stage-sub">Write Back</div>
          <div v-if="snap.last_wb.valid" class="stage-body">
            <div><span class="k">alu_result</span><span class="v">{{ hex(snap.last_wb.alu_result) }}</span></div>
            <div><span class="k">mem_data</span><span class="v">{{ hex(snap.last_wb.mem_data) }}</span></div>
            <div><span class="k">rd</span><span class="v">x{{ snap.last_wb.rd_num }}</span></div>
            <div class="ctrl">
              <span v-for="c in ctrlPretty(snap.last_wb.ctrl)" :key="c">{{ c }}</span>
            </div>
          </div>
          <div v-else class="stage-body"><div class="dim">no instruction in WB this cycle</div></div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.WB" :key="c">{{ c }}</li>
          </ul>
        </div>
      </div>

      <!-- Right side: registers, RAM, ROM -->
      <div class="grid">
        <div class="pane">
          <h3>Registers (non-zero)</h3>
          <table>
            <tbody>
              <tr v-for="(v, i) in regList" :key="i" v-show="v !== 0 || i === 0"
                  @dblclick="edit(`x${i}`, v, n => cpu!.poke_register(i, n))">
                <td class="name">x{{ i }} {{ REG_NAMES[i] }}</td>
                <td class="val">{{ hex(v) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <div class="pane">
          <h3>RAM @ 0x10000</h3>
          <table>
            <tbody>
              <tr v-for="(v, i) in ramList.slice(0, 16)" :key="i"
                  @dblclick="edit(hex(0x10000 + i*4), v, n => cpu!.poke_ram(0x10000 + i*4, n))">
                <td class="name">{{ hex(0x10000 + i*4) }}</td>
                <td class="val">{{ hex(v) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <div class="pane">
          <h3>ROM @ 0x00000</h3>
          <table>
            <tbody>
              <tr v-for="(v, i) in romList.slice(0, 24)" :key="i"
                  :class="{ pc: snap && snap.pc === i*4 }"
                  :title="disasmFull(v) ? `${disasmFull(v)!.mnemonic} ${disasmFull(v)!.operands}\n${disasmFull(v)!.desc || ''}` : ''"
                  @dblclick="edit(hex(i*4), v, n => cpu!.poke_rom(i*4, n))">
                <td class="name">{{ hex(i*4) }}</td>
                <td class="val">{{ hex(v) }}</td>
                <td class="dim">{{ disasmShort(v) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
      </div><!-- /main-area -->
    </template>
    <p v-else style="padding:2rem; color:#888;">Loading wasm…</p>
  </div>
</template>

<style scoped>
html, body { height: 100%; }
.ms-root {
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  background: #1e1e1e; color: #ddd; padding: 1rem;
  font-size: 12px; box-sizing: border-box;
  display: flex; flex-direction: column; overflow: hidden;
  position: fixed; inset: 0;
}
.title-row { display: flex; align-items: center; gap: .4rem; margin: 0 0 .8rem; font-size: 1.1rem; color: #4ec3d5; flex-wrap: wrap; }
.back-btn { background: #444; color: #fff; border: 0; padding: .35rem .7rem; cursor: pointer; border-radius: 3px; font: inherit; }
.back-btn:hover { background: #666; }
.ms-name { margin-right: .6rem; }
.pill { background: #2a2a4a; color: #cce; padding: .25rem .55rem; border-radius: 3px; font-size: .75rem; }
.pill-warn { background: #cc2222; color: #fff; }
.pill-fwd  { background: #ffd84d; color: #000; }

.bar { display: flex; align-items: center; gap: .4rem; margin-bottom: .8rem; }
.btn { background: #2aacbe; color: #000; border: 0; padding: .4rem .8rem; cursor: pointer; font: inherit; border-radius: 3px; }
.btn:hover { background: #4ec3d5; }
.btn.alt { background: #cc8800; }
.btn.alt:hover { background: #e6a31a; }

.main-area {
  display: flex;
  gap: 0.8cm;
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}
.stage-row {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: .6rem;
  flex: 2.55;
  min-width: 0;
}
.stage-card {
  background: #111; border: 1px solid #333; border-radius: 4px;
  padding: .55rem .65rem;
  display: flex; flex-direction: column;
  min-height: 0; overflow: auto;
}
.stage-card.stage-empty { opacity: 0.5; }
.stage-card h3 { margin: 0 0 .15rem; color: #4ec3d5; font-size: 1rem; }
.stage-instr {
  color: #ffd84d;
  font-size: 1rem;
  font-weight: bold;
  margin: 0 0 .25rem;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.stage-sub { color: #888; font-size: .7rem; margin-bottom: .4rem; }
.stage-body { font-size: .72rem; line-height: 1.5; }
.stage-body > div { display: flex; justify-content: space-between; }
.stage-body .k { color: #0099cc; }
.stage-body .v { color: #fff; font-variant-numeric: tabular-nums; }
.stage-body .v.hot { color: #ffd84d; }
.stage-body .dis { color: #cc8800; margin-top: .2rem; }
.stage-body .ctrl { display: flex; flex-wrap: wrap; gap: .25rem; margin-top: .35rem; font-size: .65rem; color: #888; }
.stage-body .ctrl span { background: #1a1a1a; padding: .1rem .3rem; border-radius: 2px; }
.stage-body .dim { color: #555; font-style: italic; }
.bubble { color: #cc2222; font-size: .8rem; padding: .8rem 0; text-align: center; }
.stage-card h4 { margin: .55rem 0 .15rem; color: #888; font-size: .65rem; text-transform: uppercase; }
.active { list-style: none; padding: 0; margin: 0; font-size: .68rem; color: #00ff66; }
.active li { padding: 1px 0; }

.grid {
  display: grid;
  grid-template-columns: 1fr 1fr 2.2fr;
  gap: .6rem;
  flex: 2;
  min-width: 0;
  min-height: 0;
}
.pane { background: #111; border: 1px solid #333; border-radius: 4px; padding: .55rem .7rem; overflow: auto; }
.pane h3 { margin: 0 0 .35rem; color: #cc8800; font-size: .85rem; position: sticky; top: 0; background: #111; }
table { border-collapse: collapse; width: 100%; }
td { padding: 1px 6px; border-bottom: 1px solid #222; white-space: nowrap; }
td.name { color: #0099cc; }
td.val  { text-align: right; font-variant-numeric: tabular-nums; padding-right: 2ch; color: #fff; }
td.dim  { color: #555; padding-left: 10px; }
tr.pc td { background: #2a4a2a; color: #00ff66 !important; }
tr { cursor: pointer; }
</style>
