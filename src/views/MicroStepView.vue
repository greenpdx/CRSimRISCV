<script setup lang="ts">
//
// MicroStepView — Vue port of `sim/demo.html`. Loads the standalone Rust
// simulator from /sim/pkg/cpu_sim.js (built via `npm run wasm`).
//
// This view is its own isolated simulator (a JsCpuState instance), separate
// from the Vue Pinia store that drives CpuLayout. The two coexist so the
// instructional micro-step UI doesn't fight with the existing visualizer.
//
import { computed, onMounted, ref, shallowRef } from 'vue'

const emit = defineEmits<{ 'switch-cpu': [] }>()

interface BusEntry  { name: string; value: number | null }
interface CtrlEntry { name: string; value: string | null }
interface CompEntry { name: string; value: string | null }
interface CsrEntry  { addr: number; name: string; value: number; desc: string }
interface MicroJson {
  current: string | null
  opcode_name: string | null
  active: string[]
  consumed: string[]
  reg_access: number[]
  ram_access: number[]
  rom_access: number | null
  buses: BusEntry[]
  controls: CtrlEntry[]
  components: CompEntry[]
}
interface DisasmJson { class: string; mnemonic: string; operands: string; desc: string }
interface TrapInfo {
  cycle: number; from: string; to: string
  cause: number; is_interrupt: boolean; epc: number; tval: number
}

// JsCpuState is the wasm class — typed loosely to avoid import wrangling.
type Cpu = {
  pc: number
  cycle: bigint
  priv_mode: string
  in_trap: boolean
  reset(): void
  load_program(words: Uint32Array): void
  micro_step(): string
  micro_state(): MicroJson
  registers(): Uint32Array
  ram(): Uint32Array
  rom(): Uint32Array
  read_register(idx: number): number
  poke_register(idx: number, val: number): void
  poke_ram(addr: number, val: number): void
  poke_rom(addr: number, val: number): void
  csr_read(addr: number): number
  csr_write(addr: number, val: number): void
  csr_snapshot(): CsrEntry[]
  raise_irq(kind: string): void
  clear_irq(): void
  last_trap(): TrapInfo | null
  disasm(instr: number): DisasmJson
}

const cpu = shallowRef<Cpu | null>(null)
const ready = ref(false)
const renderTick = ref(0) // bump to force re-render after a step
const REG_NAMES = [
  'zero','ra','sp','gp','tp','t0','t1','t2','s0','s1',
  'a0','a1','a2','a3','a4','a5','a6','a7','s2','s3','s4','s5','s6','s7',
  's8','s9','s10','s11','t3','t4','t5','t6',
]

const hex = (n: number, w = 8) =>
  '0x' + (n >>> 0).toString(16).padStart(w, '0')

// Privilege/trap demo program — same as the standalone demo's default.
const program = (() => {
  const p = new Uint32Array(64)
  p.fill(0x00000013)
  p[0x00 / 4] = 0x00700093 // addi x1, x0, 7
  p[0x04 / 4] = 0x00500113 // addi x2, x0, 5
  p[0x08 / 4] = 0x002081B3 // add  x3, x1, x2
  p[0x0c / 4] = 0x08000213 // addi x4, x0, 0x80
  p[0x10 / 4] = 0x30521073 // csrrw x0, mtvec, x4
  p[0x14 / 4] = 0x30046073 // csrrsi x0, mstatus, 8
  p[0x18 / 4] = 0x08000293 // addi x5, x0, 0x80
  p[0x1c / 4] = 0x30429073 // csrrw x0, mie, x5
  p[0x20 / 4] = 0x00000073 // ecall
  p[0x24 / 4] = 0x0AA00313 // addi x6, x0, 0xAA
  p[0x28 / 4] = 0x000023B7 // lui  x7, 0x2
  p[0x2c / 4] = 0x80038393 // addi x7, x7, -0x800
  p[0x30 / 4] = 0x3003B073 // csrrc x0, mstatus, x7
  p[0x34 / 4] = 0x06000413 // addi x8, x0, 0x60
  p[0x38 / 4] = 0x34141073 // csrrw x0, mepc, x8
  p[0x3c / 4] = 0x30200073 // mret
  p[0x60 / 4] = 0x05500493 // addi x9, x0, 0x55
  p[0x64 / 4] = 0x06400513 // addi x10, x0, 100
  p[0x68 / 4] = 0x00000073 // ecall
  p[0x6c / 4] = 0x0BB00593 // addi x11, x0, 0xBB
  p[0x70 / 4] = 0x0000006F // jal  x0, 0
  p[0x80 / 4] = 0x00100613 // addi x12, x0, 1
  p[0x84 / 4] = 0x342026F3 // csrrs x13, mcause, x0
  p[0x88 / 4] = 0x34102773 // csrrs x14, mepc,   x0
  p[0x8c / 4] = 0x00470713 // addi  x14, x14, 4
  p[0x90 / 4] = 0x34171073 // csrrw x0, mepc, x14
  p[0x94 / 4] = 0x30200073 // mret
  return p
})()

