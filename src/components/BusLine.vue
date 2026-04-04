<template>
  <g :id="`bus-${netId}`" class="bus-line">

    <!-- Invisible wide hit area for hover/click -->
    <path
      :d="pathD"
      stroke="transparent"
      :stroke-width="hitWidth"
      fill="none"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="bus-line__hit"
      @mouseenter="hovered = true"
      @mouseleave="hovered = false"
      @click="clicked = !clicked"
    />

    <!-- Main path -->
    <path
      :d="pathD"
      :stroke="busColor"
      :stroke-width="strokeWidth"
      fill="none"
      stroke-linecap="round"
      stroke-linejoin="round"
      :opacity="state === 'idle' ? 0.25 : 0.9"
    />

    <!-- Animated pulse overlay — only when active -->
    <path
      v-if="state === 'active'"
      :d="pathD"
      :stroke="busColor"
      :stroke-width="strokeWidth"
      fill="none"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="bus-line__pulse"
      :style="{ '--path-len': pathLength + 'px' }"
    />

    <!-- Value label — shown on hover, click, or when active+showLabel -->
    <g v-if="showValue">
      <rect
        :x="midpoint.x - labelW / 2 - 3"
        :y="midpoint.y - 14"
        :width="labelW + 6"
        :height="14"
        rx="2"
        :fill="state === 'idle' ? '#333' : busColor"
        opacity="0.9"
      />
      <text
        :x="midpoint.x"
        :y="midpoint.y - 4"
        class="bus-line__label"
        fill="#fff"
        text-anchor="middle"
      >{{ valueLabel }}</text>
    </g>

  </g>
</template>

<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import type { SignalState } from '../types/cpu'

export interface Point { x: number; y: number }

const props = defineProps<{
  netId:      string
  points:     Point[]
  bits:       number
  value:      number
  state:      SignalState
  showLabel?: boolean
}>()

const hovered = ref(false)
const clicked = ref(false)

const showValue = computed(() =>
  hovered.value || clicked.value || (props.state === 'active' && props.showLabel)
)

/** Build SVG path string from waypoints */
const pathD = computed(() => {
  if (props.points.length < 2) return ''
  const [first, ...rest] = props.points
  return `M ${first.x},${first.y} ` +
    rest.map(p => `L ${p.x},${p.y}`).join(' ')
})

/** Stroke width scales with bit width */
const strokeWidth = computed(() => {
  if (props.bits === 1)  return 1.5
  if (props.bits <= 4)   return 2
  if (props.bits <= 8)   return 2.5
  return 3
})

/** Wide invisible hit area */
const hitWidth = computed(() => Math.max(strokeWidth.value * 4, 12))

/** Colour: green for PC feedback, blue for address, amber for data */
const busColor = computed(() => {
  if (props.netId === 'next_pc' || props.netId === 'pc_offset' || props.netId === 'pc_adder_result') return '#009977'
  if (props.netId.includes('_addr') || props.netId === 'pc_out') return '#0099cc'
  return '#cc8800'
})

/** Midpoint for value label */
const midpoint = computed(() => {
  const pts = props.points
  if (pts.length < 2) return { x: 0, y: 0 }
  const mid = Math.floor(pts.length / 2)
  return pts[mid]
})

/** Formatted value for label */
const valueLabel = computed(() => {
  if (props.bits <= 4)  return props.value.toString(2).padStart(props.bits, '0')
  if (props.bits <= 8)  return '0x' + props.value.toString(16).padStart(2, '0')
  return '0x' + (props.value >>> 0).toString(16).padStart(8, '0')
})

/** Approximate label width for background rect */
const labelW = computed(() => valueLabel.value.length * 6)

/** Path length for dash animation */
const pathLength = ref(200)

onMounted(() => {
  const el = document.querySelector(`#bus-${props.netId} path`) as SVGPathElement | null
  if (el) pathLength.value = el.getTotalLength()
})
</script>

<style scoped>
.bus-line__hit {
  cursor: pointer;
  pointer-events: stroke;
}

.bus-line__pulse {
  stroke-dasharray: var(--path-len);
  stroke-dashoffset: var(--path-len);
  animation: bus-pulse 0.6s ease-out forwards;
  pointer-events: none;
}

@keyframes bus-pulse {
  0%   { stroke-dashoffset: var(--path-len); opacity: 1; }
  80%  { stroke-dashoffset: 0;              opacity: 1; }
  100% { stroke-dashoffset: 0;              opacity: 0; }
}

.bus-line__label {
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  font-size: 8px;
  font-weight: 600;
  pointer-events: none;
}
</style>
