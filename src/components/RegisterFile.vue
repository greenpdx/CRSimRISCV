<template>
  <div class="regfile">

    <!-- Header with mode toggle -->
    <div class="regfile__header">
      <span class="regfile__title">Register File</span>
      <div class="regfile__toggle">
        <button
          class="regfile__mode-btn"
          :class="{ 'regfile__mode-btn--active': mode === 'simplified' }"
          @click="mode = 'simplified'"
        >8-reg</button>
        <button
          class="regfile__mode-btn"
          :class="{ 'regfile__mode-btn--active': mode === 'full' }"
          @click="mode = 'full'"
        >32-reg</button>
      </div>
    </div>

    <!-- Single table with sticky header -->
    <div class="regfile__scroll">
      <table class="regfile__table">
        <thead>
          <tr class="regfile__col-headers">
            <th class="regfile__th">Reg</th>
            <th class="regfile__th">xN</th>
            <th class="regfile__th">Hex</th>
            <th class="regfile__th">Dec</th>
            <th class="regfile__th regfile__th--dot"></th>
          </tr>
        </thead>
        <tbody>
          <RegisterRow
            v-for="(row, i) in visibleRows"
            :key="row.def.xNum"
            :reg="row.reg"
            :def="row.def"
            :stripe="i % 2 === 1"
          />
        </tbody>
      </table>
    </div>

    <!-- Footer: active port summary -->
    <div class="regfile__footer">
      <span v-if="rs1" class="regfile__port regfile__port--a">
        rs1 {{ rs1.name }} = {{ fmtHex(rs1.value) }}
      </span>
      <span v-if="rs2" class="regfile__port regfile__port--b">
        rs2 {{ rs2.name }} = {{ fmtHex(rs2.value) }}
      </span>
      <span v-if="rd" class="regfile__port regfile__port--w">
        rd {{ rd.name }} ← {{ fmtHex(rd.value) }}
      </span>
      <span v-if="!rs1 && !rs2 && !rd" class="regfile__port regfile__port--idle">
        idle
      </span>
    </div>

  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import RegisterRow from './RegisterRow.vue'
import { REGISTER_DEFS, REGISTER_DEFS_8 } from '../data/registerData'
import type { RegisterDef } from '../data/registerData'
import type { Register } from '../types/cpu'

// ── Props ────────────────────────────────────────────────────────────────────
const props = defineProps<{
  /** All 32 register runtime values, indexed by xNum */
  registers: Register[]
}>()

// ── Mode toggle ──────────────────────────────────────────────────────────────
const mode = ref<'simplified' | 'full'>('simplified')

// ── Merge static defs with live register values ──────────────────────────────
const visibleRows = computed(() => {
  const defs: RegisterDef[] = mode.value === 'simplified'
    ? REGISTER_DEFS_8
    : REGISTER_DEFS

  return defs.map(def => ({
    def,
    reg: props.registers[def.xNum] ?? {
      name: def.name,
      xNum: def.xNum,
      value: def.resetVal,
    },
  }))
})

// ── Active port summary ───────────────────────────────────────────────────────
const rs1 = computed(() => props.registers.find(r => r.readA))
const rs2 = computed(() => props.registers.find(r => r.readB))
const rd  = computed(() => props.registers.find(r => r.active))

const fmtHex = (v: number) =>
  '0x' + (v >>> 0).toString(16).padStart(8, '0')
</script>

<style scoped>
.regfile {
  display: flex;
  flex-direction: column;
  background: #4ec3d5;
  border: 1.5px solid #2aacbe;
  border-radius: 3px;
  overflow: hidden;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  width: max-content;
}

/* Header */
.regfile__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  background: #2aacbe;
  padding: 4px 8px;
  flex-shrink: 0;
}
.regfile__title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 1px;
  color: #ffffff;
  text-transform: uppercase;
}

/* Toggle */
.regfile__toggle {
  display: flex;
  gap: 2px;
}
.regfile__mode-btn {
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  font-size: 9px;
  padding: 2px 8px;
  border: 1px solid #c0e8f0;
  background: transparent;
  color: #c0e8f0;
  border-radius: 2px;
  cursor: pointer;
  letter-spacing: 0.5px;
  transition: background 0.15s, color 0.15s;
}
.regfile__mode-btn--active {
  background: #ffffff;
  color: #0a6070;
  font-weight: 600;
}

/* Column headers */
.regfile__table {
  width: 100%;
  border-collapse: collapse;
}
.regfile__col-headers {
  background: #3ab8cc;
}
.regfile__th {
  position: sticky;
  top: 0;
  background: #3ab8cc;
  padding: 2px 6px;
  font-size: 9px;
  font-weight: 500;
  letter-spacing: 1px;
  text-transform: uppercase;
  color: #d0f0f8;
  border-bottom: 1px solid #2aacbe;
  text-align: left;
  z-index: 1;
}
.regfile__th--dot  { width: 16px; }

/* Scrollable body */
.regfile__scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  max-height: 340px;
}

/* Footer */
.regfile__footer {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  align-items: center;
  padding: 3px 8px;
  background: #2aacbe;
  min-height: 22px;
  flex-shrink: 0;
}
.regfile__port {
  font-size: 9px;
  padding: 1px 6px;
  border-radius: 2px;
  white-space: nowrap;
}
.regfile__port--a    { background: #bde8ff; color: #004070; }
.regfile__port--b    { background: #c8f0c8; color: #004020; }
.regfile__port--w    { background: #fff0b0; color: #604000; }
.regfile__port--idle { color: #90d8e4; font-size: 9px; }
</style>
