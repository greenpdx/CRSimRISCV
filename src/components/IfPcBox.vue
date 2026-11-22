<template>
  <div class="ifpc" :class="{ 'ifpc--active': active }">

    <div class="ifpc__header">
      <span class="ifpc__title">{{ title ?? 'IF / PC' }}</span>
    </div>

    <div class="ifpc__body">
      <div v-if="showPc ?? true" class="ifpc__row">
        <span class="ifpc__key">pc</span>
        <span class="ifpc__val">{{ fmtHex(pc) }}</span>
      </div>
      <div class="ifpc__row">
        <span class="ifpc__key">ir</span>
        <span class="ifpc__val">{{ instr === undefined ? '—' : fmtHex(instr) }}</span>
      </div>
      <div class="ifpc__asm">
        <span class="ifpc__mnem">{{ mnemonic || '—' }}</span>
        <span class="ifpc__ops">{{ operands }}</span>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  pc: number
  /** Box title (default "IF / PC") */
  title?: string
  /** Show the pc row (default true); false = instruction latch only */
  showPc?: boolean
  /** Instruction word fetched at pc (undefined if pc is outside ROM) */
  instr?: number
  /** Disassembly text, e.g. "ADDI x2, x2, 0x100" */
  asm?: string
  active?: boolean
}>()

const fmtHex = (v: number) =>
  '0x' + (v >>> 0).toString(16).padStart(8, '0')

/** Split "ADDI x2, x2, 0x100" into mnemonic / operands for two-line display */
const split = computed(() => {
  const s = (props.asm ?? '').trim()
  const i = s.search(/\s/)
  return i < 0 ? { m: s, o: '' } : { m: s.slice(0, i), o: s.slice(i + 1).trim() }
})
const mnemonic = computed(() => split.value.m.toLowerCase())
const operands = computed(() => split.value.o)
</script>

<style scoped>
.ifpc {
  display: flex;
  flex-direction: column;
  background: #ffffff;
  border: 1.5px solid #2aacbe;
  border-radius: 3px;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  width: 95px;
  box-sizing: border-box;
  overflow: hidden;
  transition: box-shadow 0.15s ease;
}
.ifpc--active {
  box-shadow: 0 0 0 2px #f59e0b;
}

.ifpc__header {
  background: #2aacbe;
  padding: 2px 6px;
  text-align: center;
}
.ifpc__title {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 1px;
  color: #ffffff;
}

.ifpc__body {
  padding: 3px 5px;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.ifpc__row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}
.ifpc__key {
  font-size: 8px;
  color: #6a9ab0;
}
.ifpc__val {
  font-size: 9px;
  font-weight: 600;
  color: #0a2030;
}

.ifpc__asm {
  margin-top: 2px;
  padding-top: 2px;
  border-top: 1px solid #d0eef3;
  display: flex;
  flex-direction: column;
  line-height: 1.25;
}
.ifpc__mnem {
  font-size: 9px;
  font-weight: 700;
  color: #cc2222;
}
.ifpc__ops {
  font-size: 8px;
  color: #0a2030;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
