<template>
  <div class="ls" :class="{ 'ls--active': op !== 'idle' }">
    <div class="ls__box" :class="{ 'ls__box--mirror': mirror }">

      <!-- Left side: addr, load data, store data (bottom) -->
      <!-- (mirrored: store data, addr, load data from the top, so the wires don't cross) -->
      <div class="ls__left">
        <div v-for="l in dataPorts" :key="l" class="ls__port">
          <span class="ls__port-label">{{ l }}</span>
        </div>
      </div>

      <!-- Center: name -->
      <div class="ls__center">
        <span class="ls__name">Load</span>
        <span class="ls__name">Store</span>
        <span class="ls__op">{{ opLabel }}</span>
      </div>

      <!-- Right side: memory/IO interface -->
      <div class="ls__right">
        <div class="ls__port">
          <span class="ls__port-label">I/O</span>
        </div>
        <div class="ls__port">
          <span class="ls__port-label">Memory</span>
        </div>
      </div>

    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export type LoadStoreOp   = 'idle' | 'load' | 'store'
export type AccessWidth   = 'b' | 'h' | 'w'
export type LoadSigned    = 'signed' | 'unsigned'

const props = defineProps<{
  op:        LoadStoreOp
  address:   number
  width:     AccessWidth
  loadSign?: LoadSigned
  writeData?: number
  readData?:  number
  /** Memory / I/O side on the left (memories to the left of the datapath) */
  mirror?: boolean
}>()

const dataPorts = computed(() => props.mirror ? ['store data', 'addr', 'load data'] : ['addr', 'load data', 'store data'])

const opLabel = computed(() => {
  if (props.op === 'idle')  return 'idle'
  if (props.op === 'load')  return `L${props.width.toUpperCase()}${props.loadSign === 'unsigned' ? 'U' : ''}`
  return `S${props.width.toUpperCase()}`
})
</script>

<style scoped>
.ls {
  display: flex;
  flex-direction: column;
  align-items: center;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  transition: filter 0.15s ease;
}
.ls--active {
  filter: drop-shadow(0 0 3px #f59e0b);
}

.ls__box {
  display: flex;
  align-items: stretch;
  background: #f5f5f5;
  border: 1.5px solid #444;
  border-radius: 2px;
  min-width: 125px;
  min-height: 80px;
}

/* Left: inputs */
.ls__left {
  display: flex;
  flex-direction: column;
  justify-content: space-around;
  padding: 8px 6px;
}

/* Center: name */
.ls__center {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  flex: 1;
  gap: 1px;
  padding: 4px 8px;
}
.ls__name {
  font-size: 9px;
  font-weight: 700;
  color: #333;
  letter-spacing: 0.5px;
}
.ls__op {
  font-size: 8px;
  font-weight: 600;
  color: #f59e0b;
  margin-top: 2px;
}

/* Right: memory/IO interface */
.ls__right {
  display: flex;
  flex-direction: column;
  justify-content: space-around;
  padding: 8px 6px;
}

/* Port labels */
.ls__port-label {
  font-size: 7px;
  color: #888;
  font-weight: 600;
  white-space: nowrap;
}

/* Bottom: load data output */
.ls__bottom {
  padding-top: 2px;
}
.ls__box--mirror { flex-direction: row-reverse; }
.ls__box--mirror .ls__left { align-items: flex-end; }
.ls__box--mirror .ls__right { align-items: flex-start; }
</style>
