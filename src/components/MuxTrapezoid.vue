<template>
  <div class="mux" :class="{ 'mux--active': active }">
    <div
      class="mux__shape"
      :style="vertical
        ? { width: shapeH + 'px', height: shapeW + 'px' }
        : { width: shapeW + 'px', height: shapeH + 'px' }"
    >
      <svg
        class="mux__svg"
        :viewBox="vertical ? `0 0 ${svgH} ${svgW}` : `0 0 ${svgW} ${svgH}`"
        preserveAspectRatio="none"
      >
        <polygon
          :points="polyPoints"
          class="mux__poly"
        />
      </svg>

      <div
        class="mux__content"
        :class="{
          'mux__content--flipped': isFlipped,
          'mux__content--v': vertical,
          'mux__content--up': direction === 'up',
          'mux__content--down': direction === 'down',
        }"
      >
        <!-- Inputs (wide side) -->
        <div class="mux__in-col" :class="{ 'mux__in-col--rev': inputsBottomUp }" :style="inColStyle">
          <div
            v-for="(input, i) in inputs"
            :key="i"
            class="mux__in"
            :class="{ 'mux__in--sel': i === select }"
          >
            <span class="mux__in-idx">{{ i }}</span>
            <span class="mux__in-name">{{ input.name }}</span>
          </div>
        </div>

        <!-- Name (center) -->
        <div class="mux__name-col">
          <span class="mux__name">{{ label }}</span>
        </div>

        <!-- Output label (narrow side) -->
        <div class="mux__out-col">
          <span class="mux__out-label">{{ outputLabel }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export interface MuxInput {
  name: string
  value: number
  bits: number
}

const props = withDefaults(defineProps<{
  label: string
  inputs: MuxInput[]
  select: number
  active?: boolean
  outputLabel?: string
  /** Direction the narrow (output) end points: left (default), right, up, down */
  direction?: 'left' | 'right' | 'up' | 'down'
  /** Number inputs from the bottom (input 0 last) instead of from the top */
  inputsBottomUp?: boolean
  /** Size the shape for at least this many inputs (keeps input positions fixed when inputs come and go) */
  minInputs?: number
}>(), {
  direction: 'left',
})

// Character sizing: compact monospace
const CH_W = 3.75
const LH = 7.5
const COLS = 30
const SHORT_LINES = 5
const linesPerInput = 3
const padLines = 2

const slots = computed(() => Math.max(props.inputs.length, props.minInputs ?? 0))
const longLines = computed(() => linesPerInput * slots.value + padLines * 2)

// Input rows: min-height 14px, gap 6px (see .mux__in / .mux__in-col)
const IN_ROW = 14
const IN_GAP = 6
/**
 * Bottom-up inputs are packed from the bottom edge, with the padding a full
 * set of `slots` rows would have when centred — so input 0 sits at the same
 * place whether or not the optional top inputs are present.
 */
const inColStyle = computed(() => {
  if (!props.inputsBottomUp || vertical.value) return undefined
  const used = slots.value * IN_ROW + (slots.value - 1) * IN_GAP
  return { justifyContent: 'flex-start', paddingBottom: `${(shapeH.value - used) / 2}px` }
})

const shapeW = COLS * CH_W   // 150px
const shapeH = computed(() => longLines.value * LH)

// SVG coordinate space
const svgW = 300
const svgH = computed(() => longLines.value * 20)

// Trapezoid polygon points based on direction
const polyPoints = computed(() => {
  const h = svgH.value
  const shortH = (SHORT_LINES / longLines.value) * h
  const topInset = (h - shortH) / 2
  // Vertical: the long (input) side runs along x, the short side along y
  if (props.direction === 'up') {
    // Narrow on top, wide on bottom
    return `${topInset},0 ${topInset + shortH},0 ${h},${svgW} 0,${svgW}`
  }
  if (props.direction === 'down') {
    // Wide on top, narrow on bottom
    return `0,0 ${h},0 ${topInset + shortH},${svgW} ${topInset},${svgW}`
  }
  if (props.direction === 'right') {
    // Narrow on right, wide on left
    return `0,0 ${svgW},${topInset} ${svgW},${topInset + shortH} 0,${h}`
  }
  // Default: narrow on left, wide on right
  return `0,${topInset} ${svgW},0 ${svgW},${h} 0,${topInset + shortH}`
})

const isFlipped = computed(() => props.direction === 'right')
const vertical = computed(() => props.direction === 'up' || props.direction === 'down')
</script>

<style scoped>
.mux {
  display: inline-flex;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  transition: filter 0.15s ease;
}
.mux--active {
  filter: drop-shadow(0 0 3px #f59e0b);
}

.mux__shape {
  position: relative;
}
.mux__svg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
}
.mux__poly {
  fill: #f5f5f5;
  stroke: #444444;
  stroke-width: 2;
  stroke-linejoin: round;
}
.mux--active .mux__poly {
  stroke: #f59e0b;
}

/* Content overlay: 3 columns — default: inputs right, output left */
.mux__content {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: stretch;
  flex-direction: row-reverse;
}
/* Flipped: inputs left, output right */
.mux__content--flipped {
  flex-direction: row;
}
/* Vertical: down = inputs top, output bottom; up = output top, inputs bottom.
   Text stays upright. */
.mux__content--down { flex-direction: column; }
.mux__content--up   { flex-direction: column-reverse; }
.mux__content--v > .mux__out-col,
.mux__content--v > .mux__name-col,
.mux__content--v > .mux__in-col {
  width: 100%;
  height: 33%;
}
.mux__content--v > .mux__in-col {
  flex-direction: row;
  justify-content: center;
  align-items: center;
  gap: 4px;
  padding: 0 6px;
}
.mux__content--v .mux__in {
  flex-direction: column;
  gap: 0;
  min-height: 0;
}

/* Output label (narrow side) */
.mux__out-col {
  width: 33%;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 4px;
}
.mux__out-label {
  font-size: 8px;
  font-weight: 700;
  color: #009977;
  text-align: center;
  line-height: 1.2;
  word-break: break-word;
}

/* Name (center) */
.mux__name-col {
  width: 33%;
  display: flex;
  align-items: center;
  justify-content: center;
}
.mux__name {
  font-size: 9px;
  font-weight: 700;
  color: #333;
  text-align: center;
  line-height: 1.2;
  word-break: break-word;
}

/* Inputs (wide side) */
.mux__in-col {
  width: 34%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 6px;
  padding: 0 8px;
}
.mux__in-col--rev {
  flex-direction: column-reverse;
}
.mux__content--v > .mux__in-col--rev {
  flex-direction: row-reverse;
}
.mux__in {
  display: flex;
  align-items: center;
  gap: 4px;
  min-height: 14px;
}
.mux__in-idx {
  font-size: 8px;
  color: #aaa;
}
.mux__in-name {
  font-size: 9px;
  color: #888;
  white-space: nowrap;
}
.mux__in--sel .mux__in-idx {
  color: #004060;
  font-weight: 700;
}
.mux__in--sel .mux__in-name {
  color: #004060;
  font-weight: 700;
}
</style>
