<script setup lang="ts">
//
// MicroStepPipelineView — table view for the 5-stage pipeline simulator.
// Loads the wasm Pipeline class and renders one column per
// stage (IF/ID/EX/MA/WB) showing the pipeline register contents and the
// active components, plus registers/RAM/ROM panels on the side.
//

import { computed, nextTick, onMounted, ref, shallowRef, watch } from 'vue'
import MmuIrqDemo from '../components/MmuIrqDemo.vue'
import CsrLiveDisplay from '../components/CsrLiveDisplay.vue'
import type { DisasmJson, Region } from '../stores/wasmCore'
import type { CtrlJson, PipeCsrJson, Pipeline, PipelineSnapshot } from '../wasm/cpu_sim'

const emit = defineEmits<{ 'switch-cpu': [] }>()

// The wasm Pipeline class and its snapshot types (generated from Rust).
type Cpu = Pipeline
type Snapshot = PipelineSnapshot
type CsrEntry = PipeCsrJson
type Disasmer = (instr: number) => DisasmJson

const cpu  = shallowRef<Cpu | null>(null)
const dis  = shallowRef<Disasmer | null>(null)
const rom = ref<Region>({ name: 'rom', base: 0, words: 0 })
const ram = ref<Region>({ name: 'ram', base: 0, words: 0 })
const ready = ref(false)
const renderTick = ref(0)

const REG_NAMES = [
  'zero','ra','sp','gp','tp','t0','t1','t2','s0','s1',
  'a0','a1','a2','a3','a4','a5','a6','a7','s2','s3','s4','s5','s6','s7',
  's8','s9','s10','s11','t3','t4','t5','t6',
]

const hex = (n: number, w = 8) => '0x' + (n >>> 0).toString(16).padStart(w, '0')

// Default ROM image. Layout:
//   0x00..0x18  boot: install mtvec, enable MIE & MEIE
//   0x1c..      demo body (RAW, load-use, branch, spin)
//   0x80..0x90  trap handler (clears mip, bumps s0, mret)
function buildProgram(): Uint32Array {
  const NOP = 0x00000013
  const rom: number[] = new Array(64).fill(NOP)

  // ── boot ──
  // 0x00: addi  t0, x0, 0x80
  rom[0] = 0x08000293
  // 0x04: csrrw x0, mtvec, t0      ; mtvec = 0x80
  rom[1] = 0x30529073
  // 0x08: addi  t0, x0, 1
  rom[2] = 0x00100293
  // 0x0c: slli  t0, t0, 11         ; t0 = 0x800 (MEIE bit)
  rom[3] = 0x00b29293
  // 0x10: csrrw x0, mie, t0        ; mie.MEIE = 1
  rom[4] = 0x30429073
  // 0x14: addi  t0, x0, 8          ; t0 = 0x8 (MIE bit)
  rom[5] = 0x00800293
  // 0x18: csrrs x0, mstatus, t0    ; mstatus.MIE = 1
  rom[6] = 0x3002a073

  // ── demo body starts at 0x1c (rom index 7) ──
  rom[7]  = 0x00500293          // addi x5, x0, 5
  rom[8]  = 0x00128313          // addi x6, x5, 1   (RAW forward)
  rom[9]  = 0x000102b7          // lui  x5, 0x10
  rom[10] = 0x0062a023          // sw   x6, 0(x5)
  rom[11] = 0x0002a383          // lw   x7, 0(x5)
  rom[12] = 0x00738433          // add  x8, x7, x7  (load-use stall)
  rom[13] = 0x407404b3          // sub  x9, x8, x7  (chained, both forwarded)
  rom[14] = 0x00300513          // addi x10, x0, 3        ; counter = 3
  rom[15] = 0xfff50513          // addi x10, x10, -1      ; LOOP: decrement
  rom[16] = 0xfe051ee3          // bne  x10, x0, -4       ; loop while x10 != 0
  rom[17] = 0x00540863          // beq  x8, x5, 16
  rom[18] = 0x02a00913          // addi s2, x0, 42
  rom[19] = 0x0000006f          // jal  x0, 0       (spin)

  // ── trap handler at 0x80 (rom index 32) ──
  rom[32] = 0x34202373         // csrrs x6, mcause, x0
  rom[33] = 0x34405073         // csrrwi x0, mip, 0
  rom[34] = 0x00140413         // addi s0, s0, 1
  rom[35] = 0x30200073         // mret

  return new Uint32Array(rom)
}
const program = ref<Uint32Array>(buildProgram())

