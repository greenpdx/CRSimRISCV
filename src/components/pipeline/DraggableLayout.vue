<template>
  <div
    class="drag-layout"
    :style="{ width: frameW + 'px', height: frameH + 'px', marginLeft: frameX + 'px', marginTop: frameY + 'px' }"
    ref="layoutEl"
  >
    <!-- Move handle (top bar) -->
    <div class="drag-layout__move" @mousedown.prevent="startFrameMove">⠿ drag to move</div>

    <!-- Resize handles (corners + edges) -->
    <div class="drag-layout__resize drag-layout__resize--br" @mousedown.prevent="startFrameResize('br', $event)" />
    <div class="drag-layout__resize drag-layout__resize--bl" @mousedown.prevent="startFrameResize('bl', $event)" />
    <div class="drag-layout__resize drag-layout__resize--r" @mousedown.prevent="startFrameResize('r', $event)" />
    <div class="drag-layout__resize drag-layout__resize--b" @mousedown.prevent="startFrameResize('b', $event)" />
    <div class="drag-layout__resize drag-layout__resize--l" @mousedown.prevent="startFrameResize('l', $event)" />
    <div class="drag-layout__resize drag-layout__resize--t" @mousedown.prevent="startFrameResize('t', $event)" />

    <!-- SVG layer for bus wires -->
    <svg ref="svgEl" class="drag-layout__svg" :width="frameW + 400" :height="frameH + 400" :viewBox="`-200 -200 ${frameW + 400} ${frameH + 400}`" style="left: -200px; top: -200px;">
      <g v-for="bus in buses" :key="bus.id">
        <!-- Wire segments -->
        <polyline
          :points="bus.points.map(p => `${p.x},${p.y}`).join(' ')"
          fill="none"
          :stroke="bus.color || '#cc8800'"
          stroke-width="2.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          opacity="0.6"
        />
        <!-- Draggable waypoints -->
        <circle
          v-for="(pt, pi) in bus.points"
          :key="pi"
          :cx="pt.x" :cy="pt.y" r="5"
          :fill="dragTarget?.type === 'buspoint' && dragTarget.busId === bus.id && dragTarget.pointIdx === pi ? '#ff4444' : '#cc8800'"
          stroke="#fff" stroke-width="1"
          class="drag-layout__handle"
          @mousedown.prevent="startBusDrag(bus.id, pi, $event)"
        />
        <!-- Add corner (midpoints) -->
        <circle
          v-for="(_, si) in bus.points.slice(0, -1)"
          :key="'mid' + si"
          :cx="(bus.points[si]!.x + bus.points[si+1]!.x) / 2"
          :cy="(bus.points[si]!.y + bus.points[si+1]!.y) / 2"
          r="4" fill="#4a8" stroke="#fff" stroke-width="1" opacity="0.6"
          class="drag-layout__handle"
          @mousedown.prevent="addBusCorner(bus.id, si, $event)"
        />
        <!-- Bus label -->
        <text
          v-if="bus.points[0]"
          :x="bus.points[0].x" :y="bus.points[0].y - 8"
          font-size="8" :fill="bus.color || '#cc8800'" font-family="JetBrains Mono, monospace"
        >{{ bus.id }}</text>
      </g>

      <!-- Draggable components: HTML inside the SVG via foreignObject so the
           single CTM-based getLocalPos works for both bus points and comps. -->
      <foreignObject
        v-for="comp in components"
        :key="comp.id"
        :x="comp.x"
        :y="comp.y"
        :width="comp.width ?? 200"
        :height="comp.height ?? 120"
        style="overflow: visible;"
      >
        <div
          xmlns="http://www.w3.org/1999/xhtml"
          class="drag-layout__comp"
          :class="{ 'drag-layout__comp--dragging': dragTarget?.type === 'comp' && dragTarget.compId === comp.id }"
          @mousedown.prevent="startCompDrag(comp.id, $event)"
        >
          <div class="drag-layout__comp-label">{{ comp.label }}</div>
          <div class="drag-layout__comp-id">{{ comp.id }}</div>
          <slot :name="comp.id" />
        </div>
      </foreignObject>
    </svg>

    <!-- Toolbar -->
    <div class="drag-layout__toolbar">
      <button class="drag-layout__back" @click="emit('back')">← CpuLayout</button>
      <button @click="exportPositions">Export JSON</button>
      <button @click="addBus">+ Bus</button>
      <span class="drag-layout__info">
        {{ dragTarget ? `Dragging: ${dragTarget.type}` : 'Click & drag to move' }}
      </span>
      <span class="drag-layout__info">Frame: {{ frameW }}×{{ frameH }}</span>
    </div>

    <!-- Export modal -->
    <div v-if="showExport" class="drag-layout__export-overlay" @click.self="showExport = false">
      <div class="drag-layout__export">
        <div class="drag-layout__export-header">
          <span>Exported Positions</span>
          <button @click="showExport = false">X</button>
        </div>
        <textarea class="drag-layout__export-text" readonly :value="exportText" @click="($event.target as HTMLTextAreaElement).select()" />
      </div>
    </div>

    <!-- Right-click context menu for bus points -->
    <div v-if="contextMenu" class="drag-layout__context"
      :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
    >
      <button @click="deletePoint">Delete point</button>
      <button @click="deleteBus">Delete bus</button>
      <button @click="contextMenu = null">Cancel</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, onMounted, onUnmounted } from 'vue'

