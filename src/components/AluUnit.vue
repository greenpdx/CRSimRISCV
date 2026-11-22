<template>
  <div class="alu" :class="[{ 'alu--active': active }, 'alu--' + (direction ?? 'right')]">

    <!-- Trapezoid: tall on left, narrow on right -->
    <div class="alu__shape">
      <svg class="alu__svg" :viewBox="vertical ? '0 0 100 140' : '0 0 140 100'" preserveAspectRatio="none">
        <polygon
          :points="polyPoints"
          class="alu__poly"
        />
      </svg>

      <div class="alu__content">
        <!-- A and B on the left edge -->
        <div class="alu__left">
          <div class="alu__port">
            <span class="alu__port-label">A</span>
            <span class="alu__port-val">{{ fmtHex(operandA) }}</span>
          </div>
          <div class="alu__port">
            <span class="alu__port-label">B</span>
            <span class="alu__port-val">{{ fmtHex(operandB) }}</span>
          </div>
        </div>

        <!-- ALU label + op + flags centered in shape -->
        <div class="alu__mid">
          <span class="alu__title">ALU</span>
          <span class="alu__op">{{ opName ?? opDef?.mnemonic ?? '—' }}</span>
          <div v-if="!hideFlags" class="alu__flags">
            <span class="alu__flag" :class="{ 'alu__flag--set': flag('zero', 8) }">Z</span>
            <span class="alu__flag" :class="{ 'alu__flag--set': flag('negative', 4) }">N</span>
            <span class="alu__flag" :class="{ 'alu__flag--set': flag('carry', 2) }">C</span>
            <span class="alu__flag" :class="{ 'alu__flag--set': flag('overflow', 1) }">V</span>
          </div>
        </div>

        <!-- Result on the right -->
        <div class="alu__right">
          <span class="alu__port-label">R</span>
          <span class="alu__port-val alu__port-val--result">{{ fmtHex(result ?? computed_.result) }}</span>
        </div>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { aluCompute, ALU_OP_BY_CODE } from '../data/aluData'

const props = defineProps<{
  aluControl: number
  operandA:   number
  operandB:   number
  active?: boolean
  /** Direction the narrow (result) end points: right (default), left, up, down */
  direction?: 'left' | 'right' | 'up' | 'down'
  /** Overrides for the displayed select code / name */
  opName?: string
  result?: number
  /** Z N C V as bits 3..0, when given by the core */
  flags?: number
  hideFlags?: boolean
}>()

const vertical = computed(() => props.direction === 'up' || props.direction === 'down')
const polyPoints = computed(() => {
  switch (props.direction) {
    case 'left': return '0,20 140,0 140,100 0,80'
    case 'up':   return '20,0 80,0 100,140 0,140'
    case 'down': return '0,0 100,0 80,140 20,140'
    default:     return '0,0 140,20 140,80 0,100'
  }
})

const opDef = computed(() => ALU_OP_BY_CODE.get(props.aluControl))

/** A flag: from the `flags` prop when given, else computed (single-cycle codes) */
const flag = (name: 'zero' | 'negative' | 'carry' | 'overflow', bit: number) =>
  props.flags !== undefined ? (props.flags & bit) !== 0 : computed_.value.flags[name]

const computed_ = computed(() =>
  aluCompute(props.aluControl, props.operandA, props.operandB)
)

const fmtHex = (v: number) => '0x' + (v >>> 0).toString(16).padStart(8, '0')
</script>

<style scoped>
.alu {
  display: inline-flex;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  transition: filter 0.15s ease;
}
.alu--active {
  filter: drop-shadow(0 0 4px #f59e0b);
}

/* Trapezoid container */
.alu__shape {
  position: relative;
  width: 160px;
  height: 120px;
}
.alu__svg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
}
.alu__poly {
  fill: #ffffff;
  stroke: #333333;
  stroke-width: 1.5;
  stroke-linejoin: round;
}
.alu--active .alu__poly {
  stroke: #f59e0b;
  stroke-width: 2;
}

/* Content overlaid on shape */
.alu__content {
  position: absolute;
  inset: 0;
}

/* A and B on the left edge */
.alu__left {
  position: absolute;
  left: 6px;
  top: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 20px 0;
}

/* ALU label + op + flags — centered in shape */
.alu__mid {
  position: absolute;
  left: 35%;
  top: 50%;
  transform: translate(-50%, -50%);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
}
.alu__title {
  font-size: 10px;
  font-weight: 700;
  color: #0a2030;
  letter-spacing: 2px;
}
.alu__op {
  font-size: 9px;
  font-weight: 600;
  color: #2a6a80;
}

/* Result — near the narrow right end */
.alu__right {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  flex-direction: column;
  align-items: flex-start;
}

/* Port labels + values (shared) */
.alu__port {
  display: flex;
  align-items: center;
  gap: 2px;
}
.alu__port-label {
  font-size: 8px;
  font-weight: 700;
  color: #cc8800;
}
.alu__port-val {
  font-size: 8px;
  color: #0a2030;
  white-space: nowrap;
}
.alu__port-val--result {
  font-weight: 700;
  color: #004060;
}

/* ── Other directions (text stays upright) ── */
.alu--left .alu__left  { left: auto; right: 6px; align-items: flex-end; }
.alu--left .alu__mid   { left: 65%; }
.alu--left .alu__right { right: auto; left: 6px; }

.alu--up .alu__shape,
.alu--down .alu__shape { width: 120px; height: 160px; }
.alu--up .alu__left,
.alu--down .alu__left {
  left: 0; right: 0; top: 6px; bottom: auto;
  flex-direction: row;
  justify-content: space-around;
  padding: 0 4px;
}
.alu--up .alu__left { top: auto; bottom: 6px; }
.alu--up .alu__port,
.alu--down .alu__port { flex-direction: column; }
.alu--up .alu__mid,
.alu--down .alu__mid { left: 50%; }
.alu--up .alu__mid   { top: 58%; }
.alu--down .alu__mid { top: 42%; }
.alu--up .alu__right,
.alu--down .alu__right {
  right: auto; left: 50%;
  transform: translateX(-50%);
  align-items: center;
}
.alu--up .alu__right   { top: 6px; }
.alu--down .alu__right { top: auto; bottom: 6px; }

/* Flags row */
.alu__flags {
  display: flex;
  gap: 2px;
  margin-top: 2px;
}
.alu__flag {
  font-size: 7px;
  font-weight: 600;
  padding: 1px 3px;
  border-radius: 2px;
  background: #e8e8e8;
  color: #aaaaaa;
  line-height: 1;
}
.alu__flag--set {
  background: #bde8ff;
  color: #004070;
}
</style>
