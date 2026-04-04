import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import type { Register, MemoryWord, BusState, ControlSignalState } from '../types/cpu'
import { REGISTER_DEFS }          from '../data/registerData'
import { OPCODE_BY_VALUE, BRANCH_CONDITION }  from '../data/controlData'
import { extractImmediate }        from '../data/immData'
import type { ImmSel }             from '../data/immData'
import { computeAluControl }       from '../data/aluControlData'
import { aluCompute }              from '../data/aluData'
import { RV32I_NETLIST, NET_BY_ID } from '../data/netlist'

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

const mkRegisters = (): Register[] =>
  REGISTER_DEFS.map(d => ({
    name:   d.name,
    xNum:   d.xNum,
    value:  d.resetVal,
    active: false,
    readA:  false,
    readB:  false,
  }))

const mkBusStates = (): Record<string, BusState> =>
  Object.fromEntries(
    RV32I_NETLIST.nets.map(n => [
      n.id,
      { netId: n.id, bits: n.bits, value: 0, state: 'idle' } satisfies BusState,
    ])
  )

const mkControlStates = (): Record<string, ControlSignalState> =>
  Object.fromEntries(
    RV32I_NETLIST.nets
      .filter(n => n.bits === 1)
      .map(n => [n.id, { netId: n.id, asserted: false } satisfies ControlSignalState])
  )

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