interface CompPos {
  id: string
  label: string
  x: number
  y: number
  /** Optional foreignObject width in SVG user-space units (default 200). */
  width?: number
  /** Optional foreignObject height in SVG user-space units (default 120). */
  height?: number
}

interface BusPoint { x: number; y: number }

interface BusDef {
  id: string
  color: string
  points: BusPoint[]
}

const props = defineProps<{
  initComponents: CompPos[]
  initBuses: BusDef[]
  initFrameW?: number
  initFrameH?: number
}>()

const emit = defineEmits<{
  back: []
}>()

const layoutEl = ref<HTMLElement | null>(null)
const svgEl = ref<SVGSVGElement | null>(null)

const SNAP_THRESHOLD = 8  // px — snap to H/V within this tolerance

const components = reactive<CompPos[]>([...props.initComponents])
const buses = reactive<BusDef[]>(props.initBuses.map(b => ({
  ...b,
  points: b.points.map(p => ({ ...p })),
})))
const frameW = ref(props.initFrameW ?? 600)
const frameH = ref(props.initFrameH ?? 700)
const frameX = ref(0)
const frameY = ref(0)

interface DragTarget {
  type: 'comp' | 'buspoint' | 'frame' | 'framemove'
  resizeEdge?: string
  compId?: string
  busId?: string
  pointIdx?: number
  offsetX: number
  offsetY: number
}

const dragTarget = ref<DragTarget | null>(null)
const showExport = ref(false)
const exportText = ref('')
const contextMenu = ref<{ x: number; y: number; busId: string; pointIdx: number } | null>(null)

function startCompDrag(id: string, e: MouseEvent) {
  contextMenu.value = null
  const comp = components.find(c => c.id === id)
  if (!comp) return
  // Capture the offset from cursor to component origin in SVG user-space
  // so the component stays under the cursor where it was grabbed.
  const { x, y } = getLocalPos(e)
  dragTarget.value = {
    type: 'comp',
    compId: id,
    offsetX: x - comp.x,
    offsetY: y - comp.y,
  }
}

function startBusDrag(busId: string, pointIdx: number, e: MouseEvent) {
  if (e.button === 2) {
    e.preventDefault()
    contextMenu.value = { x: e.offsetX, y: e.offsetY, busId, pointIdx }
    return
  }
  contextMenu.value = null
  const bus = buses.find(b => b.id === busId)
  if (!bus) return
  const pt = bus.points[pointIdx]
  if (!pt) return
  // Capture the offset from cursor to point center in SVG user-space
  // so the point stays "under the cursor" relative to where it was grabbed.
  const { x, y } = getLocalPos(e)
  dragTarget.value = {
    type: 'buspoint',
    busId,
    pointIdx,
    offsetX: x - pt.x,
    offsetY: y - pt.y,
  }
}

function startFrameResize(edge: string, e: MouseEvent) {
  contextMenu.value = null
  dragTarget.value = { type: 'frame', resizeEdge: edge, offsetX: e.clientX, offsetY: e.clientY }
}

function startFrameMove(e: MouseEvent) {
  contextMenu.value = null
  dragTarget.value = { type: 'framemove', offsetX: e.clientX - frameX.value, offsetY: e.clientY - frameY.value }
}

/** Convert screen-space click coordinates to SVG user-space coordinates.
 *  Uses the SVG element's getScreenCTM() so it's correct regardless of how
 *  the SVG is sized, scrolled, scaled, or transformed by CSS. */
