<template>
  <div v-if="visible" class="ipop-overlay" @click.self="close">
    <div class="ipop" :style="posStyle">
      <div class="ipop__header">
        <span class="ipop__mnemonic">{{ mnemonic }}</span>
        <span class="ipop__format">{{ group?.format }}</span>
        <button class="ipop__close" @click="close">X</button>
      </div>
      <div class="ipop__asm">{{ asmText }}</div>
      <div v-if="variant" class="ipop__desc">{{ variant.description }}</div>
      <div v-if="group" class="ipop__operands">
        <span class="ipop__label">Operands:</span> {{ group.operands }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { INSTR_GROUPS } from '../data/instrReference'

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

const posStyle = computed(() => ({
  left: `${posX.value}px`,
  top: `${posY.value}px`,
}))

function show(asm: string, x: number, y: number) {
  asmText.value = asm
  posX.value = x
  posY.value = y
  visible.value = true
}

function close() {
  visible.value = false
}

defineExpose({ show, close })
</script>

<style scoped>
.ipop-overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
}

.ipop {
  position: absolute;
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 6px;
  min-width: 280px;
  max-width: 420px;
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
</style>
