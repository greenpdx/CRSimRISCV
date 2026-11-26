<template>
  <div v-if="visible" class="ipop-overlay" :class="{ 'ipop-overlay--hover': hovering }" @click.self="close">
    <div class="ipop" :style="posStyle" @mouseenter="keep" @mouseleave="leave">
      <div class="ipop__header">
        <span class="ipop__mnemonic">{{ mnemonic }}</span>
        <span v-if="isa" class="ipop__name">{{ isa.name }}</span>
        <span class="ipop__format">{{ group?.format }}</span>
        <button class="ipop__close" @click="close">X</button>
      </div>
      <div class="ipop__asm">{{ asmText }}</div>
      <div v-if="variant" class="ipop__desc">{{ variant.description }}</div>
      <div v-if="group" class="ipop__operands">
        <span class="ipop__label">Operands:</span> {{ group.operands }}
      </div>
      <!-- From the RISC-V ISA Explorer's data -->
      <div v-if="isa" class="ipop__isa">
        <div class="ipop__isa-desc">{{ isa.description }}</div>
        <div class="ipop__isa-row">
          <span class="ipop__label">Assembly:</span> {{ mnemonic.toLowerCase() }} {{ isa.assembly }}
        </div>
        <div class="ipop__isa-row">
          <span class="ipop__label">Encoding:</span>
          <span class="ipop__enc"><span v-for="(f, i) in encFields" :key="i">{{ f }}</span></span>
        </div>
        <div v-if="isa.note" class="ipop__isa-row ipop__isa-note">{{ isa.note }}</div>
        <div class="ipop__isa-row ipop__isa-foot">
          <span v-if="isa.extension"><span class="ipop__label">Extension:</span> {{ isa.extension }}</span>
          <a v-if="isa.explorer" class="ipop__link" :href="isa.explorer" target="_blank" rel="noopener" title="Open this instruction in the RISC-V ISA Explorer">explorer ↗</a>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { INSTR_GROUPS } from '../data/instrReference'
import { useIsaData, isaInstruction, encodingFields } from '../data/isaData'

const isaData = useIsaData()

const visible = ref(false)
const asmText = ref('')
const posX = ref(0)
const posY = ref(0)

const mnemonic = computed(() => {
  const first = asmText.value.trim().split(/[\s,]+/)[0]?.toUpperCase() ?? ''
  return first.replace(/:$/, '') // strip trailing colon (labels)
})

const group = computed(() =>
  INSTR_GROUPS.find(g => g.variants.some(v => v.mnemonic === mnemonic.value)) ?? null
)

const variant = computed(() =>
  group.value?.variants.find(v => v.mnemonic === mnemonic.value) ?? null
)

/** The instruction's entry in the bundled ISA Explorer data */
const isa = computed(() => isaInstruction(isaData.value, mnemonic.value))
const encFields = computed(() => isa.value ? encodingFields(isa.value.encoding) : [])

const posStyle = computed(() => ({
  left: `${posX.value}px`,
  top: `${posY.value}px`,
}))

/** Keep the box inside the window */
function place(x: number, y: number) {
  posX.value = Math.max(4, Math.min(x, window.innerWidth - 450))
  posY.value = Math.max(4, Math.min(y, window.innerHeight - 300))
}

/** Open and stay until closed (a click outside, or X) */
function show(asm: string, x: number, y: number) {
  clearTimeout(timer)
  hovering.value = false
  asmText.value = asm
  place(x, y)
  visible.value = true
}

// ── Hover: opens after a short pause over the text, closes soon after the
//    pointer leaves both the text and the box (so the link can be reached) ──
const hovering = ref(false)
let timer: ReturnType<typeof setTimeout> | undefined
const OPEN_MS = 250, CLOSE_MS = 300

function hover(asm: string, x: number, y: number) {
  if (visible.value && !hovering.value) return   // a clicked-open box stays
  clearTimeout(timer)
  const open = () => { hovering.value = true; asmText.value = asm; place(x, y); visible.value = true }
  if (visible.value) open(); else timer = setTimeout(open, OPEN_MS)
}
/** The pointer left the text (or the box) */
function leave() {
  if (visible.value && !hovering.value) return
  clearTimeout(timer)
  timer = setTimeout(() => { visible.value = false }, CLOSE_MS)
}
/** The pointer is on the box: stay open */
function keep() { clearTimeout(timer) }

function close() {
  clearTimeout(timer)
  visible.value = false
}

defineExpose({ show, hover, leave, close })
</script>

<style scoped>
.ipop-overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
}

/* Hover: the page under the box stays live */
.ipop-overlay--hover { pointer-events: none; }
.ipop-overlay--hover .ipop { pointer-events: auto; }

.ipop {
  position: absolute;
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 6px;
  min-width: 280px;
  max-width: 440px;
  font-family: 'JetBrains Mono', monospace;
  color: #eee;
  box-shadow: 0 4px 20px rgba(0,0,0,0.5);
}

.ipop__header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-bottom: 1px solid #333;
}
.ipop__mnemonic {
  font-size: 13px;
  font-weight: 700;
  color: #8fc;
  letter-spacing: 1px;
}
.ipop__format {
  font-size: 9px;
  color: #888;
}
.ipop__close {
  margin-left: auto;
  background: none;
  border: none;
  color: #888;
  font-size: 12px;
  cursor: pointer;
  font-family: 'JetBrains Mono', monospace;
}
.ipop__close:hover { color: #fff; }

.ipop__asm {
  padding: 6px 12px;
  font-size: 11px;
  color: #f0c060;
  border-bottom: 1px solid #222;
}

.ipop__desc {
  padding: 8px 12px;
  font-size: 10px;
  color: #ccc;
  line-height: 1.5;
}

.ipop__operands {
  padding: 6px 12px;
  font-size: 9px;
  color: #888;
  border-top: 1px solid #222;
}
.ipop__label {
  color: #555;
}
.ipop__name {
  font-size: 10px;
  color: #ccc;
}

.ipop__isa {
  border-top: 1px solid #333;
  padding: 6px 12px 8px;
  font-size: 10px;
  color: #ccc;
  line-height: 1.5;
}
.ipop__isa-row { margin-top: 3px; font-size: 9px; color: #aaa; }
.ipop__isa-note { color: #c9a04a; }
.ipop__isa-foot { display: flex; align-items: baseline; gap: 10px; }
.ipop__enc { color: #8fc; letter-spacing: 0.5px; }
.ipop__enc > span + span { margin-left: 4px; }
.ipop__link { margin-left: auto; color: #7fd4e4; text-decoration: none; }
.ipop__link:hover { text-decoration: underline; }
</style>