function getLocalPos(e: MouseEvent): { x: number; y: number } {
  const svg = svgEl.value
  if (!svg) return { x: 0, y: 0 }
  const pt = svg.createSVGPoint()
  pt.x = e.clientX
  pt.y = e.clientY
  const ctm = svg.getScreenCTM()
  if (!ctm) return { x: 0, y: 0 }
  const local = pt.matrixTransform(ctm.inverse())
  return { x: local.x, y: local.y }
}

function onMouseMove(e: MouseEvent) {
  if (!dragTarget.value) return
  const { x, y } = getLocalPos(e)

  if (dragTarget.value.type === 'comp') {
    const comp = components.find(c => c.id === dragTarget.value!.compId)
    if (comp) {
      comp.x = Math.round(x - dragTarget.value.offsetX)
      comp.y = Math.round(y - dragTarget.value.offsetY)
    }
  } else if (dragTarget.value.type === 'buspoint') {
    const bus = buses.find(b => b.id === dragTarget.value!.busId)
    if (bus && dragTarget.value.pointIdx !== undefined) {
      const pi = dragTarget.value.pointIdx
      let nx = Math.round(x - dragTarget.value.offsetX)
      let ny = Math.round(y - dragTarget.value.offsetY)

      // Snap to horizontal/vertical alignment with adjacent points
      const prev = bus.points[pi - 1]
      const next = bus.points[pi + 1]
      if (prev) {
        if (Math.abs(ny - prev.y) < SNAP_THRESHOLD) ny = prev.y  // snap horizontal
        if (Math.abs(nx - prev.x) < SNAP_THRESHOLD) nx = prev.x  // snap vertical
      }
      if (next) {
        if (Math.abs(ny - next.y) < SNAP_THRESHOLD) ny = next.y
        if (Math.abs(nx - next.x) < SNAP_THRESHOLD) nx = next.x
      }

      const pt = bus.points[pi]
      if (pt) { pt.x = nx; pt.y = ny }
    }
  } else if (dragTarget.value.type === 'framemove') {
    frameX.value = e.clientX - dragTarget.value.offsetX
    frameY.value = e.clientY - dragTarget.value.offsetY
  } else if (dragTarget.value.type === 'frame') {
    const dx = e.clientX - dragTarget.value.offsetX
    const dy = e.clientY - dragTarget.value.offsetY
    const edge = dragTarget.value.resizeEdge
    if (edge?.includes('r')) frameW.value = Math.max(200, frameW.value + dx)
    if (edge?.includes('b')) frameH.value = Math.max(200, frameH.value + dy)
    if (edge?.includes('l')) { frameW.value = Math.max(200, frameW.value - dx); frameX.value += dx }
    if (edge?.includes('t')) { frameH.value = Math.max(200, frameH.value - dy); frameY.value += dy }
    dragTarget.value.offsetX = e.clientX
    dragTarget.value.offsetY = e.clientY
  }
}

function onMouseUp() {
  dragTarget.value = null
}

// Global mouse tracking so drag works outside the box
onMounted(() => {
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
})
onUnmounted(() => {
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onMouseUp)
})

function addBusCorner(busId: string, segIdx: number, e: MouseEvent) {
  const bus = buses.find(b => b.id === busId)
  if (!bus) return
  const rect = (e.currentTarget as SVGElement).closest('.drag-layout')!.getBoundingClientRect()
  const x = Math.round(e.clientX - rect.left)
  const y = Math.round(e.clientY - rect.top)
  bus.points.splice(segIdx + 1, 0, { x, y })
}

function deletePoint() {
  if (!contextMenu.value) return
  const bus = buses.find(b => b.id === contextMenu.value!.busId)
  if (bus && bus.points.length > 2) {
    bus.points.splice(contextMenu.value.pointIdx, 1)
  }
  contextMenu.value = null
}

function deleteBus() {
  if (!contextMenu.value) return
  const idx = buses.findIndex(b => b.id === contextMenu.value!.busId)
  if (idx >= 0) buses.splice(idx, 1)
  contextMenu.value = null
}

function addBus() {
  const id = `bus_${buses.length}`
  buses.push({
    id,
    color: '#cc8800',
    points: [{ x: 100, y: 100 }, { x: 200, y: 100 }],
  })
}

