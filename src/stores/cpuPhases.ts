// ─────────────────────────────────────────────────────────────────────────────
// Phased CPU execution — breaks each instruction into 7 visual micro-steps
// Each phase computes values AND activates the relevant buses
// Previous phases' buses stay active (cumulative reveal)
// ─────────────────────────────────────────────────────────────────────────────

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { useCpuState } from './cpuState'
import { OPCODE_BY_VALUE, BRANCH_CONDITION } from '../data/controlData'
import type { ControlSignals } from '../data/controlData'
import { extractImmediate } from '../data/immData'
import type { ImmSel } from '../data/immData'
import { computeAluControl } from '../data/aluControlData'
import { aluCompute } from '../data/aluData'

export type Phase =
  | 'idle'
  | 'fetch'
  | 'decode'
  | 'regread'
  | 'execute'
  | 'memwb'
  | 'pcupdate'

const PHASE_ORDER: Phase[] = [
  'fetch', 'decode', 'regread', 'execute', 'memwb', 'pcupdate'
]

const PHASE_LABELS: Record<Phase, string> = {
  idle:      'Ready',
  fetch:     '1. Fetch — PC → ROM → Instruction',
  decode:    '2. Decode — Control signals',
  regread:   '3. Register Read + Imm Gen',
  execute:   '4. Execute — ALU',
  memwb:     '5. Memory + Write Back',
  pcupdate:  '6. PC Update — Next PC',
}

// Shared context across phases for one instruction
interface InstrContext {
  instr: number
  opcode: number
  rd: number
  funct3: number
  rs1Num: number
  rs2Num: number
  funct7b30: number
  entry: ReturnType<typeof OPCODE_BY_VALUE.get>
  sig: ControlSignals
  isLui: boolean
  immediate: number
  rs1Val: number
  rs2Val: number
  aluB: number
  aluCtrlCode: number
  aluResult: number
  flags: { zero: boolean; negative: boolean; carry: boolean; overflow: boolean }
  memReadData: number
  wbData: number
  pcPlus4: number
  csrAddr: number
  csrOldVal: number
  csrOp: number
}

