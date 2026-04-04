<template>
  <div class="add" :class="{ 'add--active': active }">
    <!-- Center: + symbol -->
    <div class="add__body">
      <span class="add__plus">+</span>
    </div>

    <!-- Right: offset input -->
    <div class="add__port add__port--right">
      <span class="add__val">{{ fmtOffset(offset) }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  pc: number
  offset: number
  active?: boolean
}>()

const result = computed(() => ((props.pc >>> 0) + props.offset) >>> 0)

const fmtHex = (v: number) =>
  '0x' + (v >>> 0).toString(16).padStart(8, '0')

const fmtOffset = (v: number) =>
  v === 4 ? '+4' : ((v | 0) >= 0 ? '+' : '') + (v | 0)
</script>

<style scoped>
.add {
  position: relative;
  width: 40px;
  height: 40px;
  background: #f5f5f5;
  border: 1.5px solid #009977;
  border-radius: 2px;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  transition: box-shadow 0.15s ease;
}
.add--active {
  box-shadow: 0 0 0 2px #00e6a6;
}

.add__body {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}
.add__plus {
  font-size: 16px;
  font-weight: 700;
  color: #009977;
}

.add__port {
  position: absolute;
  white-space: nowrap;
}
.add__val {
  font-size: 7px;
  color: #333;
}

/* Left: output */
.add__port--left {
  right: 100%;
  top: 50%;
  transform: translateY(-50%);
  padding-right: 3px;
}
.add__port--left .add__val {
  font-weight: 700;
  color: #006080;
}

/* Right: offset input */
.add__port--right {
  left: 100%;
  top: 50%;
  transform: translateY(-50%);
  padding-left: 3px;
}
.add__port--right .add__val {
  color: #009977;
  font-weight: 600;
}

/* Bottom: PC input */
.add__port--bottom {
  top: 100%;
  left: 50%;
  transform: translateX(-50%);
  padding-top: 3px;
}
</style>
