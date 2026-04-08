<script setup lang="ts">
// Live CSR display — shows a curated set of CSRs from the current
// pipeline snapshot, two columns, highlights values that changed
// since the previous cycle.

import { ref } from 'vue'

interface CsrItem { addr: number; name: string; value: number; changed: boolean }

const props = defineProps<{
  entries: CsrItem[]
  privMode: number
}>()

const hoveredAddr = ref<number | null>(null)

const hex = (n: number) => '0x' + (n >>> 0).toString(16).padStart(8, '0')
const privName = (p: number) => p === 3 ? 'M' : p === 1 ? 'S' : 'U'

// CSR descriptions for tooltip on hover.
const CSR_DESC: Record<number, string> = {
  0x300: 'mstatus — machine status: global interrupt enable (MIE), previous-MIE (MPIE), previous priv (MPP), SUM, MXR',
  0x305: 'mtvec — machine trap vector: base address the hart jumps to on any M-mode trap (direct mode in v1)',
  0x341: 'mepc — machine exception PC: address of the instruction that trapped, used as the return target by mret',
  0x342: 'mcause — machine cause: code identifying why the trap was taken (interrupt vs exception, cause number)',
  0x343: 'mtval — machine trap value: extra info about the trap (faulting address, illegal instruction word, etc.)',
  0x304: 'mie — machine interrupt enable: per-source enable bits (MEIE, MTIE, MSIE)',
  0x344: 'mip — machine interrupt pending: per-source pending bits (MEIP, MTIP, MSIP)',
  0x340: 'mscratch — machine scratch: free 32-bit register reserved for trap-handler bookkeeping',
  0x180: 'satp — supervisor address translation and protection: MODE (Bare/Sv32) and root page-table PPN',
  0x100: 'sstatus — supervisor status: subset view of mstatus (SIE, SPIE, SPP, SUM, MXR)',
  0x141: 'sepc — supervisor exception PC: return target for sret',
  0x142: 'scause — supervisor cause: code identifying the trap delivered to S-mode',
  0x105: 'stvec — supervisor trap vector: base address for traps delegated to S-mode',
  0x302: 'medeleg — machine exception delegation: bitmap of exceptions delegated to S-mode',
  0x303: 'mideleg — machine interrupt delegation: bitmap of interrupts delegated to S-mode',
  0x144: 'sip — supervisor interrupt pending: subset view of mip (SSIP, STIP, SEIP)',
  0x104: 'sie — supervisor interrupt enable: subset view of mie (SSIE, STIE, SEIE)',
  0x140: 'sscratch — supervisor scratch: free 32-bit register reserved for the S-mode trap handler',
  0x143: 'stval — supervisor trap value: extra info for traps delivered to S-mode',
  0x3a0: 'pmpcfg0 — physical memory protection config 0: per-region R/W/X/L/A bits for PMP entries 0..3',
}

function csrTooltip(e: CsrItem): string {
  const desc = CSR_DESC[e.addr] ?? e.name
  return `${desc}\naddr: 0x${e.addr.toString(16).padStart(3, '0')}`
}

function hoverDesc(): string {
  if (hoveredAddr.value === null) return 'hover a CSR for a description'
  const e = props.entries.find(x => x.addr === hoveredAddr.value)
  if (!e) return ''
  const desc = CSR_DESC[e.addr] ?? e.name
  return `${e.name} (0x${e.addr.toString(16).padStart(3, '0')}): ${desc}`
}
</script>

<template>
  <section class="csr-panel">
    <header>
      <h3>CSR live state</h3>
      <span class="priv-pill">priv {{ privName(privMode) }}</span>
    </header>
    <div class="csr-grid">
      <div v-for="e in entries" :key="e.addr"
           class="csr-cell" :class="{ changed: e.changed }"
           @mouseenter="hoveredAddr = e.addr"
           @mouseleave="hoveredAddr = null">
        <span class="name">{{ e.name }}</span>
        <span class="val">{{ hex(e.value) }}</span>
      </div>
    </div>
    <div class="csr-desc">{{ hoverDesc() }}</div>
  </section>
</template>

<style scoped>
.csr-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: #1a0f1f;
  border: 1px solid #6a3a8a;
  border-radius: 4px;
  padding: .4rem .55rem;
  overflow: hidden;
  min-width: 0;
  min-height: 0;
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  font-size: 11px;
  color: #ddd;
}
header { display: flex; align-items: center; gap: .5rem; margin-bottom: .35rem; }
h3 { margin: 0; font-size: .8rem; color: #c080e0; }
.priv-pill {
  background: #4a2068; color: #fff; padding: 1px 6px;
  border: 1px solid #c080e0; border-radius: 2px;
  font-size: 10px; margin-left: auto;
}
.csr-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 3px 6px;
  flex: 1;
  min-height: 0;
  overflow: auto;
  align-content: start;
}
.csr-cell {
  display: flex; justify-content: space-between;
  background: #15101a;
  border: 1px solid #3a2050;
  padding: 2px 6px;
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  cursor: help;
}
.csr-cell:hover { background: #2a1838; border-color: #c080e0; }
.csr-desc {
  margin-top: .35rem;
  padding: 4px 6px;
  background: #15101a;
  border: 1px solid #3a2050;
  border-radius: 3px;
  font-size: 10px;
  color: #ffd84d;
  font-style: italic;
  line-height: 1.3;
  height: calc(3 * 1.3em + 8px);
  box-sizing: content-box;
  flex-shrink: 0;
  overflow: hidden;
}
.csr-cell.changed {
  background: #4a2068;
  border-color: #e0a0ff;
  color: #fff;
  box-shadow: 0 0 6px #c080e0;
}
.csr-cell .name { color: #aaa; }
.csr-cell.changed .name { color: #fff; }
.csr-cell .val { color: #ffd84d; }
</style>