function exportPositions() {
  const data = {
    frame: { x: frameX.value, y: frameY.value, width: frameW.value, height: frameH.value },
    components: components.map(c => ({ id: c.id, label: c.label, x: c.x, y: c.y })),
    buses: buses.map(b => ({
      id: b.id,
      color: b.color,
      points: b.points.map(p => ({ x: p.x, y: p.y })),
    })),
  }
  exportText.value = JSON.stringify(data, null, 2)
  showExport.value = true
}
</script>

<style scoped>
.drag-layout {
  position: relative;
  background: #fafafa;
  border: 2px solid #0066cc;
  border-radius: 8px;
  overflow: visible;
  user-select: none;
}

.drag-layout__move {
  position: absolute;
  top: -22px;
  left: 0;
  right: 0;
  height: 20px;
  background: #0066cc;
  color: #fff;
  font-size: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: move;
  border-radius: 3px 3px 0 0;
  z-index: 20;
  font-family: 'JetBrains Mono', monospace;
}

.drag-layout__resize { position: absolute; z-index: 20; }
.drag-layout__resize--br { right: -4px; bottom: -4px; width: 12px; height: 12px; background: #0066cc; cursor: nwse-resize; border-radius: 2px; }
.drag-layout__resize--bl { left: -4px; bottom: -4px; width: 12px; height: 12px; background: #0066cc; cursor: nesw-resize; border-radius: 2px; }
.drag-layout__resize--r { right: -3px; top: 20%; width: 6px; height: 60%; background: #0066cc44; cursor: ew-resize; border-radius: 2px; }
.drag-layout__resize--l { left: -3px; top: 20%; width: 6px; height: 60%; background: #0066cc44; cursor: ew-resize; border-radius: 2px; }
.drag-layout__resize--b { bottom: -3px; left: 20%; width: 60%; height: 6px; background: #0066cc44; cursor: ns-resize; border-radius: 2px; }
.drag-layout__resize--t { top: -3px; left: 20%; width: 60%; height: 6px; background: #0066cc44; cursor: ns-resize; border-radius: 2px; }

.drag-layout__svg {
  position: absolute;
  top: 0;
  left: 0;
  pointer-events: none;
  z-index: 5;
}
.drag-layout__svg circle {
  pointer-events: all;
}
.drag-layout__handle {
  cursor: grab;
}
.drag-layout__handle:active { cursor: grabbing; }

.drag-layout__comp {
  /* Position is now controlled by the parent foreignObject (SVG x/y).
     Keep cursor + visual styling. */
  cursor: grab;
  border: 1px dashed transparent;
  padding: 2px;
  border-radius: 3px;
}
.drag-layout__comp:hover {
  border-color: #38bdf8;
}
.drag-layout__comp--dragging {
  border-color: #f59e0b;
  opacity: 0.8;
}
.drag-layout__comp-label {
  font-size: 0;
  height: 0;
  overflow: hidden;
}
.drag-layout__comp-id {
  position: absolute;
  top: -12px;
  left: 0;
  font-size: 7px;
  color: #999;
  font-family: 'JetBrains Mono', monospace;
  pointer-events: none;
  white-space: nowrap;
}

.drag-layout__toolbar {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: #1a1a2e;
  border-bottom: 1px solid #333;
  font-family: 'JetBrains Mono', monospace;
}
.drag-layout__toolbar button {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 4px 12px;
  border: 1px solid #555;
  border-radius: 3px;
  background: #2a2a4a;
  color: #eee;
  cursor: pointer;
}
.drag-layout__toolbar button:hover { background: #3a3a6a; }
.drag-layout__info {
  font-size: 10px;
  color: #888;
  margin-left: auto;
}

.drag-layout__export-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.5);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
}
.drag-layout__export {
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 6px;
  width: 600px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
}
.drag-layout__export-header {
  display: flex;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid #333;
  color: #eee;
  font-size: 12px;
  font-family: 'JetBrains Mono', monospace;
}
.drag-layout__export-header button {
  background: none;
  border: none;
  color: #888;
  cursor: pointer;
  font-size: 14px;
}
.drag-layout__export-text {
  flex: 1;
  padding: 12px;
  background: #0a0a1a;
  color: #8fc;
  border: none;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  resize: none;
  min-height: 300px;
}

.drag-layout__context {
  position: absolute;
  z-index: 50;
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 3px;
  overflow: hidden;
}
.drag-layout__context button {
  display: block;
  width: 100%;
  padding: 6px 12px;
  border: none;
  background: none;
  color: #ccc;
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  text-align: left;
  cursor: pointer;
}
.drag-layout__context button:hover {
  background: #2a2a4a;
}
</style>