onMounted(async () => {
  // Imported from src/wasm/ (populated by `npm run wasm`). Vite handles the
  // .wasm asset hashing automatically because the shim uses `import.meta.url`.
  const mod = await import('../wasm/cpu_sim.js')
  await mod.default()
  const c = new mod.JsCpuState() as Cpu
  c.load_program(program)
  cpu.value = c
  ready.value = true
})

function micro() { cpu.value?.micro_step(); renderTick.value++ }
function full()  { for (let i = 0; i < 10; i++) cpu.value?.micro_step(); renderTick.value++ }
function run10() { for (let i = 0; i < 100; i++) cpu.value?.micro_step(); renderTick.value++ }
function reset() {
  if (!cpu.value) return
  cpu.value.reset()
  cpu.value.load_program(program)
  renderTick.value++
}
function irq(k: string) { cpu.value?.raise_irq(k); renderTick.value++ }
function clrIrq()       { cpu.value?.clear_irq(); renderTick.value++ }

// Reactivity helpers — read fresh state every render via the renderTick dep.
function snap<T>(fn: () => T): T { renderTick.value; return fn() }

function disasmShort(instr: number): string {
  if (!cpu.value || instr === 0) return ''
  const d = cpu.value.disasm(instr >>> 0)
  return `${d.mnemonic.padEnd(6)} ${d.operands}`
}
function disasmFull(instr: number): DisasmJson | null {
  if (!cpu.value) return null
  return cpu.value.disasm(instr >>> 0)
}

// Manual edit prompt — shared by all storage tables.
function edit(label: string, current: number, write: (n: number) => void) {
  const v = window.prompt(`new value for ${label} (hex or dec)`, hex(current))
  if (v === null) return
  const n = v.startsWith('0x') ? parseInt(v.slice(2), 16) : Number(v)
  if (Number.isNaN(n)) return
  write(n >>> 0)
  renderTick.value++
}

// Reactive snapshots of storage. The renderTick dep forces re-evaluation
// after every micro_step / poke / IRQ button click.
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

const csrInteresting = new Set([
  0x300,0x302,0x303,0x304,0x305,0x340,0x341,0x342,0x343,0x344,
  0x100,0x104,0x105,0x140,0x141,0x142,0x143,0x144,
])

function rowCls(name: string, m: MicroJson): string {
  if (m.active.includes(name))   return 'active'
  if (m.consumed.includes(name)) return 'consumed'
  return 'held'
}
</script>