export const useCpuPhases = defineStore('cpuPhases', () => {

  const cpu = useCpuState()

  const currentPhase = ref<Phase>('idle')
  const phaseIndex = ref(-1)
  const animating = ref(false)
  const autoRunning = ref(false)
  const animDuration = ref(400)

  let autoTimer: ReturnType<typeof setInterval> | null = null
  let ctx: InstrContext | null = null

  const phaseLabel = computed(() => PHASE_LABELS[currentPhase.value])

  // ── Phase functions ──────────────────────────────────────────────────────

  function phaseFetch() {
    cpu.clearActiveFlags()
    cpu.clearBusStates()
    cpu.cycleCount++
    cpu.csrIncrementCycle()
    cpu.csrIncrementInstret()

    const instr = cpu.romRead(cpu.pc)
    cpu.lastInstr = instr
    cpu.setBus('pc_out', cpu.pc)
    cpu.setBus('instr', instr)

    const romWord = cpu.romWords.find(w => w.address === cpu.pc)
    if (romWord) romWord.active = true

    const pcPlus4 = (cpu.pc >>> 0) + 4

    // Initialize context
    ctx = {
      instr,
      opcode: instr & 0x7f,
      rd: (instr >> 7) & 0x1f,
      funct3: (instr >> 12) & 0x07,
      rs1Num: (instr >> 15) & 0x1f,
      rs2Num: (instr >> 20) & 0x1f,
      funct7b30: (instr >> 30) & 0x1,
      entry: undefined,
      sig: { regWrite: false, aluSrc: false, aluOp: 0, memRead: false, memWrite: false, wbSel: 0, pcSel: 0, immSel: 5, csrOp: 0 },
      isLui: false,
      csrAddr: 0, csrOldVal: 0, csrOp: 0,
      immediate: 0,
      rs1Val: 0, rs2Val: 0,
      aluB: 0, aluCtrlCode: 0,
      aluResult: 0, flags: { zero: false, negative: false, carry: false, overflow: false },
      memReadData: 0, wbData: 0,
      pcPlus4,
    }
  }

  function phaseDecode() {
    if (!ctx) return

    const entry = OPCODE_BY_VALUE.get(ctx.opcode)
    ctx.entry = entry
    cpu.lastOpEntry = entry

    if (!entry) {
      // Unknown opcode — treat as NOP
      ctx.sig = { regWrite: false, aluSrc: false, aluOp: 0, memRead: false, memWrite: false, wbSel: 0, pcSel: 0, immSel: 5, csrOp: 0 }
      return
    }

    ctx.sig = { ...entry.signals }
    ctx.isLui = ctx.opcode === 0b0110111

    // CSR: derive csrOp from funct3 for SYSTEM opcode
    if (ctx.opcode === 0b1110011) {
      const f3 = ctx.funct3 & 0x7
      // funct3: 001=CSRRW, 010=CSRRS, 011=CSRRC, 101=CSRRWI, 110=CSRRSI, 111=CSRRCI
      if (f3 === 1 || f3 === 5) ctx.sig.csrOp = 1       // RW
      else if (f3 === 2 || f3 === 6) ctx.sig.csrOp = 2   // RS (set)
      else if (f3 === 3 || f3 === 7) ctx.sig.csrOp = 3   // RC (clear)
      ctx.csrAddr = (ctx.instr >>> 20) & 0xFFF
      ctx.csrOp = ctx.sig.csrOp
    }

    cpu.setCtrl('ctrl_reg_write', ctx.sig.regWrite)
    cpu.setCtrl('ctrl_alu_src', ctx.sig.aluSrc)
    cpu.setCtrl('ctrl_mem_read', ctx.sig.memRead)
    cpu.setCtrl('ctrl_mem_write', ctx.sig.memWrite)
    cpu.setBus('ctrl_alu_op', ctx.sig.aluOp)
    cpu.setBus('ctrl_wb_sel', ctx.sig.wbSel)
    cpu.setBus('ctrl_imm_sel', ctx.sig.immSel)
    cpu.setBus('ctrl_pc_sel', ctx.sig.pcSel)
  }

  function phaseRegRead() {
    if (!ctx) return

    // Immediate generation
    ctx.immediate = extractImmediate(ctx.instr, ctx.sig.immSel as ImmSel)
    cpu.setBus('immediate', ctx.immediate >>> 0)

    // Register read
    ctx.rs1Val = cpu.regRead(ctx.rs1Num)
    ctx.rs2Val = cpu.regRead(ctx.rs2Num)

    if (ctx.rs1Num !== 0 || !ctx.sig.aluSrc) {
      const r1 = cpu.registers[ctx.rs1Num]
      if (r1) r1.readA = true
    }
    if (!ctx.sig.aluSrc) {
      const r2 = cpu.registers[ctx.rs2Num]
      if (r2) r2.readB = true
    }

    cpu.setBus('rs1_data', ctx.rs1Val >>> 0)
    cpu.setBus('rs2_data', ctx.rs2Val >>> 0)

    // ALU src mux output — computed here so it highlights during regread
    ctx.aluB = ctx.sig.aluSrc ? ctx.immediate : ctx.rs2Val
    cpu.setBus('alu_b', ctx.aluB >>> 0)
  }

  function phaseExecute() {
    if (!ctx) return

    // ALU src mux
    ctx.aluB = ctx.sig.aluSrc ? ctx.immediate : ctx.rs2Val
    cpu.setBus('alu_b', ctx.aluB >>> 0)

    // ALU control
    // For I-type ALU (opcode 0010011), funct7b30 is part of the immediate,
    // not a function selector. Only R-type (0110011) and shifts (SRAI) use it.
    const isRtype = ctx.opcode === 0b0110011
    const isShiftImm = ctx.opcode === 0b0010011 && (ctx.funct3 === 0b101 || ctx.funct3 === 0b001)
    const effectiveFunct7b30 = (isRtype || isShiftImm) ? (ctx.funct7b30 as 0 | 1) : 0

    ctx.aluCtrlCode = computeAluControl(
      ctx.sig.aluOp as 0 | 1 | 2,
      ctx.funct3,
      effectiveFunct7b30,
      ctx.isLui,
    )
    cpu.setBus('alu_control', ctx.aluCtrlCode)

    // ALU compute
    const { result, flags } = aluCompute(ctx.aluCtrlCode, ctx.rs1Val, ctx.aluB)
    ctx.aluResult = result
    ctx.flags = flags
    cpu.setBus('alu_result', result)
    cpu.setCtrl('zero_flag', flags.zero)

    // CSR: read old value and write new value
    if (ctx.csrOp !== 0) {
      // For immediate variants (funct3 >= 5), use rs1 field as 5-bit zero-extended immediate
      const csrSrcVal = (ctx.funct3 >= 5) ? ctx.rs1Num : ctx.rs1Val
      ctx.csrOldVal = cpu.csrOp(ctx.csrAddr, csrSrcVal, ctx.csrOp)
      cpu.setBus('csr_read_data', ctx.csrOldVal)
    }
  }

  function phaseMemWb() {
    if (!ctx) return

    // Re-activate buses needed for memory
    cpu.setBus('alu_result', ctx.aluResult)
    cpu.setBus('rs2_data', ctx.rs2Val >>> 0)

    // Memory access (only if load/store)
    ctx.memReadData = 0

    if (ctx.sig.memRead) {
      const addr = ctx.aluResult
      cpu.setBus('ctrl_mem_read', 1)
      if (cpu.isIoAddress(addr)) {
        ctx.memReadData = cpu.ioData
        cpu.ioOp = 'read'
      } else {
        ctx.memReadData = cpu.ramRead(addr)
      }
      cpu.setBus('mem_read_data', ctx.memReadData >>> 0)
    }

    if (ctx.sig.memWrite) {
      const addr = ctx.aluResult
      cpu.setBus('ctrl_mem_write', 1)
      if (cpu.isIoAddress(addr)) {
        cpu.ioData = ctx.rs2Val >>> 0
        cpu.ioOp = 'write'
      } else {
        cpu.ramWrite(addr, ctx.rs2Val)
      }
    }

    // Write back
    ctx.wbData = 0
    switch (ctx.sig.wbSel) {
      case 0: ctx.wbData = ctx.aluResult;    break
      case 1: ctx.wbData = ctx.memReadData;  break
      case 2: ctx.wbData = ctx.pcPlus4;      break
      case 3: ctx.wbData = ctx.csrOldVal;    break  // CSR read value
    }
    cpu.setBus('write_data', ctx.wbData >>> 0)

    if (ctx.sig.regWrite && ctx.rd !== 0) {
      cpu.regWrite(ctx.rd, ctx.wbData)
    }
  }

  function phasePcUpdate() {
    if (!ctx) return

    const branchCond = BRANCH_CONDITION[ctx.funct3]
    const branchTaken = (() => {
      if (ctx!.sig.pcSel !== 1) return false
      if (ctx!.opcode === 0b1101111) return true  // JAL
      switch (branchCond) {
        case 'zero':   return ctx!.flags.zero
        case '!zero':  return !ctx!.flags.zero
        case 'neg':    return ctx!.flags.negative
        case '!neg':   return !ctx!.flags.negative
        case 'carry':  return ctx!.flags.carry
        case '!carry': return !ctx!.flags.carry
        default:       return false
      }
    })()

    const isJalr = ctx.sig.pcSel === 2

    // Re-activate imm chain buses for branch/JAL
    if (branchTaken) {
      cpu.setBus('instr', ctx.instr)
      cpu.setBus('immediate', ctx.immediate >>> 0)
    }

    const pcOffset = branchTaken ? ctx.immediate : 4
    cpu.setCtrl('ctrl_offset_sel', branchTaken)
    cpu.setBus('pc_offset', pcOffset >>> 0)

    const pcAdderResult = ((cpu.pc >>> 0) + pcOffset) >>> 0
    cpu.setBus('pc_adder_result', pcAdderResult)

    const jalrTarget = ctx.aluResult & 0xFFFFFFFE
    cpu.setBus('jalr_target', jalrTarget)

    // Re-activate alu_result for JALR trunk highlighting
    if (isJalr) {
      cpu.setBus('alu_result', ctx.aluResult)
    }

    cpu.setCtrl('ctrl_is_jalr', isJalr)
    const computedNextPc = isJalr ? jalrTarget : pcAdderResult

    cpu.setBus('ctrl_pc_sel', isJalr ? 2 : (branchTaken ? 1 : 0))
    cpu.setBus('next_pc', computedNextPc)
    cpu.nextPc = computedNextPc
    cpu.pc = computedNextPc

    // Done with this instruction
    ctx = null
  }

  const PHASE_FNS: Record<Phase, (() => void) | null> = {
    idle: null,
    fetch: phaseFetch,
    decode: phaseDecode,
    regread: phaseRegRead,
    execute: phaseExecute,
    memwb: phaseMemWb,
    pcupdate: phasePcUpdate,
  }

  // ── Controls ─────────────────────────────────────────────────────────────

  /** Advance one micro-phase */
  function microStep() {
    if (animating.value || cpu.halted) return

    phaseIndex.value++

    if (phaseIndex.value >= PHASE_ORDER.length) {
      phaseIndex.value = -1
      currentPhase.value = 'idle'
      cpu.clearBusVisuals()
      return
    }

    const phase = PHASE_ORDER[phaseIndex.value]
    if (!phase) return
    currentPhase.value = phase

    // Clear previous phase visuals (but not computed ctx values)
    if (phaseIndex.value > 0) {
      cpu.clearBusVisuals()
    }

    const fn = PHASE_FNS[phase]
    if (fn) fn()

    // Save state after each phase
    cpu.saveState({ phase: currentPhase.value, phaseIndex: phaseIndex.value })

    // Brief animation lock
    animating.value = true
    setTimeout(() => { animating.value = false }, animDuration.value)
  }

  /** Play all phases for one instruction with animation delay */
  async function animatedStep() {
    if (animating.value || cpu.halted) return
    animating.value = true

    // Reset if mid-instruction
    phaseIndex.value = -1

    for (let i = 0; i < PHASE_ORDER.length; i++) {
      const phase = PHASE_ORDER[i]
      if (!phase) continue
      phaseIndex.value = i
      currentPhase.value = phase

      if (i > 0) {
        cpu.clearBusVisuals()
      }

      const fn = PHASE_FNS[phase]
      if (fn) fn()
      await delay(animDuration.value)
    }

    phaseIndex.value = -1
    currentPhase.value = 'idle'
    cpu.clearBusVisuals()
    cpu.saveState({ phase: currentPhase.value, phaseIndex: phaseIndex.value })
    animating.value = false
  }

  /** Instant step — no animation, just run all phases immediately */
  function instantStep() {
    if (cpu.halted) return
    for (const phase of PHASE_ORDER) {
      const fn = PHASE_FNS[phase]
      if (fn) fn()
    }
    currentPhase.value = 'idle'
    phaseIndex.value = -1
    cpu.saveState({ phase: currentPhase.value, phaseIndex: phaseIndex.value })
  }

  /** Auto-run: animated steps continuously */
  function startAutoRun() {
    if (autoRunning.value) return
    autoRunning.value = true

    const runNext = async () => {
      if (!autoRunning.value || cpu.halted) { stopAutoRun(); return }
      await animatedStep()
      await delay(animDuration.value) // pause between instructions
      if (autoRunning.value) runNext()
    }
    runNext()
  }

  function stopAutoRun() {
    autoRunning.value = false
    if (autoTimer) { clearInterval(autoTimer); autoTimer = null }
  }

  function toggleAutoRun() {
    if (autoRunning.value) stopAutoRun()
    else startAutoRun()
  }

  function reset() {
    stopAutoRun()
    cpu.reset()
    currentPhase.value = 'idle'
    phaseIndex.value = -1
    animating.value = false
    ctx = null
  }

  return {
    currentPhase,
    phaseIndex,
    phaseLabel,
    animating,
    autoRunning,
    animDuration,
    microStep,
    animatedStep,
    instantStep,
    startAutoRun,
    stopAutoRun,
    toggleAutoRun,
    reset,
  }
})

function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms))
}
