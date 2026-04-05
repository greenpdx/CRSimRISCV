<template>
  <div class="ctrl" :class="{ 'ctrl--active': active }">

    <div class="ctrl__header">
      <span class="ctrl__title">Decoder / Control</span>
      <span v-if="asmText" class="ctrl__asm ctrl__asm--clickable" @click="emit('asm-click', $event)">{{ asmText }}</span>
      <span class="ctrl__phase-label">{{ phaseText ?? '' }}</span>
    </div>

    <div class="ctrl__body">

      <!-- Decoded instruction type -->
      <div class="ctrl__instr-row">
        <span class="ctrl__instr-type">{{ entry?.name ?? '?' }}</span>
        <span class="ctrl__instr-list">{{ entry?.instrs.join(' · ') ?? '—' }}</span>
      </div>

      <div class="ctrl__divider" />

      <!-- Control signals — absolutely positioned to match vertical control lines -->
      <!-- x-offsets relative to decoder left (x=300): 110, 140, 173, 334, 420, 553, 693 -->
      <div class="ctrl__grid">
        <div class="ctrl__group" style="left: 34px">
          <ControlSignalBadge label="RegWr" :value="signals.regWrite ? 1 : 0" />
        </div>
        <div class="ctrl__group" style="left: 159px">
          <ControlSignalBadge label="ImmSel" :value="signals.immSel" :hint="IMM_FORMAT_LABEL[entry?.immFormat ?? 'none']" :bits="3" />
        </div>
        <div class="ctrl__group" style="left: 230px">
          <ControlSignalBadge label="JALR" :value="signals.pcSel === 2 ? 1 : 0" />
        </div>
        <div class="ctrl__group" style="left: 334px">
          <ControlSignalBadge label="ALUSrc" :value="signals.aluSrc ? 1 : 0" :hint="signals.aluSrc ? 'imm' : 'rs2'" />
        </div>
        <div class="ctrl__group" style="left: 477px">
          <ControlSignalBadge label="+4/Br" :value="signals.pcSel === 1 ? 1 : 0" />
        </div>
        <div class="ctrl__group" style="left: 553px">
          <ControlSignalBadge label="ALUOp" :value="signals.aluOp" :hint="ALU_OP_LABEL[signals.aluOp]" :bits="2" />
        </div>
        <div class="ctrl__group" style="left: 655px">
          <ControlSignalBadge label="WBSel" :value="signals.wbSel" :hint="WB_SEL_LABEL[signals.wbSel]" :bits="2" />
        </div>
      </div>

    </div>

  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import ControlSignalBadge from './ControlSignalBadge.vue'
import {
  OPCODE_BY_VALUE,
  WB_SEL_LABEL,
  ALU_OP_LABEL,
  IMM_FORMAT_LABEL,
  PC_SEL_LABEL,
} from '../data/controlData'
import type { ControlSignals } from '../data/controlData'

const props = defineProps<{
  opcode: number
  active?: boolean
  asmText?: string
  phaseText?: string
}>()

const emit = defineEmits<{
  'asm-click': [event: MouseEvent]
}>()

const entry = computed(() => OPCODE_BY_VALUE.get(props.opcode & 0x7f))

/** Binary string for header display */
const opcodeBin = computed(() =>
  (props.opcode & 0x7f).toString(2).padStart(7, '0')
)

/** Live control signals — default all-zero if opcode unknown */
const signals = computed<ControlSignals>(() =>
  entry.value?.signals ?? {
    regWrite: false, aluSrc: false, aluOp: 0,
    memRead:  false, memWrite: false,
    wbSel: 0, pcSel: 0, immSel: 5,
  }
)
</script>

<style scoped>
.ctrl {
  display: flex;
  flex-direction: column;
  background: #ffffff;
  border: 1.5px solid #cc2222;
  border-radius: 3px;
  overflow: hidden;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  min-width: 703px;
  transition: box-shadow 0.15s ease;
}
.ctrl--active {
  box-shadow: 0 0 0 2px #ff4444;
}

/* Header */
.ctrl__header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  background: #cc2222;
  padding: 4px 8px;
}
.ctrl__title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 1px;
  color: #ffffff;
  text-transform: uppercase;
}
.ctrl__asm {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 1px;
  color: #ffffff;
  text-transform: uppercase;
  margin-left: 5em;
}
.ctrl__asm--clickable {
  cursor: pointer;
  border-bottom: 1px dashed rgba(255,255,255,0.4);
}
.ctrl__asm--clickable:hover {
  color: #f0c060;
}
.ctrl__phase-label {
  font-size: 10px;
  font-weight: 600;
  color: #ffffff;
  margin-left: auto;
}

/* Body */
.ctrl__body {
  padding: 4px 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.ctrl__instr-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.ctrl__instr-type {
  font-size: 11px;
  font-weight: 700;
  color: #cc2222;
}
.ctrl__instr-list {
  font-size: 8px;
  color: #888888;
  white-space: nowrap;
}

.ctrl__divider {
  height: 1px;
  background: #f0d0d0;
  margin: 2px 0;
}

/* Signal grid — positioned to match vertical control lines */
.ctrl__grid {
  position: relative;
  height: 20px;
}
.ctrl__group {
  position: absolute;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  transform: translateX(-50%);
}
.ctrl__section-label {
  font-size: 7px;
  color: #cc2222;
  letter-spacing: 0.5px;
  opacity: 0.7;
}
</style>
