<template>
  <div class="mulb" :class="{ 'mulb--active': active }">
    <div class="mulb__box">
      <div class="mulb__in">
        <span class="mulb__port">A <b>{{ hex(a) }}</b></span>
        <span class="mulb__port">B <b>{{ hex(b) }}</b></span>
      </div>
      <div class="mulb__center">
        <span class="mulb__name">MUL</span>
        <span class="mulb__op">{{ opName }}</span>
      </div>
      <div class="mulb__out">
        <span class="mulb__port">Y <b>{{ hex(result) }}</b></span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

/** Zmmul multiplier (single-cycle: combinational) — rs1 × rs2 → WB Sel input 4 */
const props = defineProps<{
  a: number
  b: number
  /** funct3[1:0]: 0 MUL · 1 MULH · 2 MULHSU · 3 MULHU */
  op: number
  result: number
  /** The current instruction is a multiply */
  used: boolean
  active?: boolean
}>()

const opName = computed(() => props.used ? ['MUL', 'MULH', 'MULHSU', 'MULHU'][props.op & 3] : 'idle')
const hex = (v: number) => (v >>> 0).toString(16).padStart(8, '0')
</script>

<style scoped>
.mulb {
  display: inline-flex;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  transition: filter 0.15s ease;
}
.mulb--active { filter: drop-shadow(0 0 3px #f59e0b); }

.mulb__box {
  display: flex;
  align-items: center;
  gap: 10px;
  background: #f5f5f5;
  border: 1.5px solid #444;
  border-radius: 2px;
  padding: 6px 8px;
  min-width: 170px;
  height: 46px;
  box-sizing: border-box;
}
.mulb__in, .mulb__out {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.mulb__port { font-size: 8px; color: #888; font-weight: 600; white-space: nowrap; }
.mulb__port b { color: #333; font-weight: 600; }
.mulb__center {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: 1;
}
.mulb__name { font-size: 10px; font-weight: 700; color: #333; letter-spacing: 1px; }
.mulb__op { font-size: 8px; color: #d97706; font-weight: 700; }
</style>
