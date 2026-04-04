<template>
  <div class="ram">

    <div class="ram__header">
      <span class="ram__title">Data Memory</span>
      <span class="ram__subtitle">RAM · {{ words.length }} words</span>
    </div>

    <!-- Single table with sticky header inside scrollable container -->
    <div class="ram__scroll" ref="scrollEl">
      <table class="ram__table">
        <thead>
          <tr class="ram__col-headers">
            <th class="ram__th">Address</th>
            <th class="ram__th">Bytes</th>
            <th class="ram__th">Dec</th>
            <th class="ram__th ram__th--dot"></th>
          </tr>
        </thead>
        <tbody>
          <MemoryRow
            v-for="(word, i) in words"
            :key="word.address"
            :word="word"
            kind="ram"
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

export type RamOp = 'read' | 'write' | 'idle'

const props = defineProps<{
  /** Full RAM contents — all words, in address order */
  words: MemoryWord[]
  /** Last memory operation this cycle */
  lastOp: RamOp
  /** Address involved in the last operation */
  lastAddress: number
  /** Value read or written in the last operation */
  lastValue: number
}>()

const scrollEl = ref<HTMLDivElement | null>(null)

/** The word currently being accessed (active flag set by simulator) */
const activeWord = computed(() =>
  props.words.find(w => w.active) ?? null
)

/**
 * Auto-scroll to keep the active row visible on read/write.
 */
watch(activeWord, async () => {
  await nextTick()
  if (!scrollEl.value || !activeWord.value) return

  const rowIndex = props.words.findIndex(w => w.active)
  if (rowIndex < 0) return

  const rowH  = 18
  const viewH = scrollEl.value.clientHeight
  const target = rowIndex * rowH - viewH / 2 + rowH / 2

  scrollEl.value.scrollTo({ top: Math.max(0, target), behavior: 'smooth' })
})
</script>

<style scoped>
.ram {
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
.ram__header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  background: #2aacbe;
  padding: 4px 8px;
  flex-shrink: 0;
}
.ram__title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 1px;
  color: #ffffff;
  text-transform: uppercase;
}
.ram__subtitle {
  font-size: 9px;
  color: #c0eaf2;
}

/* Table */
.ram__table {
  width: 100%;
  border-collapse: collapse;
}
.ram__col-headers {
  background: #3ab8cc;
}
.ram__th {
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
.ram__th--dot { width: 16px; }

/* Scrollable body */
.ram__scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  max-height: 360px;
}
</style>
