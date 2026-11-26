<template>
  <div class="csr">
    <div class="csr__header">
      <span class="csr__title">CSR Registers</span>
    </div>
    <div class="csr__body">
      <div
        class="csr__row"
        v-for="reg in csrList"
        :key="reg.addr"
        :class="{ 'csr__row--active': reg.addr === activeCsr, 'csr__row--help': reg.addr === 0x342 }"
        :title="reg.addr === 0x342 ? causeHelp(valueOf(reg.addr)) : undefined"
      >
        <span class="csr__name">{{ reg.name }}</span>
        <span class="csr__addr">{{ reg.addrHex }}</span>
        <span
          v-if="editingAddr !== reg.addr"
          class="csr__value"
          @dblclick="startEdit(reg.addr)"
        >{{ fmtHex(valueOf(reg.addr)) }}</span>
        <input
          v-else
          ref="editInput"
          class="csr__edit"
          :value="fmtHex(valueOf(reg.addr))"
          @keydown.enter="commitEdit($event, reg.addr)"
          @keydown.escape="editingAddr = -1"
          @blur="commitEdit($event, reg.addr)"
          spellcheck="false"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick } from 'vue'
import { useCpuState } from '../stores/cpuState'

const props = defineProps<{
  activeCsr?: number
  /** CSR values by address (default: the TypeScript engine's CSRs) */
  values?: Record<number, number>
  /** CSR write (default: the TypeScript engine) */
  write?: (addr: number, value: number) => void
}>()

const cpu = useCpuState()
const valueOf = (addr: number) => (props.values ? props.values[addr] : cpu.csrRegisters[addr]) ?? 0
const editingAddr = ref(-1)
const editInput = ref<HTMLInputElement[]>([])

const csrList = [
  { name: 'cycle',   addr: 0xC00, addrHex: 'C00' },
  { name: 'instret', addr: 0xC02, addrHex: 'C02' },
  { name: 'mstatus', addr: 0x300, addrHex: '300' },
  { name: 'mtvec',   addr: 0x305, addrHex: '305' },
  { name: 'mepc',    addr: 0x341, addrHex: '341' },
  { name: 'mcause',  addr: 0x342, addrHex: '342' },
]

// mcause: top bit = interrupt, the rest = cause code
const EXCEPTIONS: Record<number, string> = {
  0: 'instruction address misaligned', 1: 'instruction access fault', 2: 'illegal instruction',
  3: 'breakpoint (EBREAK)', 4: 'load address misaligned', 5: 'load access fault',
  6: 'store address misaligned', 7: 'store access fault', 8: 'ECALL from U mode',
  9: 'ECALL from S mode', 11: 'ECALL from M mode',
}
const INTERRUPTS: Record<number, string> = {
  1: 'supervisor software (SSI)', 3: 'machine software (MSI)', 5: 'supervisor timer (STI)',
  7: 'machine timer (MTI)', 9: 'supervisor external (SEI)', 11: 'machine external (MEI)',
}
function causeHelp(v: number): string {
  const irq = (v >>> 31) === 1, code = v & 0x7fffffff
  const now = `${fmtHex(v)} = ${irq ? 'interrupt' : 'exception'} ${code}: ${(irq ? INTERRUPTS : EXCEPTIONS)[code] ?? 'reserved'}`
  const list = (t: Record<number, string>) => Object.entries(t).map(([c, n]) => `  ${c.padStart(2)}  ${n}`).join('\n')
  return `mcause — why the last trap was taken\n${now}\n\nExceptions (top bit 0):\n${list(EXCEPTIONS)}\n\nInterrupts (top bit 1, 0x8000_00xx):\n${list(INTERRUPTS)}`
}

function fmtHex(v: number): string {
  return '0x' + (v >>> 0).toString(16).padStart(8, '0')
}

function startEdit(addr: number) {
  // Don't edit read-only counters
  if (addr === 0xC00 || addr === 0xC02) return
  editingAddr.value = addr
  nextTick(() => {
    const el = editInput.value?.[0]
    if (el) el.select()
  })
}

function commitEdit(event: Event, addr: number) {
  const val = (event.target as HTMLInputElement).value.trim()
  editingAddr.value = -1
  let num: number
  if (val.startsWith('0x') || val.startsWith('0X')) {
    num = parseInt(val, 16)
  } else {
    num = parseInt(val, 10)
  }
  if (!isNaN(num)) {
    if (props.write) props.write(addr, num)
    else cpu.csrWrite(addr, num)
  }
}
</script>

<style scoped>
.csr {
  display: flex;
  flex-direction: column;
  background: #4ec3d5;
  border: 1.5px solid #2aacbe;
  border-radius: 3px;
  overflow: hidden;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  width: max-content;
}

.csr__header {
  background: #2aacbe;
  padding: 2px 8px;
}
.csr__title {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 1px;
  color: #ffffff;
  text-transform: uppercase;
}

.csr__body {
  padding: 2px 0;
}
.csr__row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 6px;
  transition: background 0.1s;
}
.csr__row:nth-child(even) {
  background: #e8f8fb;
}
.csr__row--active {
  background: #d8f0ff !important;
  font-weight: 600;
}
.csr__row--help { cursor: help; }
.csr__row--help .csr__name { text-decoration: underline dashed; text-underline-offset: 2px; }
.csr__name {
  font-size: 9px;
  font-weight: 600;
  color: #0a2030;
  width: 52px;
}
.csr__addr {
  font-size: 8px;
  color: #6a9ab0;
  width: 24px;
}
.csr__value {
  font-size: 9px;
  color: #0a2030;
  font-weight: 500;
  cursor: pointer;
}
.csr__edit {
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  font-size: 9px;
  width: 75px;
  padding: 1px 3px;
  border: 1px solid #38bdf8;
  border-radius: 2px;
  background: #fff;
  color: #0a2030;
  outline: none;
}
</style>
