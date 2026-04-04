<template>
  <div class="aluctl" :class="{ 'aluctl--active': active }">
    <div class="aluctl__box">
      <!-- Input at top -->
      <div class="aluctl__input">
        <span class="aluctl__in-label">instr</span>
        <span class="aluctl__in-label">ALUOp</span>
      </div>

      <!-- Name centered -->
      <span class="aluctl__name">ALU Control</span>

      <!-- Output at bottom -->
      <div class="aluctl__output">
        <span class="aluctl__out-label">ctrl</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { computeAluControl } from '../data/aluControlData'

const props = defineProps<{
  aluOp:     0 | 1 | 2
  funct3:    number
  funct7b30: 0 | 1
  isLui?:    boolean
  active?:   boolean
}>()

const aluControlOut = computed(() =>
  computeAluControl(props.aluOp, props.funct3, props.funct7b30, props.isLui)
)
</script>

<style scoped>
.aluctl {
  display: inline-flex;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  transition: filter 0.15s ease;
}
.aluctl--active {
  filter: drop-shadow(0 0 3px #f59e0b);
}

.aluctl__box {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: space-between;
  background: #f5f5f5;
  border: 1.5px solid #444;
  border-radius: 2px;
  padding: 8px 16px;
  gap: 6px;
  min-width: 100px;
  min-height: 60px;
}

.aluctl__input {
  display: flex;
  width: 100%;
  justify-content: space-between;
}
.aluctl__in-label {
  font-size: 8px;
  color: #888;
  font-weight: 600;
}

.aluctl__name {
  font-size: 9px;
  font-weight: 700;
  color: #333;
  letter-spacing: 0.5px;
}

.aluctl__output {
  display: flex;
}
.aluctl__out-label {
  font-size: 8px;
  color: #888;
  font-weight: 600;
}
</style>
