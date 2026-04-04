<template>
  <div class="immgen" :class="{ 'immgen--active': active }">

    <!-- Control input on top -->
    <div class="immgen__ctrl">
      <span class="immgen__ctrl-label">immSel</span>
      <span class="immgen__ctrl-val">{{ formatDef?.name ?? '?' }}</span>
    </div>

    <!-- Main body -->
    <div class="immgen__box">
      <!-- Input label (left) -->
      <div class="immgen__port immgen__port--in">
        <span class="immgen__port-label">instr</span>
      </div>

      <!-- Center -->
      <div class="immgen__center">
        <span class="immgen__name">Imm Gen</span>
        <span class="immgen__format">{{ formatDef?.name ?? '?' }}-type</span>
      </div>

      <!-- Output label (right) -->
      <div class="immgen__port immgen__port--out">
        <span class="immgen__port-label">imm</span>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { extractImmediate, IMM_FORMAT_BY_SEL } from '../data/immData'
import type { ImmSel } from '../data/immData'

const props = defineProps<{
  instruction: number
  immSel: ImmSel
  active?: boolean
}>()

const formatDef = computed(() => IMM_FORMAT_BY_SEL.get(props.immSel))

const immediate = computed(() =>
  extractImmediate(props.instruction, props.immSel)
)

const fmtHex = (v: number) =>
  '0x' + (v >>> 0).toString(16).padStart(8, '0')
</script>

<style scoped>
.immgen {
  display: flex;
  flex-direction: column;
  align-items: center;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  transition: filter 0.15s ease;
}
.immgen--active {
  filter: drop-shadow(0 0 3px #00e6a6);
}

/* Control input on top */
.immgen__ctrl {
  display: flex;
  align-items: center;
  gap: 4px;
  padding-bottom: 2px;
}
.immgen__ctrl-label {
  font-size: 7px;
  color: #cc2222;
  font-weight: 600;
  letter-spacing: 0.5px;
}
.immgen__ctrl-val {
  font-size: 7px;
  color: #cc2222;
  font-weight: 700;
}

/* Main rectangular box */
.immgen__box {
  display: flex;
  align-items: center;
  background: #f5f5f5;
  border: 1.5px solid #444;
  border-radius: 2px;
  padding: 6px 4px;
  gap: 6px;
}

/* Port labels on sides */
.immgen__port {
  display: flex;
  align-items: center;
}
.immgen__port-label {
  font-size: 7px;
  font-weight: 600;
  color: #888;
  white-space: nowrap;
}
.immgen__port--in .immgen__port-label {
  color: #0099cc;
}
.immgen__port--out .immgen__port-label {
  color: #cc8800;
}

/* Center label */
.immgen__center {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
  padding: 0 8px;
}
.immgen__name {
  font-size: 8px;
  font-weight: 700;
  color: #333;
  letter-spacing: 0.5px;
}
.immgen__format {
  font-size: 7px;
  color: #2a6a80;
  font-weight: 600;
}
</style>