<template>
  <div class="ms-root">
    <h1 class="title-row">
      <button class="back-btn" @click="emit('switch-cpu')">← CpuLayout</button>
      <span class="ms-name">RV32I micro-step</span>
      <template v-if="ready && cpu">
        <span class="pill mode" :class="'mode-' + cpu.priv_mode">{{ cpu.priv_mode }}</span>
        <span class="pill" :class="cpu.in_trap ? 'trap-on' : 'trap-off'">
          {{ cpu.in_trap ? 'IN_TRAP' : '—' }}
        </span>
        <template v-if="snap(() => cpu!.micro_state().opcode_name)">
          <span class="pill cls">{{ cpu.micro_state().opcode_name }}</span>
        </template>
        <template v-if="snap(() => cpu!.micro_state().buses.find(b => b.name === 'instr')?.value != null)">
          <span class="pill mnem">{{ disasmFull(cpu.micro_state().buses.find(b => b.name === 'instr')!.value!)?.mnemonic }}</span>
          <span class="pill ops">{{ disasmFull(cpu.micro_state().buses.find(b => b.name === 'instr')!.value!)?.operands }}</span>
        </template>
        <span class="pill phase">{{ cpu.micro_state().current ?? 'idle' }}</span>
      </template>
    </h1>

    <div class="bar">
      <button class="btn alt" @click="micro">Micro Step</button>
      <button class="btn" @click="full">Full Step</button>
      <button class="btn" @click="run10">Run 10</button>
      <button class="btn" @click="reset">Reset</button>
      <span class="sep">|</span>
      <button class="btn" @click="irq('mti')">Trigger MTI</button>
      <button class="btn" @click="irq('mei')">Trigger MEI</button>
      <button class="btn" @click="irq('sei')">Trigger SEI</button>
      <button class="btn" @click="clrIrq">Clear IRQ</button>
      <span class="cycle">cycle: {{ ready && cpu ? cpu.cycle : 0 }}</span>
    </div>

    <template v-if="ready && cpu">
      <div class="grid">
        <!-- Column 1: Privilege+Trap (top) and Components+Control (bottom) -->
        <div class="col">
          <div class="pane short">
            <h3>Privilege &amp; Traps</h3>
            <table>
              <tbody>
                <tr><td class="name">mode</td><td class="val">{{ cpu.priv_mode }}</td></tr>
                <tr><td class="name">in trap</td><td class="val">{{ cpu.in_trap ? 'yes' : 'no' }}</td></tr>
                <tr><td class="name">pending IRQ</td><td class="val">{{ hex(cpu.csr_snapshot().find(c => c.addr === 0x344)?.value ?? 0) }}</td></tr>
              </tbody>
            </table>
            <h3 style="margin-top:.5rem;">Last trap</h3>
            <pre class="trap-log">{{
              cpu.last_trap()
                ? `cycle ${cpu.last_trap()!.cycle}  ${cpu.last_trap()!.from}→${cpu.last_trap()!.to}  ${cpu.last_trap()!.is_interrupt ? 'irq' : 'exc'} cause=${cpu.last_trap()!.cause}  epc=${hex(cpu.last_trap()!.epc)}`
                : '—'
            }}</pre>
          </div>

          <div class="pane grow">
            <h3>Components</h3>
            <table>
              <tbody>
                <tr v-for="c in cpu.micro_state().components" :key="c.name"
                    :class="rowCls(c.name, cpu.micro_state())">
                  <td class="name">{{ c.name }}</td>
                  <td class="val">{{ c.value ?? '—' }}</td>
                </tr>
              </tbody>
            </table>
            <h3 style="margin-top:.6rem;">Control signals</h3>
            <table>
              <tbody>
                <tr v-for="c in cpu.micro_state().controls" :key="c.name"
                    :class="rowCls(c.name, cpu.micro_state())">
                  <td class="name">{{ c.name }}</td>
                  <td class="val">{{ c.value ?? '—' }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- Column 2: Buses (top) + CSRs (bottom) -->
        <div class="col">
          <div class="pane fit">
            <h3>Buses</h3>
            <table>
              <tbody>
                <tr v-for="b in cpu.micro_state().buses" :key="b.name"
                    :class="rowCls(b.name, cpu.micro_state())">
                  <td class="name">{{ b.name }}</td>
                  <td class="val">{{ b.value == null ? '—' : hex(b.value) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div class="pane grow">
            <h3>CSRs</h3>
            <table>
              <tbody>
                <tr v-for="c in cpu.csr_snapshot().filter(c => csrInteresting.has(c.addr))"
                    :key="c.addr"
                    :title="c.desc"
                    @dblclick="edit(c.name, c.value, v => cpu!.csr_write(c.addr, v))">
                  <td class="name">{{ c.name }}(0x{{ c.addr.toString(16).padStart(3,'0') }})</td>
                  <td class="val">{{ hex(c.value) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- Column 3: Registers -->
        <div class="pane">
          <h3>Registers</h3>
          <table>
            <tbody>
              <tr v-for="(v, i) in regList" :key="i"
                  :class="cpu.micro_state().reg_access.includes(i) ? 'consumed' : 'held'"
                  @dblclick="edit(`x${i}`, v, n => cpu!.poke_register(i, n))">
                <td class="name">x{{ i }} {{ REG_NAMES[i] }}</td>
                <td class="val">{{ hex(v) }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- Column 4: RAM -->
        <div class="pane">
          <h3>RAM @ 0x10000</h3>
          <table>
            <tbody>
              <tr v-for="(v, i) in ramList" :key="i"
                  :class="cpu.micro_state().ram_access.includes(0x10000 + i*4) ? 'consumed' : 'held'"
                  @dblclick="edit(hex(0x10000 + i*4), v, n => cpu!.poke_ram(0x10000 + i*4, n))">
                <td class="name">{{ hex(0x10000 + i*4) }}</td>
                <td class="val">{{ hex(v) }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- Column 5: ROM -->
        <div class="pane">
          <h3>ROM @ 0x00000</h3>
          <table>
            <tbody>
              <tr v-for="(v, i) in romList" :key="i"
                  :class="cpu.micro_state().rom_access === i*4 ? 'consumed' : 'held'"
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
    </template>

    <p v-else style="padding:2rem; color:#888;">Loading wasm…</p>
  </div>
</template>

<style scoped>
.ms-root {
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  background: #1e1e1e; color: #ddd; padding: 1.2rem;
  font-size: 13px; height: 100vh; box-sizing: border-box;
  display: flex; flex-direction: column; overflow: hidden;
}
.title-row { display: flex; align-items: center; gap: .4rem; margin: 0 0 .8rem; font-size: 1.15rem; color: #4ec3d5; }
.back-btn { background: #444; color: #fff; border: 0; padding: .35rem .7rem; cursor: pointer; border-radius: 3px; font: inherit; }
.back-btn:hover { background: #666; }
.ms-name { margin-right: .6rem; }
.pill { background: #444; color: #fff; padding: .3rem .6rem; border-radius: 3px; font-variant-numeric: tabular-nums; white-space: pre; }
.pill.mode-M { background: #009977; }
.pill.mode-S { background: #0099cc; }
.pill.mode-U { background: #666; }
.pill.trap-on { background: #cc2222; }
.pill.trap-off { background: #444; }
.pill.cls { background: #009977; min-width: 5.5rem; text-align: center; }
.pill.mnem { background: #cc8800; min-width: 5rem; text-align: center; }
.pill.ops { background: #222; color: #cce; min-width: 14rem; text-align: left; padding-left: .6rem; border: 1px solid #333; }
.pill.phase { background: #cc2222; min-width: 6rem; text-align: center; }

.bar { display: flex; align-items: center; gap: .4rem; margin-bottom: .8rem; }
.btn { background: #2aacbe; color: #000; border: 0; padding: .45rem .9rem; cursor: pointer; font: inherit; border-radius: 4px; }
.btn:hover { background: #4ec3d5; }
.btn.alt { background: #cc8800; }
.btn.alt:hover { background: #e6a31a; }
.sep { color: #666; margin: 0 .3rem; }
.cycle { margin-left: 1rem; color: #888; }

.grid {
  display: grid;
  /* Each column has a minimum content width so the rows aren't truncated. */
  grid-template-columns:
    minmax(200px, 1fr)        /* Components+Privilege          */
    minmax(280px, 1.2fr)      /* Buses + CSRs (long CSR names) */
    minmax(180px, 0.9fr)      /* Registers                     */
    minmax(220px, 1fr)        /* RAM                           */
    minmax(520px, 2.2fr);     /* ROM with disassembly          */
  grid-template-rows: minmax(0, 1fr);
  gap: .6rem;
  flex: 1;
  min-height: 0;
}
.grid > * { min-height: 0; }            /* allow each column/pane to shrink */
.col { display: flex; flex-direction: column; gap: .6rem; min-height: 0; }
.pane { background: #111; padding: .6rem .75rem; border: 1px solid #333; border-radius: 4px; overflow: auto; scrollbar-gutter: stable; display: flex; flex-direction: column; min-height: 0; }
.pane.short { flex: 0 0 auto; }
.pane.grow  { flex: 1; min-height: 0; }
.pane.fit   { flex: 0 0 auto; overflow: visible; }
.pane h3 { color: #cc8800; margin: 0 0 .4rem; font-size: .95rem; position: sticky; top: 0; background: #111; }

table { border-collapse: collapse; width: 100%; }
td { padding: 2px 6px; border-bottom: 1px solid #222; vertical-align: top; white-space: nowrap; }
td.name { color: #0099cc; }
td.val  { text-align: right; font-variant-numeric: tabular-nums; padding-right: 1ch; }
td.dim  { color: #555; padding-left: 10px; padding-right: 1ch; white-space: nowrap; }

tr.active   td.val { color: #00ff66; font-weight: bold; }
tr.consumed td.val { color: #ffff00; font-weight: bold; }
tr.held     td.val { color: #e05050; }
tr.active   td.name { color: #4ec3d5; }
tr.consumed td.name { color: #e6e63a; }
tr.held     td.name { color: #a06060; }

tr { cursor: pointer; }

.trap-log { margin: 0; padding: .25rem .4rem; background: #0a0a0a; border-radius: 3px; color: #cce; font-size: 11px; }
</style>