export const useCpuState = defineStore('cpu', () => {

  // ── Registers ──────────────────────────────────────────────────────────────
  const registers = ref<Register[]>(mkRegisters())

  // ── Program Counter ────────────────────────────────────────────────────────
  const pc     = ref<number>(0x00000000)
  const nextPc = ref<number>(0x00000004)

  // ── Memory ─────────────────────────────────────────────────────────────────
  const romWords = ref<MemoryWord[]>(
    Array.from({ length: 32 }, (_, i) => ({
      address: 0x00000000 + i * 4,
      value:   0x00000013,  // NOP (addi x0, x0, 0)
      asm:     'nop',
      active:  false,
    }))
  )
  const ramWords = ref<MemoryWord[]>(
    Array.from({ length: 32 }, (_, i) => ({
      address: 0x00010000 + i * 4,
      value:   0,
      active:  false,
    }))
  )

  // ── I/O ────────────────────────────────────────────────────────────────────
  const ioBaseAddress = ref<number>(0x40020000)
  const ioData        = ref<number>(0x00000000)
  const ioOp          = ref<'idle' | 'read' | 'write'>('idle')

  // ── Bus and control signal states ──────────────────────────────────────────
  const busStates     = ref<Record<string, BusState>>(mkBusStates())
  const controlStates = ref<Record<string, ControlSignalState>>(mkControlStates())

  // ── Last decoded instruction (for display) ─────────────────────────────────
  const lastInstr    = ref<number>(0x00000000)
  const lastOpEntry  = ref<ReturnType<typeof OPCODE_BY_VALUE.get>>(undefined)
  const halted       = ref<boolean>(false)
  const cycleCount   = ref<number>(0)

  // ── Getters ────────────────────────────────────────────────────────────────

  const currentInstruction = computed(() =>
    romWords.value.find(w => w.address === pc.value)?.value ?? 0
  )

  // ─────────────────────────────────────────────────────────────────────────────
  // Internal helpers
  // ─────────────────────────────────────────────────────────────────────────────

  function clearActiveFlags() {
    registers.value.forEach(r => { r.active = false; r.readA = false; r.readB = false })
    romWords.value.forEach(w => { w.active = false })
    ramWords.value.forEach(w => { w.active = false })
    ioOp.value = 'idle'
  }

  function clearBusStates() {
    for (const key in busStates.value)     busStates.value[key].state    = 'idle'
    for (const key in controlStates.value) controlStates.value[key].asserted = false
  }

  function setBus(netId: string, value: number) {
    const b = busStates.value[netId]
    if (b) { b.value = value; b.state = 'active' }
  }

  function setCtrl(netId: string, asserted: boolean) {
    const c = controlStates.value[netId]
    if (c) c.asserted = asserted
  }

  function romRead(address: number): number {
    const word = romWords.value.find(w => w.address === address)
    if (word) { word.active = true; return word.value }
    return 0
  }

  function ramRead(address: number): number {
    const word = ramWords.value.find(w => w.address === (address & ~3))
    if (word) { word.active = true; return word.value }
    return 0
  }

  function ramWrite(address: number, value: number) {
    const aligned = address & ~3
    const word = ramWords.value.find(w => w.address === aligned)
    if (word) { word.value = value; word.active = true }
    else ramWords.value.push({ address: aligned, value, active: true })
  }

  function isIoAddress(address: number): boolean {
    return (address >>> 0) >= (ioBaseAddress.value >>> 0)
  }

  function regRead(xNum: number): number {
    return registers.value[xNum]?.value ?? 0
  }

  function regWrite(xNum: number, value: number) {
    if (xNum === 0) return   // x0 hardwired to zero
    const r = registers.value[xNum]
    if (r) { r.value = value >>> 0; r.active = true }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Step — execute one instruction
  // ─────────────────────────────────────────────────────────────────────────────

  function step() {
    if (halted.value) return

    clearActiveFlags()
    clearBusStates()
    cycleCount.value++

    // ── 1. Fetch ─────────────────────────────────────────────────────────────
    const instr  = romRead(pc.value)
    lastInstr.value = instr
    setBus('pc_out',  pc.value)
    setBus('instr',   instr)

    // Mark active ROM word
    const romWord = romWords.value.find(w => w.address === pc.value)
    if (romWord) romWord.active = true

    // PC+4 — always needed for WB link address (JAL/JALR)
    const pcPlus4 = (pc.value >>> 0) + 4

    // ── 2. Decode ────────────────────────────────────────────────────────────
    const opcode   = instr & 0x7f
    const rd       = (instr >> 7)  & 0x1f
    const funct3   = (instr >> 12) & 0x07
    const rs1Num   = (instr >> 15) & 0x1f
    const rs2Num   = (instr >> 20) & 0x1f
    const funct7b30 = (instr >> 30) & 0x1

    const entry = OPCODE_BY_VALUE.get(opcode)
    lastOpEntry.value = entry

    if (!entry) {
      // Unknown opcode — treat as NOP, advance PC
      nextPc.value = pcPlus4
      pc.value = pcPlus4
      return
    }

    const sig = entry.signals
    const isLui = opcode === 0b0110111

    // Control signals → buses
    setCtrl('ctrl_reg_write',  sig.regWrite)
    setCtrl('ctrl_alu_src',    sig.aluSrc)
    setCtrl('ctrl_mem_read',   sig.memRead)
    setCtrl('ctrl_mem_write',  sig.memWrite)
    setBus('ctrl_alu_op',   sig.aluOp)
    setBus('ctrl_wb_sel',   sig.wbSel)
    setBus('ctrl_imm_sel',  sig.immSel)
    setBus('ctrl_pc_sel',   sig.pcSel)   // 2-bit

    // ── 3. Immediate generation ──────────────────────────────────────────────
    const immediate = extractImmediate(instr, sig.immSel as ImmSel)
    setBus('immediate', immediate >>> 0)

    // ── 4. Register read ─────────────────────────────────────────────────────
    const rs1Val = regRead(rs1Num)
    const rs2Val = regRead(rs2Num)

    if (rs1Num !== 0 || sig.aluSrc === false) {
      const r1 = registers.value[rs1Num]
      if (r1) r1.readA = true
    }
    if (!sig.aluSrc) {
      const r2 = registers.value[rs2Num]
      if (r2) r2.readB = true
    }

    setBus('rs1_data', rs1Val >>> 0)
    setBus('rs2_data', rs2Val >>> 0)

    // ── 5. ALU src mux ───────────────────────────────────────────────────────
    const aluB = sig.aluSrc ? immediate : rs2Val
    setBus('alu_b', aluB >>> 0)

    // ── 6. ALU control + compute ─────────────────────────────────────────────
    const aluCtrlCode = computeAluControl(
      sig.aluOp as 0 | 1 | 2,
      funct3,
      funct7b30 as 0 | 1,
      isLui,
    )
    setBus('alu_control', aluCtrlCode)

    const { result: aluResult, flags } = aluCompute(aluCtrlCode, rs1Val, aluB)
    setBus('alu_result', aluResult)
    setCtrl('zero_flag',  flags.zero)

    // ── 7. Memory access ─────────────────────────────────────────────────────
    let memReadData = 0

    if (sig.memRead) {
      const addr = aluResult
      setBus('ctrl_mem_read', 1)
      if (isIoAddress(addr)) {
        memReadData = ioData.value
        ioOp.value  = 'read'
      } else {
        memReadData = ramRead(addr)
      }
      setBus('mem_read_data', memReadData >>> 0)
    }

    if (sig.memWrite) {
      const addr = aluResult
      setBus('ctrl_mem_write', 1)
      if (isIoAddress(addr)) {
        ioData.value = rs2Val >>> 0
        ioOp.value   = 'write'
      } else {
        ramWrite(addr, rs2Val)
      }
    }

    // ── 8. Write-back mux ────────────────────────────────────────────────────
    let wbData = 0
    switch (sig.wbSel) {
      case 0: wbData = aluResult;   break   // ALU result
      case 1: wbData = memReadData; break   // Memory read
      case 2: wbData = pcPlus4;     break   // PC+4 (JAL/JALR)
    }
    setBus('write_data', wbData >>> 0)

    // ── 9. Register write ────────────────────────────────────────────────────
    if (sig.regWrite && rd !== 0) {
      regWrite(rd, wbData)
    }

    // ── 10. PC path: mux_pc_offset → pc_adder → mux_pc ─────────────────────

    // Evaluate branch condition from funct3
    const branchCond = BRANCH_CONDITION[funct3]
    const branchTaken = (() => {
      if (sig.pcSel !== 1) return false            // not a branch/JAL opcode
      if (opcode === 0b1101111) return true         // JAL — always taken
      // Conditional branch — evaluate flag against condition
      switch (branchCond) {
        case 'zero':   return flags.zero
        case '!zero':  return !flags.zero
        case 'neg':    return flags.negative
        case '!neg':   return !flags.negative
        case 'carry':  return flags.carry
        case '!carry': return !flags.carry
        default:       return false
      }
    })()

    const isJalr = sig.pcSel === 2

    // mux_pc_offset: select +4 (normal) or immediate (branch taken / JAL)
    const offsetSel = branchTaken ? 1 : 0
    const pcOffset  = branchTaken ? immediate : 4
    setCtrl('ctrl_offset_sel', branchTaken)
    setBus('pc_offset', pcOffset >>> 0)

    // pc_adder: PC + offset
    const pcAdderResult = ((pc.value >>> 0) + pcOffset) >>> 0
    setBus('pc_adder_result', pcAdderResult)

    // JALR target: ALU result (rs1 + imm) with bit0 cleared
    const jalrTarget = aluResult & 0xFFFFFFFE
    setBus('jalr_target', jalrTarget)

    // mux_pc: select adder result (0) or JALR target (1)
    setCtrl('ctrl_is_jalr', isJalr)
    const computedNextPc = isJalr ? jalrTarget : pcAdderResult

    setBus('ctrl_pc_sel', isJalr ? 2 : (branchTaken ? 1 : 0))
    setBus('next_pc', computedNextPc)
    nextPc.value = computedNextPc
    pc.value     = computedNextPc
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions
  // ─────────────────────────────────────────────────────────────────────────────

  function reset() {
    registers.value = mkRegisters()
    pc.value        = 0x00000000
    nextPc.value    = 0x00000004
    ioData.value    = 0x00000000
    ioOp.value      = 'idle'
    halted.value    = false
    cycleCount.value = 0
    busStates.value     = mkBusStates()
    controlStates.value = mkControlStates()
    clearActiveFlags()
    romWords.value.forEach(w => { w.active = false })
    ramWords.value.forEach(w => { w.value = 0; w.active = false })
  }

  /**
   * Load a program into ROM.
   * words: array of 32-bit instruction values, starting at baseAddress.
   */
  function loadProgram(words: number[], baseAddress = 0x00000000) {
    const padded = Array.from({ length: 32 }, (_, i) =>
      i < words.length ? words[i] : 0x00000013
    )
    romWords.value = padded.map((value, i) => ({
      address: baseAddress + i * 4,
      value,
      asm:    i < words.length ? undefined : 'nop',
      active: false,
    }))
    reset()
  }

  /**
   * Load with pre-decoded ASM labels.
   */
  function loadProgramWithAsm(
    entries: { value: number; asm: string }[],
    baseAddress = 0x00000000,
  ) {
    const padded = Array.from({ length: 32 }, (_, i) =>
      i < entries.length
        ? entries[i]
        : { value: 0x00000013, asm: 'nop' }
    )
    romWords.value = padded.map((e, i) => ({
      address: baseAddress + i * 4,
      value:   e.value,
      asm:     e.asm,
      active:  false,
    }))
    reset()
  }

  /**
   * Write to I/O from outside (e.g. testing or UI button).
   */
  function setIoData(value: number) {
    ioData.value = value >>> 0
  }

  return {
    // State
    registers,
    pc,
    nextPc,
    romWords,
    ramWords,
    ioBaseAddress,
    ioData,
    ioOp,
    busStates,
    controlStates,
    lastInstr,
    lastOpEntry,
    halted,
    cycleCount,

    // Getters
    currentInstruction,

    // Actions
    step,
    reset,
    loadProgram,
    loadProgramWithAsm,
    setIoData,
  }
})
