<template>
  <g>
    <path
      :d="d"
      fill="none"
      :stroke="color ?? idle"
      :stroke-width="width"
      :stroke-dasharray="dashed ? '6 4' : undefined"
      :opacity="color ? 0.95 : 0.25"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <polygon v-if="arrow && points.length >= 2" :points="head" :fill="color ?? idle" :opacity="color ? 0.95 : 0.3" />
    <text
      v-if="label"
      :x="labelAt.x"
      :y="labelAt.y + labelDy"
      class="wire__label"
      :fill="color ?? '#374151'"
      text-anchor="middle"
    >{{ label }}</text>
  </g>
</template>

<script setup lang="ts">
import { computed } from 'vue'

/**
 * A wire on the memory panel, drawn like the CPU panels' buses (idle =
 * faint amber, 3 px for 32-bit), lit in `color` when it carries the
 * current access. Optional arrowhead at the end and a label at a corner.
 */
const props = withDefaults(defineProps<{
  points: { x: number; y: number }[]
  /** Lit colour, or null when idle */
  color?: string | null
  width?: number
  arrow?: boolean
  dashed?: boolean
  label?: string
  /** Waypoint index the label sits on (default: middle) */
  labelIdx?: number
  /** Idle colour (default bus amber) */
  idle?: string
  /** Label offset from the waypoint (default 5 px above; positive = below) */
  labelDy?: number
}>(), { color: null, width: 3, arrow: true, dashed: false, idle: '#cc8800', labelDy: -5 })

const d = computed(() => props.points.map((p, i) => `${i ? 'L' : 'M'} ${p.x},${p.y}`).join(' '))
const head = computed(() => {
  const n = props.points.length
  const a = props.points[n - 2]!, b = props.points[n - 1]!
  const ang = Math.atan2(b.y - a.y, b.x - a.x), s = 4 + props.width
  const p = (t: number) => `${b.x - s * Math.cos(ang + t)},${b.y - s * Math.sin(ang + t)}`
  return `${b.x},${b.y} ${p(0.45)} ${p(-0.45)}`
})
const labelAt = computed(() => props.points[props.labelIdx ?? Math.floor(props.points.length / 2)] ?? { x: 0, y: 0 })
</script>

<style scoped>
.wire__label { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 9.5px; font-weight: 700; }
</style>
