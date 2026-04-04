<template>
  <div class="rom">

    <div class="rom__header">
      <span class="rom__title">Instruction Memory</span>
      <span class="rom__subtitle">ROM · {{ words.length }} words</span>
    </div>

    <!-- Address input port -->
    <div class="rom__addr-port">
      <span class="rom__addr-label">addr</span>
      <span class="rom__addr-value">{{ fmtHex(pc) }}</span>
    </div>

    <!-- Single table with sticky header inside scrollable container -->
    <div class="rom__scroll" ref="scrollEl">
      <table class="rom__table">
        <thead>
          <tr class="rom__col-headers">
            <th class="rom__th">Address</th>
            <th class="rom__th">Bytes</th>
            <th class="rom__th">Assembly</th>
            <th class="rom__th rom__th--dot"></th>
          </tr>
        </thead>
        <tbody>
          <MemoryRow
            v-for="(word, i) in words"
            :key="word.address"
            :word="word"
            kind="rom"
            :stripe="i % 2 === 1"
          />
        </tbody>
      </table>
    </div>

  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import MemoryRow from './MemoryRow.vue'
import type { MemoryWord } from '../types/cpu'

const props = defineProps<{
  /** Full ROM contents — all words, in address order */
  words: MemoryWord[]
  /** Current PC value — used to auto-scroll to active row */
  pc: number
}>()

const scrollEl = ref<HTMLDivElement | null>(null)

const fmtHex = (v: number) =>
  '0x' + (v >>> 0).toString(16).padStart(8, '0')

/** The word currently at the PC */
const activeWord = computed(() =>
  props.words.find(w => w.active) ?? null
)

/**
 * Auto-scroll so the active (PC) row stays visible.
 * Fires whenever the active word changes.
 */
watch(activeWord, async () => {
  await nextTick()
  if (!scrollEl.value || !activeWord.value) return

  const rowIndex = props.words.findIndex(w => w.active)
  if (rowIndex < 0) return

  // Each row is ~18px tall; keep active row in the middle third of the viewport
  const rowH   = 18
  const viewH  = scrollEl.value.clientHeight
  const target = rowIndex * rowH - viewH / 2 + rowH / 2

  scrollEl.value.scrollTo({ top: Math.max(0, target), behavior: 'smooth' })
})
</script>

<style scoped>
.rom {
  display: flex;
  flex-direction: column;
  background: #4ec3d5;
  border: 1.5px solid #2aacbe;
  border-radius: 3px;
  overflow: hidden;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  width: max-content;
}

/* Header */
.rom__header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  background: #2aacbe;
  padding: 4px 8px;
  flex-shrink: 0;
}
.rom__title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 1px;
  color: #ffffff;
  text-transform: uppercase;
}
.rom__subtitle {
  font-size: 9px;
  color: #c0eaf2;
}

/* Address input port */
.rom__addr-port {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 8px;
  background: #3ab8cc;
  flex-shrink: 0;
}
.rom__addr-label {
  font-size: 8px;
  font-weight: 600;
  color: #d0f0f8;
  text-transform: uppercase;
  letter-spacing: 1px;
}
.rom__addr-value {
  font-size: 9px;
  font-weight: 600;
  color: #ffffff;
}

/* Column headers */
.rom__table {
  width: 100%;
  border-collapse: collapse;
}
.rom__col-headers {
  background: #3ab8cc;
}
.rom__th {
  position: sticky;
  top: 0;
  background: #3ab8cc;
  padding: 2px 6px;
  font-size: 9px;
  font-weight: 500;
  letter-spacing: 1px;
  text-transform: uppercase;
  color: #d0f0f8;
  border-bottom: 1px solid #2aacbe;
  text-align: left;
  z-index: 1;
}
.rom__th--dot   { width: 16px; }

/* Scrollable body */
.rom__scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  max-height: 360px;
}

</style>