onMounted(async () => {
  const mod = await import('../wasm/cpu_sim.js')
  await mod.default()
  const c = new mod.Pipeline()
  for (const r of c.regions()) { if (r.name === 'rom') rom.value = r; if (r.name === 'ram') ram.value = r }
  c.load(rom.value.base, program.value)
  cpu.value = c
  dis.value = mod.disasm
  ready.value = true
})

function step()    { cpu.value?.step(); renderTick.value++ }
function step5()   { for (let i = 0; i < 5; i++) cpu.value?.step(); renderTick.value++ }
function step20()  { for (let i = 0; i < 20; i++) cpu.value?.step(); renderTick.value++ }
function reset()   {
  if (!cpu.value) return
  cpu.value.reset()
  cpu.value.load(rom.value.base, program.value)
  renderTick.value++
}

// ── Load program from file ─────────────────────────────────────────
const fileInput = ref<HTMLInputElement | null>(null)
function clickLoad() { fileInput.value?.click() }

interface ProgramEntryHex  { hex: string; asm?: string }
interface ProgramFileHex   { program?: ProgramEntryHex[] }
interface ProgramEntryAddr { addr: number; value: string; asm?: string }
const ROM_WORDS = 64
const NOP = 0x00000013

function onFileLoad(event: Event) {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file || !cpu.value) return
  const reader = new FileReader()
  reader.onload = () => {
    const text = reader.result as string
    let words: Uint32Array | null = null

    // Try JSON formats.
    try {
      const json = JSON.parse(text) as unknown
      // Format A: array of {addr, value, asm} (lab format).
      if (Array.isArray(json) && json.length > 0
          && typeof (json as ProgramEntryAddr[])[0]?.addr === 'number') {
        const arr = json as ProgramEntryAddr[]
        const buf = new Uint32Array(ROM_WORDS).fill(NOP)
        for (const e of arr) {
          const idx = (e.addr >>> 2)
          const v = parseInt(e.value, 16)
          if (!isNaN(v) && idx < buf.length) buf[idx] = v >>> 0
        }
        words = buf
      } else {
        // Format B: { program: [{ hex, asm }, ...] } (CpuLayout format).
        const j = json as ProgramFileHex
        if (j.program && Array.isArray(j.program)) {
          words = new Uint32Array(j.program.map(e => parseInt(e.hex, 16) >>> 0))
        }
      }
    } catch { /* not JSON, try hex-per-line */ }

    // Format C: hex per line.
    if (!words) {
      const lines = text.trim().split('\n').filter(l => l.trim())
      const arr = lines.map(l => parseInt(l.trim(), 16)).filter(n => !isNaN(n))
      if (arr.length > 0) words = new Uint32Array(arr)
    }

    if (!words || words.length === 0) {
      alert('No instructions parsed from file.')
      target.value = ''
      return
    }
    program.value = words
    cpu.value!.reset()
    cpu.value!.load(rom.value.base, program.value)
    renderTick.value++
  }
  reader.readAsText(file)
  target.value = ''
}

const snap = computed<Snapshot | null>(() => {
  renderTick.value
  if (!cpu.value) return null
  return cpu.value.snapshot()
})

const regList = computed<number[]>(() => {
  renderTick.value
  return cpu.value ? Array.from(cpu.value.regs()) : []
})
const machineRamList = computed<number[]>(() => {
  renderTick.value
  return cpu.value ? Array.from(cpu.value.mem(ram.value.base, ram.value.words)) : []
})
const userRamList = computed<number[]>(() => {
  renderTick.value
  return cpu.value ? Array.from(cpu.value.user_ram()) : []
})
const romList = computed<number[]>(() => {
  renderTick.value
  return cpu.value ? Array.from(cpu.value.mem(rom.value.base, rom.value.words)) : []
})

