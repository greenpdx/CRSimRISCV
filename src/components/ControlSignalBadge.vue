<template>
  <div class="csb" :class="[asserted ? 'csb--on' : 'csb--off', { 'csb--compact': compact }]" :title="hint">
    <span class="csb__label">{{ label }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  label: string
  /** Numeric value */
  value: number
  /** Bit width — 1-bit signals show 0/1, multi-bit show decimal */
  bits?: number
  /** Optional tooltip / hint text */
  hint?: string
  /** Smaller variant (decoder FSM strip) */
  compact?: boolean
}>()

const bits = computed(() => props.bits ?? 1)

const asserted = computed(() => props.value !== 0)

const displayValue = computed(() =>
  bits.value === 1
    ? props.value ? '1' : '0'
    : props.value.toString(10)
)
</script>

<style scoped>
.csb {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 1px 5px;
  border-radius: 2px;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  font-size: 9px;
  cursor: default;
  transition: background 0.1s, color 0.1s;
}
.csb--compact {
  padding: 0 3px;
  font-size: 8px;
}
.csb--compact .csb__label {
  letter-spacing: 0;
}
.csb--off {
  background: #f5f5f5;
  color: #aaaaaa;
}
.csb--on {
  background: #ffe0e0;
  color: #cc2222;
}
.csb__label {
  font-weight: 500;
  letter-spacing: 0.5px;
}
.csb__value {
  font-weight: 700;
  min-width: 8px;
  text-align: right;
}
</style>
