<template>
  <g :id="`ctrl-${netId}`" class="ctrl-line">

    <!-- Main path -->
    <path
      :d="pathD"
      stroke="#cc2222"
      stroke-width="1.5"
      fill="none"
      stroke-linecap="round"
      stroke-linejoin="round"
      :stroke-dasharray="effectiveAsserted ? 'none' : '4,3'"
      :opacity="effectiveAsserted ? 0.9 : 0.3"
    />

    <!-- Pulse when transitioning to asserted -->
    <path
      v-if="effectiveAsserted"
      :d="pathD"
      stroke="#ff4444"
      stroke-width="1.5"
      fill="none"
      stroke-linecap="round"
      class="ctrl-line__pulse"
      :style="{ '--path-len': pathLength + 'px' }"
    />

    <!-- Asserted dot at destination end -->
    <circle
      v-if="effectiveAsserted && endPoint"
      :cx="endPoint.x"
      :cy="endPoint.y"
      r="3"
      fill="#cc2222"
      class="ctrl-line__dot"
    />

    <!-- Signal name label -->
    <text
      v-if="showLabel"
      :x="midpoint.x"
      :y="midpoint.y - 4"
      class="ctrl-line__label"
      :fill="effectiveAsserted ? '#cc2222' : '#cc888888'"
      text-anchor="middle"
    >{{ netId.replace('ctrl_', '') }}</text>

  </g>
</template>

<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { useCpuPhases } from '../stores/cpuPhases'
import type { Phase } from '../stores/cpuPhases'

export interface Point { x: number; y: number }

const props = defineProps<{
  netId:     string
  points:    Point[]
  asserted:  boolean
  showLabel?: boolean
  /** If set, this control line only activates during this specific phase */
  phase?:    Phase
}>()

const phases = useCpuPhases()

const effectiveAsserted = computed(() => {
  if (!props.phase) return props.asserted
  return phases.currentPhase === props.phase ? props.asserted : false
})

const pathD = computed(() => {
  if (props.points.length < 2) return ''
  const [first, ...rest] = props.points
  return `M ${first.x},${first.y} ` +
    rest.map(p => `L ${p.x},${p.y}`).join(' ')
})

const endPoint = computed(() =>
  props.points.length > 0
    ? props.points[props.points.length - 1]
    : null
)

const midpoint = computed(() => {
  const pts = props.points
  if (pts.length < 2) return { x: 0, y: 0 }
  return pts[Math.floor(pts.length / 2)]
})

const pathLength = ref(100)

onMounted(() => {
  const el = document.querySelector(`#ctrl-${props.netId} path`) as SVGPathElement | null
  if (el) pathLength.value = el.getTotalLength()
})
</script>

<style scoped>
.ctrl-line__pulse {
  stroke-dasharray: var(--path-len);
  stroke-dashoffset: var(--path-len);
  animation: ctrl-pulse 0.4s ease-out forwards;
}

@keyframes ctrl-pulse {
  0%   { stroke-dashoffset: var(--path-len); opacity: 1; }
  100% { stroke-dashoffset: 0;              opacity: 0; }
}

.ctrl-line__dot {
  animation: dot-pop 0.2s ease-out;
}

@keyframes dot-pop {
  0%   { r: 0; opacity: 0; }
  60%  { r: 5; opacity: 1; }
  100% { r: 3; opacity: 1; }
}

.ctrl-line__label {
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  font-size: 8px;
  pointer-events: none;
  opacity: 0.7;
}
</style>
