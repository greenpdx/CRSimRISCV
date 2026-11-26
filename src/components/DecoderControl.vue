<template>
  <div class="ctrl" :class="{ 'ctrl--active': active, 'ctrl--split': split }">

    <!-- Split layout (single-cycle): ports whose control lines leave the top edge -->
    <div v-if="split" class="ctrl__grid ctrl__grid--top">
      <div v-for="b in splitBadges.top" :key="b.port" class="ctrl__group" :style="badgeLeft(b.port, 0)">
        <ControlSignalBadge :label="b.label" :value="b.value" :hint="b.hint" :bits="b.bits" />
      </div>
    </div>

    <div class="ctrl__header">
      <span class="ctrl__title">{{ title ?? 'Decoder / Control' }}</span>
      <span v-if="asmText" class="ctrl__asm ctrl__asm--clickable" @click="emit('asm-click', $event)" @mouseenter="emit('asm-hover', $event)" @mouseleave="emit('asm-leave')">{{ asmText }}</span>
      <span class="ctrl__phase-label">{{ phaseText ?? '' }}</span>
    </div>

    <div class="ctrl__body">

      <!-- Decoded instruction type -->
      <div class="ctrl__instr-row">
        <span class="ctrl__instr-type">{{ entry?.name ?? '?' }}</span>
        <span class="ctrl__instr-list">{{ entry?.instrs.join(' · ') ?? '—' }}</span>
      </div>

      <template v-if="split">
        <div class="ctrl__divider" />
        <!-- Split layout: ports whose control lines leave the bottom edge -->
        <div class="ctrl__grid ctrl__grid--bottom">
          <div v-for="b in splitBadges.bottom" :key="b.port" class="ctrl__group" :style="badgeLeft(b.port, 0)">
            <ControlSignalBadge :label="b.label" :value="b.value" :hint="b.hint" :bits="b.bits" />
          </div>
        </div>
      </template>

      <div v-if="!split" class="ctrl__divider" />

      <!-- Control signals — absolutely positioned to match vertical control lines -->
      <!-- left: offsets relative to the decoder's left edge; each must line up with its control line.
           Defaults are used when badgeX is not given. -->
      <div v-if="!split" class="ctrl__grid">
        <div v-if="showBadge('reg_write')" class="ctrl__group" :style="badgeLeft('reg_write', 34)">
          <ControlSignalBadge label="RegWr" :value="signals.regWrite ? 1 : 0" />
        </div>
        <div v-if="showBadge('imm_sel')" class="ctrl__group" :style="badgeLeft('imm_sel', 159)">
          <ControlSignalBadge label="ImmSel" :value="signals.immSel" :hint="IMM_FORMAT_LABEL[entry?.immFormat ?? 'none']" :bits="3" />
        </div>
        <div v-if="showBadge('is_jalr')" class="ctrl__group" :style="badgeLeft('is_jalr', 230)">
          <ControlSignalBadge label="JALR" :value="signals.pcSel === 2 ? 1 : 0" />
        </div>
        <div v-if="showBadge('alu_src')" class="ctrl__group" :style="badgeLeft('alu_src', 334)">
          <ControlSignalBadge label="ALUSrc" :value="signals.aluSrc ? 1 : 0" :hint="signals.aluSrc ? 'imm' : 'rs2'" />
        </div>
        <div v-if="showBadge('offset_sel')" class="ctrl__group" :style="badgeLeft('offset_sel', 477)">
          <ControlSignalBadge label="+4/Br" :value="signals.pcSel === 1 ? 1 : 0" />
        </div>
        <div v-if="showBadge('alu_op')" class="ctrl__group" :style="badgeLeft('alu_op', 553)">
          <ControlSignalBadge label="ALUOp" :value="signals.aluOp" :hint="ALU_OP_LABEL[signals.aluOp]" :bits="2" />
        </div>
        <div v-if="showBadge('wb_sel')" class="ctrl__group" :style="badgeLeft('wb_sel', 655)">
          <ControlSignalBadge label="WBSel" :value="signals.wbSel" :hint="WB_SEL_LABEL[signals.wbSel]" :bits="3" />
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
  /** Header title (default "Decoder / Control") */
  title?: string
  /** Single-cycle: signals that differ from the opcode table (MUL → WB Sel 4; illegal MUL → all off) */
  signalsOverride?: Partial<ControlSignals>
  /** Badge x offsets keyed by decoder port id (reg_write, imm_sel, …); missing keys use the defaults */
  badgeX?: Record<string, number>
  /** Single-cycle split layout: port ids shown in a row above the header (lines go up)
      and below the instruction row (lines go down); positions from badgeX */
  split?: { top: string[]; bottom: string[] }
}>()

