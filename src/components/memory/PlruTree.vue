<template>
  <svg class="plru" width="112" height="84">
    <text x="56" y="9" text-anchor="middle" class="plru__title">PLRU tree</text>
    <!-- edges: the side each bit points at (the victim side) is bold -->
    <path v-for="(e, i) in EDGES" :key="i" :d="`M${e.x1},${e.y1} L${e.x2},${e.y2}`" :class="['plru__edge', { 'plru__edge--on': edgeOn(i) }]" />
    <!-- nodes with their bit -->
    <g v-for="(n, i) in NODES" :key="'n' + i">
      <circle :cx="n.x" :cy="n.y" r="8" :class="['plru__node', { 'plru__node--chg': changed(i) }]" />
      <text :x="n.x" :y="n.y + 3" text-anchor="middle" class="plru__bit">{{ bit(i) }}</text>
    </g>
    <!-- leaves = entries -->
    <g v-for="l in 4" :key="'l' + l">
      <rect :x="LEAF_X[l - 1]! - 9" y="62" width="18" height="14" rx="2" :class="['plru__leaf', leafClass(l - 1)]" />
      <text :x="LEAF_X[l - 1]" y="72" text-anchor="middle" class="plru__lt">{{ l - 1 }}</text>
    </g>
    <text v-if="next !== null && next !== bits" x="56" y="83" text-anchor="middle" class="plru__chg">{{ b3(bits) }} → {{ b3(next) }}</text>
  </svg>
</template>

<script setup lang="ts">
/**
 * 4-entry tree pseudo-LRU (CVA6 Figs 11–13). Bit 0 = root (0 → victim on
 * the left pair), bit 1 = left pair (0 → entry 0), bit 2 = right pair
 * (0 → entry 2). `next` = the bits after this phase (touch or fill).
 */
const props = defineProps<{
  bits: number
  next: number | null
  /** Entry the tree selects as victim (highlight) */
  victim: number | null
  /** Entry just used (hit or fill) */
  touched: number | null
}>()

const NODES = [{ x: 56, y: 22 }, { x: 30, y: 44 }, { x: 82, y: 44 }]
const LEAF_X = [16, 42, 70, 96]
const EDGES = [
  { x1: 56, y1: 22, x2: 30, y2: 44 }, { x1: 56, y1: 22, x2: 82, y2: 44 },     // root → left / right
  { x1: 30, y1: 44, x2: 16, y2: 62 }, { x1: 30, y1: 44, x2: 42, y2: 62 },     // left → 0 / 1
  { x1: 82, y1: 44, x2: 70, y2: 62 }, { x1: 82, y1: 44, x2: 96, y2: 62 },     // right → 2 / 3
]
const shown = () => props.next ?? props.bits
const bit = (i: number) => (shown() >> i) & 1
const changed = (i: number) => props.next !== null && ((props.next ^ props.bits) >> i) & 1
/** Edge i is the side its node's bit points at */
const edgeOn = (i: number) => {
  const b = shown()
  if (i < 2) return (b & 1) === i
  if (i < 4) return ((b >> 1) & 1) === i - 2
  return ((b >> 2) & 1) === i - 4
}
const leafClass = (i: number) => i === props.victim ? 'plru__leaf--victim' : i === props.touched ? 'plru__leaf--touched' : ''
const b3 = (n: number) => n.toString(2).padStart(3, '0')
</script>

<style scoped>
.plru { display: block; }
.plru__title { font-size: 9px; font-weight: 700; fill: #334155; font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; }
.plru__edge { stroke: #cbd5e1; stroke-width: 1.2; }
.plru__edge--on { stroke: #7c3aed; stroke-width: 2.2; }
.plru__node { fill: #fff; stroke: #64748b; stroke-width: 1.2; }
.plru__node--chg { fill: #fef3c7; stroke: #f59e0b; stroke-width: 2; }
.plru__bit { font-size: 9px; font-weight: 700; fill: #1e293b; font-family: 'JetBrains Mono', monospace; }
.plru__leaf { fill: #fff; stroke: #94a3b8; }
.plru__leaf--victim { fill: #7c3aed; stroke: #7c3aed; }
.plru__leaf--touched { fill: #dbeafe; stroke: #2563eb; stroke-width: 2; }
.plru__leaf--victim + .plru__lt { fill: #fff; }
.plru__lt { font-size: 8px; font-weight: 700; fill: #334155; font-family: 'JetBrains Mono', monospace; }
.plru__chg { font-size: 8.5px; font-weight: 600; fill: #b45309; font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; }
</style>
