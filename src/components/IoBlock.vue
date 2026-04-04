<template>
  <div class="io" :class="{ 'io--active': op !== 'idle' }">

    <div class="io__header">
      <span class="io__title">I/O</span>
      <span class="io__subtitle">{{ fmtHex(baseAddress) }}</span>
    </div>

    <div class="io__body">

      <!-- 8-bit LED indicators — bit 7 left, bit 0 right -->
      <div class="io__leds">
        <div
          v-for="bit in bitIndices"
          :key="bit"
          class="io__led-col"
        >
          <div
            class="io__led"
            :class="getBit(bit) ? 'io__led--on' : 'io__led--off'"
            :title="`bit ${bit} = ${getBit(bit)}`"
          />
          <span class="io__bit-label">{{ bit }}</span>
        </div>
      </div>

      <!-- Byte value below LEDs -->
      <div class="io__byte-row">
        <span class="io__byte-hex">{{ fmtByte(data) }}</span>
        <span class="io__byte-bin">{{ fmtBin(data) }}</span>
      </div>

    </div>

  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export type IoOp = 'idle' | 'read' | 'write'

const props = defineProps<{
  /** Base address of this peripheral, e.g. 0x40020000 */
  baseAddress: number
  /** Effective address being accessed */
  address:     number
  /** Current output data register value */
  data:        number
  /** Operation this cycle */
  op:          IoOp
}>()

/** bit 7 down to bit 0, left to right */
const bitIndices = [7, 6, 5, 4, 3, 2, 1, 0]

const getBit = (bit: number): 0 | 1 =>
  ((props.data >>> bit) & 1) as 0 | 1

const fmtHex  = (v: number) => '0x' + (v >>> 0).toString(16).padStart(8, '0')
const fmtByte = (v: number) => '0x' + (v & 0xff).toString(16).padStart(2, '0')
const fmtBin  = (v: number) => (v & 0xff).toString(2).padStart(8, '0')
</script>

<style scoped>
.io {
  display: flex;
  flex-direction: column;
  background: #ffffff;
  border: 1.5px solid #2aacbe;
  border-radius: 3px;
  overflow: hidden;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  min-width: 200px;
  transition: box-shadow 0.15s;
}
.io--active {
  box-shadow: 0 0 0 2px #f59e0b;
}

/* Header */
.io__header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  background: #2aacbe;
  padding: 4px 8px;
}
.io__title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 2px;
  color: #ffffff;
  text-transform: uppercase;
}
.io__subtitle {
  font-size: 9px;
  color: #c0eaf2;
}

/* Body */
.io__body {
  padding: 5px 10px;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.io__row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.io__label {
  font-size: 9px;
  color: #6a9ab0;
  text-transform: uppercase;
  letter-spacing: 1px;
  width: 32px;
  flex-shrink: 0;
}
.io__value {
  font-size: 11px;
  color: #0a2030;
}

/* Op badge */
.io__op-badge {
  font-size: 8px;
  font-weight: 700;
  padding: 1px 5px;
  border-radius: 2px;
  text-transform: uppercase;
  letter-spacing: 1px;
}
.io__op-badge--idle  { background: #f0f0f0; color: #aaaaaa; }
.io__op-badge--read  { background: #c8eeff; color: #004070; }
.io__op-badge--write { background: #fff0b0; color: #604000; }

.io__divider {
  height: 1px;
  background: #d0e8ef;
  margin: 3px 0;
}

/* LED strip */
.io__leds {
  display: flex;
  justify-content: center;
  gap: 6px;
  padding: 4px 0;
}
.io__led-col {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
}
.io__led {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 1.5px solid #aaaaaa;
  transition: background 0.1s, box-shadow 0.1s;
}
.io__led--off {
  background: #333333;
  border-color: #555555;
}
.io__led--on {
  background: #00e676;
  border-color: #00c853;
  box-shadow: 0 0 6px #00e676;
}
.io__bit-label {
  font-size: 7px;
  color: #aaaaaa;
}

/* Byte summary */
.io__byte-row {
  display: flex;
  justify-content: center;
  align-items: baseline;
  gap: 10px;
  padding-top: 2px;
}
.io__byte-hex {
  font-size: 10px;
  color: #0a2030;
  font-weight: 600;
}
.io__byte-bin {
  font-size: 9px;
  color: #6a9ab0;
  letter-spacing: 1px;
}
</style>
