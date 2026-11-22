<template>
  <tr
    class="mem-row"
    :class="{
      'mem-row--active': word.active,
      'mem-row--stripe': stripe,
      'mem-row--rom': kind === 'rom',
    }"
  >
    <!-- Fixed address — never changes, not selectable -->
    <td class="mem-row__addr">{{ addrHex }}</td>

    <!-- Hex bytes — double-click to edit (RAM only) -->
    <td class="mem-row__bytes" @dblclick="startEdit" v-if="!editing">
      <template v-if="word.active && word.lanes"><span
        v-for="b in [3, 2, 1, 0]" :key="b"
        :class="{ 'mem-row__lane': (word.lanes >> b) & 1 }"
      >{{ byteHex(b) }}{{ b === 2 ? ' ' : '' }}</span></template>
      <template v-else>{{ bytesDisplay }}</template>
    </td>
    <td class="mem-row__bytes" v-else>
      <input
        ref="editInput"
        class="mem-row__edit"
        :value="'0x' + (word.value >>> 0).toString(16).padStart(8, '0')"
        @keydown.enter="commitEdit($event)"
        @keydown.escape="editing = false"
        @blur="commitEdit($event)"
        spellcheck="false"
      />
    </td>

    <!-- ASM (ROM) or decimal (RAM) — optional, slot-driven by parent -->
    <td
      v-if="showAnnotation"
      class="mem-row__annotation"
      :title="kind === 'rom' ? word.asm : undefined"
    >
      {{ word.asm ?? decValue }}
    </td>

    <!-- Active indicator dot -->
    <td class="mem-row__dot">
      <span v-if="word.active" class="mem-row__dot-inner" />
    </td>
  </tr>
</template>

<script setup lang="ts">
import { computed, ref, nextTick } from 'vue'
import type { MemoryWord } from '../types/cpu'

const props = defineProps<{
  word: MemoryWord
  kind: 'rom' | 'ram'
  stripe?: boolean
  /** When set, the address column shows the offset from this base (+0000) */
  base?: number
  /** Hex digits for the offset */
  offsetDigits?: number
}>()

const editing = ref(false)
const editInput = ref<HTMLInputElement | null>(null)

function startEdit() {
  if (props.kind !== 'ram') return // only RAM is editable
  editing.value = true
  nextTick(() => editInput.value?.select())
}

function commitEdit(event: Event) {
  const val = (event.target as HTMLInputElement).value.trim()
  editing.value = false
  let num: number
  if (val.startsWith('0x') || val.startsWith('0X')) {
    num = parseInt(val, 16)
  } else {
    num = parseInt(val, 10)
  }
  if (!isNaN(num)) {
    props.word.value = num >>> 0
  }
}

/** Whether to render the annotation (asm/dec) column */
const showAnnotation = computed(() => props.kind === 'rom' || props.kind === 'ram')

/** Zero-padded 8-digit hex address */
const addrHex = computed(() =>
  props.base === undefined
    ? '0x' + props.word.address.toString(16).padStart(8, '0')
    : '+' + ((props.word.address - props.base) >>> 0).toString(16).padStart(props.offsetDigits ?? 4, '0')
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

/** One byte of the word as 2 hex digits */
const byteHex = (b: number) => ((props.word.value >>> (8 * b)) & 0xff).toString(16).padStart(2, '0')

/** Signed 32-bit decimal — shown in RAM annotation column */
const decValue = computed(() => (props.word.value | 0).toString(10))
</script>

<style scoped>
.mem-row {
  transition: background-color 0.15s ease;
  cursor: pointer;
}
.mem-row--stripe {
  background-color: #e8f8fb;
}
.mem-row__lane {
  color: #d97706;
  text-decoration: underline;
}
.mem-row--active {
  background-color: #d8f0ff !important;
  font-weight: 600;
}

/* ROM: fixed-width table — tighter padding, long assembly truncates with … */
.mem-row--rom > td {
  padding-left: 4px;
  padding-right: 4px;
}
.mem-row--rom .mem-row__annotation {
  overflow: hidden;
  text-overflow: ellipsis;
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

.mem-row__edit {
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  font-size: 10px;
  width: 80px;
  padding: 1px 3px;
  border: 1px solid #38bdf8;
  border-radius: 2px;
  background: #fff;
  color: #0a2030;
  outline: none;
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