// RAM tab: 'auto' follows priv mode, 'user'/'machine' override.
type RamTab = 'auto' | 'user' | 'machine'
const ramTab = ref<RamTab>('auto')
const effectiveRamSide = computed<'user' | 'machine'>(() => {
  if (ramTab.value !== 'auto') return ramTab.value
  // Auto: M-mode shows machine, otherwise user.
  if (snap.value && snap.value.priv_mode !== 3) return 'user'
  return 'machine'
})
const ramView = computed<number[]>(() => {
  return effectiveRamSide.value === 'user' ? userRamList.value : machineRamList.value
})

// Register highlight: 1=read this cycle, 2=written this cycle, 0=idle.
function regClass(i: number): string {
  if (!snap.value) return ''
  const rd = (snap.value.reg_read_mask  >>> i) & 1
  const wr = (snap.value.reg_write_mask >>> i) & 1
  if (wr) return 'reg-write'
  if (rd) return 'reg-read'
  return ''
}

// Active RAM cell: highlight only if the side matches what's being viewed.
function ramRowClass(byteAddr: number): string {
  if (!snap.value || snap.value.last_ram_kind === 0) return ''
  const side = snap.value.last_ram_side === 1 ? 'user' : 'machine'
  if (side !== effectiveRamSide.value) return ''
  if ((snap.value.last_ram_addr & ~3) !== (byteAddr & ~3)) return ''
  return snap.value.last_ram_kind === 1 ? 'ram-read' : 'ram-write'
}

const privName = (p: number) =>
  p === 3 ? 'M' : p === 1 ? 'S' : 'U'

// Curated CSR list (addr → display name) for the under-box.
const CSR_DISPLAY: Array<[number, string]> = [
  [0x300, 'mstatus'], [0x305, 'mtvec'], [0x341, 'mepc'], [0x342, 'mcause'],
  [0x343, 'mtval'],   [0x304, 'mie'],   [0x344, 'mip'],  [0x340, 'mscratch'],
  [0x180, 'satp'],    [0x100, 'sstatus'], [0x141, 'sepc'], [0x142, 'scause'],
  [0x105, 'stvec'],   [0x302, 'medeleg'], [0x303, 'mideleg'],
  [0x144, 'sip'],     [0x104, 'sie'],   [0x140, 'sscratch'], [0x143, 'stval'],
  [0x3a0, 'pmpcfg0'],
]
const csrEntries = computed(() => {
  const s = snap.value
  if (!s) return []
  const byAddr = new Map<number, CsrEntry>()
  for (const e of s.csrs) byAddr.set(e.addr, e)
  return CSR_DISPLAY.map(([addr, name]) => {
    const e = byAddr.get(addr)
    return { addr, name, value: e?.value ?? 0, changed: e?.changed ?? false }
  })
})

// ── ROM auto-scroll on jump/branch/trap ─────────────────────────────
const romScrollRef = ref<HTMLDivElement | null>(null)

function scrollRomToTarget(pc: number) {
  const sb = romScrollRef.value
  if (!sb) return
  const rowIdx = pc >>> 2
  const row = sb.querySelector(`tr[data-rom-idx="${rowIdx}"]`) as HTMLElement | null
  if (!row) return
  const rowTop = row.offsetTop
  const visibleTop = sb.scrollTop
  const visibleBottom = visibleTop + sb.clientHeight
  // Already fully visible? leave it alone.
  if (rowTop >= visibleTop && rowTop + row.offsetHeight <= visibleBottom) return
  // Otherwise put the target row 8 rows down from the top edge.
  const rowHeight = row.offsetHeight || 16
  sb.scrollTop = Math.max(0, rowTop - 8 * rowHeight)
}

