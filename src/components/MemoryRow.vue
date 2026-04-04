<template>
  <tr
    class="mem-row"
    :class="{
      'mem-row--active': word.active,
      'mem-row--stripe': stripe,
    }"
  >
    <!-- Fixed address — never changes, not selectable -->
    <td class="mem-row__addr">{{ addrHex }}</td>

    <!-- Hex bytes — mutable, updated by simulator -->
    <td class="mem-row__bytes">{{ bytesDisplay }}</td>

    <!-- ASM (ROM) or decimal (RAM) — optional, slot-driven by parent -->
    <td v-if="showAnnotation" class="mem-row__annotation">
      {{ word.asm ?? decValue }}
    </td>

    <!-- Active indicator dot -->
    <td class="mem-row__dot">
      <span v-if="word.active" class="mem-row__dot-inner" />
    </td>
  </tr>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { MemoryWord } from '../types/cpu'

const props = defineProps<{
  word: MemoryWord
  /** 'rom' shows asm column, 'ram' shows signed decimal column */
  kind: 'rom' | 'ram'
  /** Alternating background stripe */
  stripe?: boolean
}>()

/** Whether to render the annotation (asm/dec) column */
const showAnnotation = computed(() => props.kind === 'rom' || props.kind === 'ram')

/** Zero-padded 8-digit hex address */
const addrHex = computed(() =>
  '0x' + props.word.address.toString(16).padStart(8, '0')
)

/**
 * Display the word value as two 16-bit hex halves.
 * e.g. 0x00100513 → "0010 0513"
 */
const bytesDisplay = computed(() => {
  const w = props.word.value >>> 0
  const hi = (w >>> 16) & 0xffff
  const lo =  w         & 0xffff
  return hi.toString(16).padStart(4, '0') + ' ' + lo.toString(16).padStart(4, '0')
})

/** Signed 32-bit decimal — shown in RAM annotation column */
const decValue = computed(() => (props.word.value | 0).toString(10))
</script>

<style scoped>
.mem-row {
  transition: background-color 0.15s ease;
}
.mem-row--stripe {
  background-color: #e8f8fb;
}
.mem-row--active {
  background-color: #d8f0ff !important;
  font-weight: 600;
}

/* Address — fixed, dimmed, non-selectable */
.mem-row__addr {
  padding: 2px 6px;
  font-size: 10px;
  color: #6a9ab0;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  white-space: nowrap;
  user-select: none;
}

/* Hex bytes */
.mem-row__bytes {
  padding: 2px 6px;
  font-size: 10px;
  color: #0a2030;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  white-space: nowrap;
  text-align: left;
}

/* ASM (ROM) or decimal (RAM) */
.mem-row__annotation {
  padding: 2px 6px;
  font-size: 10px;
  color: #2a6090;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  white-space: nowrap;
  text-align: left;
}

/* Active dot */
.mem-row__dot {
  padding: 0 4px;
  width: 10px;
}
.mem-row__dot-inner {
  display: inline-block;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #38bdf8;
}
</style>
