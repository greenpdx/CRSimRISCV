<template>
  <div class="pc" :class="{ 'pc--active': active }">

    <div class="pc__header">
      <span class="pc__title">PC</span>
    </div>

    <div class="pc__body">
      <span class="pc__value">{{ fmtHex(pc) }}</span>
    </div>

    <!-- Address output port (left side, toward ROM) -->
    <div class="pc__out">
      <span class="pc__out-label">← addr</span>
    </div>

  </div>
</template>

<script setup lang="ts">
defineProps<{
  pc: number
  nextPc: number
  active?: boolean
}>()

const fmtHex = (v: number) =>
  '0x' + (v >>> 0).toString(16).padStart(8, '0')
</script>

<style scoped>
.pc {
  position: relative;
  display: flex;
  flex-direction: column;
  background: #ffffff;
  border: 1.5px solid #2aacbe;
  border-radius: 3px;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  width: max-content;
  transition: box-shadow 0.15s ease;
}
.pc--active {
  box-shadow: 0 0 0 2px #f59e0b;
}

.pc__header {
  background: #2aacbe;
  padding: 2px 8px;
  text-align: center;
}
.pc__title {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 2px;
  color: #ffffff;
  text-transform: uppercase;
}

.pc__body {
  padding: 4px 8px;
  text-align: center;
}
.pc__value {
  font-size: 11px;
  font-weight: 600;
  color: #0a2030;
  letter-spacing: 0.5px;
}

.pc__out {
  position: absolute;
  right: 100%;
  top: 50%;
  transform: translateY(-50%);
  padding-right: 3px;
}
.pc__out-label {
  font-size: 7px;
  font-weight: 600;
  color: #0099cc;
  white-space: nowrap;
}
</style>