watch(() => snap.value?.pc, (newPc, oldPc) => {
  if (newPc == null) return
  // Sequential advance does not trigger scroll.
  if (oldPc != null && newPc === ((oldPc + 4) >>> 0)) return
  nextTick(() => scrollRomToTarget(newPc))
})

// Branch mode dropdown
const BRANCH_MODES = [
  { v: 0, label: 'Eager (resolve in EX)', enabled: true },
  { v: 1, label: 'Always-target (ID redirect)', enabled: true },
  { v: 2, label: 'Early resolve (comparator in ID)', enabled: true },
  { v: 3, label: 'Predict (BHT/BTB) — TODO', enabled: false },
]
function setBranchMode(e: Event) {
  if (!cpu.value) return
  const v = Number((e.target as HTMLSelectElement).value)
  cpu.value.set_branch_mode(v)
  renderTick.value++
}

function injectIrq() {
  if (!cpu.value) return
  const c = cpu.value
  // Only assert the external device line (mip.MEIP). MIE / MEIE are
  // enabled by the boot code; the trap handler clears mip.MEIP and
  // mret restores mstatus.MIE. This way clicking Inject IRQ while a
  // handler is in flight does not re-trap mid-handler.
  c.csr_write(0x344, c.csr_read(0x344) | (1 << 11))
  renderTick.value++
}

function disasmShort(instr: number): string {
  if (!dis.value || instr === 0) return ''
  const d = dis.value(instr >>> 0)
  return `${d.mnemonic.padEnd(6)} ${d.operands}`
}

function disasmFull(instr: number): DisasmJson | null {
  if (!dis.value || instr === 0) return null
  return dis.value(instr >>> 0)
}

function edit(label: string, current: number, write: (n: number) => void) {
  const v = window.prompt(`new value for ${label} (hex or dec)`, hex(current))
  if (v === null) return
  const n = v.startsWith('0x') ? parseInt(v.slice(2), 16) : Number(v)
  if (Number.isNaN(n)) return
  write(n >>> 0)
  renderTick.value++
}