/** Position of a control-signal badge (centred on its control line) */
const badgeLeft = (port: string, fallback: number) => ({ left: `${props.badgeX?.[port] ?? fallback}px` })
/** Single-cycle control ports: label, value, hint, width */
const portBadge = (port: string) => {
  const g = signals.value
  switch (port) {
    case 'reg_write':  return { label: 'RegWr',  value: g.regWrite ? 1 : 0, hint: 'write rd', bits: 1 }
    case 'imm_sel':    return { label: 'ImmSel', value: g.immSel, hint: IMM_FORMAT_LABEL[entry.value?.immFormat ?? 'none'], bits: 3 }
    case 'is_jalr':    return { label: 'JALR',   value: g.pcSel === 2 ? 1 : 0, hint: 'PC Sel ← JALR target', bits: 1 }
    case 'offset_sel': return { label: '+4/Br',  value: g.pcSel === 1 ? 1 : 0, hint: 'adder offset: 0 = +4, 1 = imm', bits: 1 }
    case 'alu_a':      return { label: 'ASrc',   value: (props.opcode & 0x7f) === 0b0010111 ? 1 : 0, hint: (props.opcode & 0x7f) === 0b0010111 ? 'pc' : 'rs1', bits: 1 }
    case 'alu_src':    return { label: 'ALUSrc', value: g.aluSrc ? 1 : 0, hint: g.aluSrc ? 'imm' : 'rs2', bits: 1 }
    case 'alu_op':     return { label: 'ALUOp',  value: g.aluOp, hint: ALU_OP_LABEL[g.aluOp], bits: 2 }
    case 'wb_sel':     return { label: 'WBSel',  value: g.wbSel, hint: WB_SEL_LABEL[g.wbSel], bits: 3 }
    case 'mem_read':   return { label: 'MemRd',  value: g.memRead ? 1 : 0, hint: 'load', bits: 1 }
    case 'mem_write':  return { label: 'MemWr',  value: g.memWrite ? 1 : 0, hint: 'store', bits: 1 }
    default:           return { label: port, value: 0, hint: '', bits: 1 }
  }
}
const splitBadges = computed(() => ({
  top: (props.split?.top ?? []).map(port => ({ port, ...portBadge(port) })),
  bottom: (props.split?.bottom ?? []).map(port => ({ port, ...portBadge(port) })),
}))

/** With badgeX given, show only the badges that have a decoder port */
const showBadge = (port: string) => !props.badgeX || port in props.badgeX

const emit = defineEmits<{
  'asm-click': [event: MouseEvent]
  'asm-hover': [event: MouseEvent]
  'asm-leave': []
}>()

const entry = computed(() => OPCODE_BY_VALUE.get(props.opcode & 0x7f))

/** Binary string for header display */
const opcodeBin = computed(() =>
  (props.opcode & 0x7f).toString(2).padStart(7, '0')
)

/** Live control signals — default all-zero if opcode unknown */
const signals = computed<ControlSignals>(() => ({
  ...(entry.value?.signals ?? {
    regWrite: false, aluSrc: false, aluOp: 0,
    memRead:  false, memWrite: false,
    wbSel: 0, pcSel: 0, immSel: 5, csrOp: 0,
  }),
  ...props.signalsOverride,
}))
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
  min-width: 685px;
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
  gap: 14px;
  white-space: nowrap;
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
  margin-left: 1em;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ctrl__asm--clickable {
  cursor: help;
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
.ctrl__grid--top,
.ctrl__grid--bottom { height: 22px; margin: 0 8px; }
.ctrl__grid--top { margin-top: 2px; }
.ctrl__grid--bottom { margin: 0; }
.ctrl__grid--top .ctrl__group,
.ctrl__grid--bottom .ctrl__group { top: 2px; }
/* Split layout: tighter rows */
.ctrl--split { min-width: 0; width: 100%; box-sizing: border-box; }
.ctrl--split .ctrl__header { padding: 1px 8px; line-height: 14px; }
.ctrl--split .ctrl__body { padding: 1px 8px; gap: 0; }
.ctrl--split .ctrl__instr-row { line-height: 13px; }
.ctrl--split .ctrl__divider { margin: 1px 0; }
.ctrl--split .ctrl__grid--top,
.ctrl--split .ctrl__grid--bottom { height: 18px; }
.ctrl--split .ctrl__grid--top { margin-top: 0; }
.ctrl--split .ctrl__grid--top .ctrl__group,
.ctrl--split .ctrl__grid--bottom .ctrl__group { top: 1px; }
.ctrl__section-label {
  font-size: 7px;
  color: #cc2222;
  letter-spacing: 0.5px;
  opacity: 0.7;
}
</style>