// Single source of truth for what a stage card displays in its
// stage-instr line and whether the card is "empty" (dimmed). Used by
// all five stage cards so a new bubble kind only needs to be added
// here, not in five places.
interface StageReg {
  valid: boolean
  instr: number
  flush_bubble: boolean
  stall_reg?: number   // IF/ID register has no stall_reg; treat as 0
}
function stageLabel(r: StageReg): string {
  if (r.valid) return disasmShort(r.instr)
  if ((r.stall_reg ?? 0) !== 0) return 'REG STALL'
  if (r.flush_bubble) return 'BR FLUSH'
  return '— bubble —'
}
function stageEmpty(r: StageReg): boolean {
  return !r.valid && (r.stall_reg ?? 0) === 0 && !r.flush_bubble
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
        <span class="pill pill-priv">priv {{ privName(snap.priv_mode) }}</span>
        <label class="branch-sel">
          branch:
          <select :value="snap.branch_mode" @change="setBranchMode">
            <option v-for="m in BRANCH_MODES" :key="m.v"
                    :value="m.v" :disabled="!m.enabled">{{ m.label }}</option>
          </select>
        </label>
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
      <button class="btn"     @click="clickLoad">Load…</button>
      <input ref="fileInput" type="file" accept=".json,.hex,.txt"
             style="display:none" @change="onFileLoad" />
      <span class="bar-gap"></span>
      <button class="btn btn-irq" @click="injectIrq">Inject IRQ</button>
    </div>

    <template v-if="ready && snap">
      <div class="main-area">
      <div class="left-col">
      <!-- Five stage cards in a row (left side) -->
      <div class="stage-row">
        <!-- IF -->
        <div class="stage-card stage-if" :class="{ 'stage-empty': stageEmpty(snap.if_id) }">
          <h3>IF</h3>
          <div class="stage-instr">{{ stageLabel(snap.if_id) }}</div>
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
        <div class="stage-card stage-id" :class="{ 'stage-empty': stageEmpty(snap.id_ex) }">
          <h3>ID</h3>
          <div class="stage-instr">{{ stageLabel(snap.id_ex) }}</div>
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
          <div v-else-if="snap.id_ex.stall_reg !== 0" class="stage-body">
            <div class="stall-line">
              <span class="k">reg</span>
              <span class="stall-reg">x{{ snap.id_ex.stall_reg }} {{ REG_NAMES[snap.id_ex.stall_reg] }}</span>
            </div>
          </div>
          <div v-else class="bubble">— bubble —</div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.ID" :key="c">{{ c }}</li>
          </ul>
        </div>

        <!-- EX -->
        <div class="stage-card stage-ex" :class="{ 'stage-empty': stageEmpty(snap.ex_ma) }">
          <h3>EX</h3>
          <div class="stage-instr">{{ stageLabel(snap.ex_ma) }}</div>
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
          <div v-else-if="snap.ex_ma.stall_reg !== 0" class="stage-body">
            <div class="stall-line">
              <span class="k">reg</span>
              <span class="stall-reg">x{{ snap.ex_ma.stall_reg }} {{ REG_NAMES[snap.ex_ma.stall_reg] }}</span>
            </div>
          </div>
          <div v-else class="bubble">— bubble —</div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.EX" :key="c">{{ c }}</li>
          </ul>
        </div>

        <!-- MA -->
        <div class="stage-card stage-ma" :class="{ 'stage-empty': stageEmpty(snap.ma_wb) }">
          <h3>MA</h3>
          <div class="stage-instr">{{ stageLabel(snap.ma_wb) }}</div>
          <div v-if="snap.ma_wb.valid" class="stage-body">
            <div><span class="k">alu_result</span><span class="v">{{ hex(snap.ma_wb.alu_result) }}</span></div>
            <div><span class="k">mem_data</span><span class="v">{{ hex(snap.ma_wb.mem_data) }}</span></div>
            <div><span class="k">rd</span><span class="v">x{{ snap.ma_wb.rd_num }}</span></div>
            <div class="ctrl">
              <span v-for="c in ctrlPretty(snap.ma_wb.ctrl)" :key="c">{{ c }}</span>
            </div>
          </div>
          <div v-else-if="snap.ma_wb.stall_reg !== 0" class="stage-body">
            <div class="stall-line">
              <span class="k">reg</span>
              <span class="stall-reg">x{{ snap.ma_wb.stall_reg }} {{ REG_NAMES[snap.ma_wb.stall_reg] }}</span>
            </div>
          </div>
          <div v-else class="bubble">— bubble —</div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.MA" :key="c">{{ c }}</li>
          </ul>
        </div>

        <!-- WB -->
        <div class="stage-card stage-wb" :class="{ 'stage-empty': stageEmpty(snap.last_wb) }">
          <h3>WB</h3>
          <div class="stage-instr">{{ stageLabel(snap.last_wb) }}</div>
          <div v-if="snap.last_wb.valid" class="stage-body">
            <div><span class="k">alu_result</span><span class="v">{{ hex(snap.last_wb.alu_result) }}</span></div>
            <div><span class="k">mem_data</span><span class="v">{{ hex(snap.last_wb.mem_data) }}</span></div>
            <div><span class="k">rd</span><span class="v">x{{ snap.last_wb.rd_num }}</span></div>
            <div class="ctrl">
              <span v-for="c in ctrlPretty(snap.last_wb.ctrl)" :key="c">{{ c }}</span>
            </div>
          </div>
          <div v-else-if="snap.last_wb.stall_reg !== 0" class="stage-body">
            <div class="stall-line">
              <span class="k">reg</span>
              <span class="stall-reg">x{{ snap.last_wb.stall_reg }} {{ REG_NAMES[snap.last_wb.stall_reg] }}</span>
            </div>
          </div>
          <div v-else class="stage-body"><div class="dim">no instruction in WB this cycle</div></div>
          <h4>active</h4>
          <ul class="active">
            <li v-for="c in snap.active_components.WB" :key="c">{{ c }}</li>
          </ul>
        </div>
      </div>
      <div class="under-box">
        <CsrLiveDisplay :entries="csrEntries" :priv-mode="snap.priv_mode" />
        <MmuIrqDemo />
      </div>
      </div><!-- /left-col -->

      <!-- Right side: registers, RAM, ROM -->
      <div class="grid">
        <!-- header row: tabs aligned over the RAM column -->
        <div class="grid-header"></div>
        <div class="grid-header ram-header">
          <span class="ram-tabs-label">RAM:</span>
          <span class="ram-tabs">
            <button :class="{ active: ramTab === 'auto' }" @click="ramTab = 'auto'">Auto</button>
            <button :class="{ active: ramTab === 'user' }" @click="ramTab = 'user'">User</button>
            <button :class="{ active: ramTab === 'machine' }" @click="ramTab = 'machine'">Machine</button>
          </span>
          <span class="ram-side">{{ effectiveRamSide }}</span>
        </div>
        <div class="grid-header"></div>
        <div class="pane">
          <h3>Registers (32)</h3>
          <div class="scroll-body">
            <table>
              <tbody>
                <tr v-for="(v, i) in regList" :key="i" :class="regClass(i)"
                    @dblclick="edit(`x${i}`, v, n => cpu!.poke_reg(i, n))">
                  <td class="name">x{{ i }} {{ REG_NAMES[i] }}</td>
                  <td class="val">{{ hex(v) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
        <div class="pane">
          <h3>RAM <span class="ram-side">({{ effectiveRamSide }})</span></h3>
          <div class="scroll-body">
            <table>
              <tbody>
                <tr v-for="(v, i) in ramView" :key="i"
                    :class="ramRowClass(ram.base + i*4)"
                    @dblclick="edit(hex(ram.base + i*4), v,
                        n => effectiveRamSide === 'user'
                              ? cpu!.poke_user_ram(ram.base + i*4, n)
                              : cpu!.poke_mem(ram.base + i*4, n))">
                  <td class="name">{{ hex(ram.base + i*4) }}</td>
                  <td class="val">{{ hex(v) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
        <div class="pane">
          <h3>ROM @ {{ hex(rom.base) }}</h3>
          <div class="scroll-body" ref="romScrollRef">
            <table>
              <tbody>
                <tr v-for="(v, i) in romList" :key="i"
                    :data-rom-idx="i"
                    :class="{ pc: snap && snap.pc === rom.base + i*4 }"
                    :title="disasmFull(v) ? `${disasmFull(v)!.mnemonic} ${disasmFull(v)!.operands}\n${disasmFull(v)!.desc || ''}` : ''"
                    @dblclick="edit(hex(rom.base + i*4), v, n => cpu!.poke_mem(rom.base + i*4, n))">
                  <td class="name">{{ hex(rom.base + i*4) }}</td>
                  <td class="val">{{ hex(v) }}</td>
                  <td class="dim">{{ disasmShort(v) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
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
.left-col {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: .6rem;
}
.stage-row {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: .4rem;
  flex: 5;
  min-width: 0;
  min-height: 0;
}
.under-box {
  flex: 5;
  min-height: 0;
  max-height: 42%;
  background: #000;
  border: 1px solid #333;
  border-radius: 4px;
  display: flex;
  overflow: hidden;
}
.stage-card {
  background: #111; border: 1px solid #333; border-radius: 4px;
  padding: .35rem .45rem;
  display: flex; flex-direction: column;
  min-height: 0; min-width: 0; overflow: auto;
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
  flex-shrink: 0;
}
.stage-sub { color: #888; font-size: .7rem; margin-bottom: .4rem; }
.stage-body { font-size: .72rem; line-height: 1.5; }
.stage-body > div { display: flex; justify-content: space-between; }
.stage-body .k { color: #0099cc; white-space: nowrap; }
.stage-body .v { color: #fff; font-variant-numeric: tabular-nums; white-space: nowrap; }
.stage-body .v.hot { color: #ffd84d; }
.stage-body .dis { color: #cc8800; margin-top: .2rem; }
.stage-body .ctrl { display: flex; flex-wrap: wrap; gap: .25rem; margin-top: .35rem; font-size: .65rem; color: #888; }
.stage-body .ctrl span { background: #1a1a1a; padding: .1rem .3rem; border-radius: 2px; }
.stage-body .dim { color: #555; font-style: italic; }
.bubble { color: #cc2222; font-size: .8rem; padding: .8rem 0; text-align: center; }
.stall-line { display: flex; gap: .5rem; justify-content: flex-start; }
.stall-line .k { color: #0099cc; }
.stall-reg { color: #fff; text-align: left; }
.stage-card h4 { margin: .55rem 0 .15rem; color: #888; font-size: .65rem; text-transform: uppercase; }
.active { list-style: none; padding: 0; margin: 0; font-size: .68rem; color: #00ff66; }
.active li { padding: 1px 0; }

.grid {
  display: grid;
  grid-template-columns: minmax(11ch, 1.1fr) minmax(20ch, 1.3fr) minmax(28ch, 2.0fr);
  grid-template-rows: auto 1fr;
  gap: .25rem .5rem;
  flex: 1;
  min-width: 0;
  min-height: 0;
}
.grid-header { display: flex; align-items: center; min-height: 1.5rem; }
.ram-header { gap: .25rem; }
.pane {
  background: #111; border: 1px solid #333; border-radius: 4px;
  padding: .55rem .7rem;
  display: flex; flex-direction: column;
  min-height: 0; overflow: hidden;
}
.pane h3 { margin: 0 0 .35rem; color: #cc8800; font-size: .85rem; flex-shrink: 0; }
.scroll-body { flex: 1; min-height: 0; overflow-x: hidden; overflow-y: auto; }
table { border-collapse: collapse; width: 100%; }
td { padding: 1px 2px; border-bottom: 1px solid #222; white-space: nowrap; }
td.name { color: #0099cc; padding-right: 4px; width: 1%; }
td.val  { font-variant-numeric: tabular-nums; padding-left: 0; padding-right: 4px; color: #fff; width: 1%; }
td.dim  { color: #555; padding-left: 6px; }
tr.pc td { background: #2a4a2a; color: #00ff66 !important; }
tr { cursor: pointer; }

/* Per-cycle reg/RAM activity highlighting */
tr.reg-read td  { background: #103a3a; color: #4dd0d0 !important; }
tr.reg-write td { background: #3a2a10; color: #ffb84d !important; }
tr.ram-read td  { background: #103a3a; color: #4dd0d0 !important; }
tr.ram-write td { background: #3a2a10; color: #ffb84d !important; }

/* Title-row priv pill */
.pill-priv { background: #4a2068; color: #fff; border: 1px solid #c080e0; }
.branch-sel { font-size: .75rem; color: #cce; display: inline-flex; align-items: center; gap: .25rem; }
.branch-sel select {
  background: #1a1a2e; color: #cce; border: 1px solid #4a4a6a;
  font-family: inherit; font-size: .7rem; padding: 1px 4px; border-radius: 3px;
}
.branch-sel select:focus { outline: 1px solid #6a6a8a; }
.branch-sel select option:disabled { color: #555; }

/* Bar gap before Inject IRQ */
.bar-gap { display: inline-block; width: 1.5rem; }
.btn-irq {
  background: #4a2068; color: #fff; border: 1px solid #c080e0;
}
.btn-irq:hover { background: #6a3088; }

/* RAM tab buttons */
.ram-tabs {
  display: inline-flex; gap: 2px; margin-left: .5rem;
}
.ram-tabs button {
  background: #1a1a1a; color: #888; border: 1px solid #333;
  font-size: 9px; padding: 0 6px; cursor: pointer; font-family: inherit;
  border-radius: 2px;
}
.ram-tabs button.active { background: #333; color: #fff; border-color: #888; }
.ram-tabs-label { color: #888; font-size: 10px; margin-right: .25rem; }
.ram-side { font-size: 10px; color: #aaa; margin-left: .35rem; font-style: italic; }
</style>
