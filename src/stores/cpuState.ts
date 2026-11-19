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
  // Default program — exercises every non-trap RV32I operation visible in
  // the single-cycle datapath: R-type ALU, I-ALU, loads (B/H/W), stores
  // (B/W), all 6 conditional branches taken, JAL/JALR with a real stack
  // frame, CSR ops, and an I/O write.
  //
  // Stack convention: x2 (sp) starts at 0x10100 and grows down into RAM.
  // The function at 0xdc computes a0 = a0 + a1*2; the caller pushes ra and
  // pops it after the call.
  const DEFAULT_PROGRAM: { value: number; asm: string }[] = [
    // 0x00 — set up stack pointer at 0x10100 (top of RAM)
    { value: 0x00010137, asm: 'lui  x2, 0x10'      },
    { value: 0x10010113, asm: 'addi x2, x2, 0x100' },
    // 0x08 — R-type ALU sweep with t0=7, t1=5
    { value: 0x00700293, asm: 'addi x5, x0, 7'     },
    { value: 0x00500313, asm: 'addi x6, x0, 5'     },
    { value: 0x006283b3, asm: 'add  x7, x5, x6'    },
    { value: 0x40628433, asm: 'sub  x8, x5, x6'    },
    { value: 0x0062f4b3, asm: 'and  x9, x5, x6'    },
    { value: 0x0062e533, asm: 'or   x10, x5, x6'   },
    { value: 0x0062c5b3, asm: 'xor  x11, x5, x6'   },
    { value: 0x00629633, asm: 'sll  x12, x5, x6'   },
    { value: 0x0063d6b3, asm: 'srl  x13, x7, x6'   },
    { value: 0x4062d733, asm: 'sra  x14, x5, x6'   },
    { value: 0x0062a7b3, asm: 'slt  x15, x5, x6'   },
    { value: 0x00533833, asm: 'sltu x16, x6, x5'   },
    // 0x38 — I-ALU sweep
    { value: 0xfff00893, asm: 'addi x17, x0, -1'   },
    { value: 0x0ff8f913, asm: 'andi x18, x17, 0xff'},
    { value: 0x00a06993, asm: 'ori  x19, x0, 0xa'  },
    { value: 0x00429a13, asm: 'slli x20, x5, 4'    },
    { value: 0x0088da93, asm: 'srli x21, x17, 8'   },
    { value: 0x4088db13, asm: 'srai x22, x17, 8'   },
    // 0x50 — Memory sweep (RAM base 0x10000)
    { value: 0x00010bb7, asm: 'lui  x23, 0x10'     },
    { value: 0x05500c13, asm: 'addi x24, x0, 0x55' },
    { value: 0x018b8023, asm: 'sb   x24, 0(x23)'   },
    { value: 0x06600c93, asm: 'addi x25, x0, 0x66' },
    { value: 0x019b80a3, asm: 'sb   x25, 1(x23)'   },
    { value: 0x000b9d03, asm: 'lh   x26, 0(x23)'   },
    { value: 0x000bad83, asm: 'lw   x27, 0(x23)'   },
    { value: 0x01bba223, asm: 'sw   x27, 4(x23)'   },
    // 0x70 — Branch sweep (each conditional taken, skipping the nop after)
    { value: 0x00528463, asm: 'beq  x5, x5, 8'     },
    { value: 0x00000013, asm: 'nop'                },
    { value: 0x00629463, asm: 'bne  x5, x6, 8'     },
    { value: 0x00000013, asm: 'nop'                },
    { value: 0x00534463, asm: 'blt  x6, x5, 8'     },
    { value: 0x00000013, asm: 'nop'                },
    { value: 0x0062d463, asm: 'bge  x5, x6, 8'     },
    { value: 0x00000013, asm: 'nop'                },
    { value: 0x00536463, asm: 'bltu x6, x5, 8'     },
    { value: 0x00000013, asm: 'nop'                },
    { value: 0x0062f463, asm: 'bgeu x5, x6, 8'     },
    { value: 0x00000013, asm: 'nop'                },
    // 0xa0 — Function call with stack frame
    { value: 0xff010113, asm: 'addi x2, x2, -16'   }, // sp -= 16
    { value: 0x00112023, asm: 'sw   x1, 0(x2)'     }, // save ra
    { value: 0x00400513, asm: 'addi x10, x0, 4'    }, // a0 = 4
    { value: 0x00600593, asm: 'addi x11, x0, 6'    }, // a1 = 6
    { value: 0x02c000ef, asm: 'jal  x1, 44'        }, // call func at 0xdc
    { value: 0x00012083, asm: 'lw   x1, 0(x2)'     }, // restore ra
    { value: 0x01010113, asm: 'addi x2, x2, 16'    }, // sp += 16
    { value: 0x00050e93, asm: 'addi x29, x10, 0'   }, // t4 = result (= 16)
    // 0xc0 — CSR sweep on mscratch
    { value: 0x04200f13, asm: 'addi x30, x0, 0x42' },
    { value: 0x340f1073, asm: 'csrrw x0, mscratch, x30' },
    { value: 0x34002ff3, asm: 'csrrs x31, mscratch, x0' },
    // 0xcc — I/O write (LED block at 0x40020000)
    { value: 0x40020237, asm: 'lui  x4, 0x40020'   },
    { value: 0x0aa00193, asm: 'addi x3, x0, 0xaa'  },
    { value: 0x00322023, asm: 'sw   x3, 0(x4)'     },
    // 0xd8 — Spin
    { value: 0x0000006f, asm: 'jal  x0, 0'         },
    // 0xdc — Function: a0 = a0 + a1*2 ; ret
    { value: 0x00159593, asm: 'slli x11, x11, 1'   },
    { value: 0x00b50533, asm: 'add  x10, x10, x11' },
    { value: 0x00008067, asm: 'jalr x0, x1, 0'     },
  ]

  const ROM_WORDS = 64
  const RAM_WORDS = 64

  const romWords = ref<MemoryWord[]>(
    Array.from({ length: ROM_WORDS }, (_, i) => ({
      address: 0x00000000 + i * 4,
      value:   DEFAULT_PROGRAM[i]?.value ?? 0x00000013,
      asm:     DEFAULT_PROGRAM[i]?.asm ?? 'nop',
      active:  false,
    }))
  )
  const ramWords = ref<MemoryWord[]>(
    Array.from({ length: RAM_WORDS }, (_, i) => ({
      address: 0x00010000 + i * 4,
      value:   0,
      active:  false,
    }))
  )

  // ── I/O ────────────────────────────────────────────────────────────────────
  const ioBaseAddress = ref<number>(0x40020000)
  const ioData        = ref<number>(0x00000000)
  const ioOp          = ref<'idle' | 'read' | 'write'>('idle')

  // ── CSR Registers (Zicsr extension) ─────────────────────────────────────────
  // CSR addresses for the trap feature.
  const CSR_FFLAGS  = 0x001, CSR_FRM = 0x002, CSR_FCSR = 0x003
  const CSR_SSTATUS = 0x100, CSR_SIE = 0x104, CSR_STVEC = 0x105
  const CSR_SSCRATCH= 0x140, CSR_SEPC = 0x141, CSR_SCAUSE = 0x142
  const CSR_STVAL   = 0x143, CSR_SIP = 0x144
  const CSR_MSTATUS = 0x300, CSR_MEDELEG = 0x302, CSR_MIDELEG = 0x303
  const CSR_MIE     = 0x304, CSR_MTVEC = 0x305
  const CSR_MSCRATCH= 0x340, CSR_MEPC = 0x341, CSR_MCAUSE = 0x342
  const CSR_MTVAL   = 0x343, CSR_MIP = 0x344

  const csrRegisters = ref<Record<number, number>>({
    0xC00: 0,  // cycle
    0xC01: 0,  // time (alias for cycle)
    0xC02: 0,  // instret
    [CSR_MSTATUS]: 0, [CSR_MEDELEG]: 0, [CSR_MIDELEG]: 0,
    [CSR_MIE]: 0, [CSR_MTVEC]: 0,
    [CSR_MSCRATCH]: 0, [CSR_MEPC]: 0, [CSR_MCAUSE]: 0,
    [CSR_MTVAL]: 0, [CSR_MIP]: 0,
    [CSR_SSTATUS]: 0, [CSR_SIE]: 0, [CSR_STVEC]: 0,
    [CSR_SSCRATCH]: 0, [CSR_SEPC]: 0, [CSR_SCAUSE]: 0,
    [CSR_STVAL]: 0, [CSR_SIP]: 0,
    0xF11: 0,  // mvendorid (read-only)
    0xF13: 0,  // mimpid (read-only)
  })

  const CSR_READ_ONLY = new Set([0xC00, 0xC01, 0xC02, 0xF11, 0xF13])

  // sstatus/sie/sip are *views* into the M-mode counterparts.
  const SSTATUS_VIEW_MASK = (1 << 1) | (1 << 5) | (1 << 8)            // SIE | SPIE | SPP
  const SIRQ_VIEW_MASK    = (1 << 1) | (1 << 5) | (1 << 9)            // SSI | STI | SEI

  function csrRead(addr: number): number {
    if (addr === 0xC01) return csrRegisters.value[0xC00] ?? 0 // time = cycle
    if (addr === CSR_SSTATUS) return (csrRegisters.value[CSR_MSTATUS] ?? 0) & SSTATUS_VIEW_MASK
    if (addr === CSR_SIE)     return (csrRegisters.value[CSR_MIE]     ?? 0) & SIRQ_VIEW_MASK
    if (addr === CSR_SIP)     return (csrRegisters.value[CSR_MIP]     ?? 0) & SIRQ_VIEW_MASK
    return csrRegisters.value[addr] ?? 0
  }

  function csrWrite(addr: number, value: number) {
    if (CSR_READ_ONLY.has(addr)) return
    if (addr === CSR_SSTATUS) {
      const old = csrRegisters.value[CSR_MSTATUS] ?? 0
      csrRegisters.value[CSR_MSTATUS] = ((old & ~SSTATUS_VIEW_MASK) | (value & SSTATUS_VIEW_MASK)) >>> 0
      return
    }
    if (addr === CSR_SIE) {
      const old = csrRegisters.value[CSR_MIE] ?? 0
      csrRegisters.value[CSR_MIE] = ((old & ~SIRQ_VIEW_MASK) | (value & SIRQ_VIEW_MASK)) >>> 0
      return
    }
    if (addr === CSR_SIP) {
      const old = csrRegisters.value[CSR_MIP] ?? 0
      csrRegisters.value[CSR_MIP] = ((old & ~SIRQ_VIEW_MASK) | (value & SIRQ_VIEW_MASK)) >>> 0
      return
    }
    csrRegisters.value[addr] = value >>> 0
  }

  /** Execute a CSR operation: returns old value, writes new value */
  function csrOp(addr: number, rs1Val: number, op: number): number {
    const old = csrRead(addr)
    switch (op) {
      case 1: csrWrite(addr, rs1Val); break              // CSRRW: write rs1
      case 2: csrWrite(addr, old | rs1Val); break         // CSRRS: set bits
      case 3: csrWrite(addr, old & ~rs1Val); break        // CSRRC: clear bits
    }
    return old
  }

  function csrIncrementCycle() {
    csrRegisters.value[0xC00] = ((csrRegisters.value[0xC00] ?? 0) + 1) >>> 0
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Trap subsystem (feature-flagged via `trapsEnabled`)
  // Mirrors the Rust simulator's `sim/src/sim/trap.rs`. Off by default; the
  // existing CpuLayout default program does not need it. Toggle via the
  // CpuLayout control bar to exercise privilege modes, IRQs, and ECALL.
  // ─────────────────────────────────────────────────────────────────────────
  type PrivMode = 'M' | 'S' | 'U'

  const trapsEnabled = ref<boolean>(false)
  const privMode    = ref<PrivMode>('M')
  const pendingIrq  = ref<number>(0)
  const trapDepth   = ref<number>(0)
  const lastTrap    = ref<{
    cycle: number
    fromMode: PrivMode
    toMode:   PrivMode
    cause: number
    isInterrupt: boolean
    epc: number
    tval: number
  } | null>(null)

  // mstatus bit masks
  const MS_SIE  = 1 << 1
  const MS_MIE  = 1 << 3
  const MS_SPIE = 1 << 5
  const MS_MPIE = 1 << 7
  const MS_SPP  = 1 << 8
  const MS_MPP_SHIFT = 11
  const MS_MPP_MASK  = 0x3 << MS_MPP_SHIFT

  // mip/mie bit positions
  const IRQ_SSI = 1 << 1
  const IRQ_MSI = 1 << 3
  const IRQ_STI = 1 << 5
  const IRQ_MTI = 1 << 7
  const IRQ_SEI = 1 << 9
  const IRQ_MEI = 1 << 11

  // Exception causes
  const EXC_ILLEGAL    = 2
  const EXC_BREAKPOINT = 3
  const EXC_ECALL_U    = 8
  const EXC_ECALL_S    = 9
  const EXC_ECALL_M    = 11

  function modeBits(m: PrivMode): number {
    return m === 'M' ? 3 : m === 'S' ? 1 : 0
  }
  function modeFromBits(b: number): PrivMode {
    return (b & 3) === 3 ? 'M' : (b & 3) === 1 ? 'S' : 'U'
  }

  /** Take a trap (interrupt or exception). Updates mstatus, m/sepc, m/scause,
   *  m/stval, and redirects PC to the appropriate trap vector. */
  function takeTrap(cause: number, isInterrupt: boolean, epc: number, tval: number) {
    const bit = 1 << (cause & 31)
    const medeleg = csrRead(CSR_MEDELEG)
    const mideleg = csrRead(CSR_MIDELEG)
    const delegate = privMode.value !== 'M' &&
      ((isInterrupt ? mideleg : medeleg) & bit) !== 0
    const dest: PrivMode = delegate ? 'S' : 'M'
    const causeVal = isInterrupt ? (0x80000000 | cause) : cause
    const mstatus = csrRead(CSR_MSTATUS)

    if (dest === 'M') {
      const oldMie = (mstatus & MS_MIE) !== 0
      const newMs = (mstatus & ~(MS_MIE | MS_MPIE | MS_MPP_MASK)) >>> 0
        | (modeBits(privMode.value) << MS_MPP_SHIFT)
        | (oldMie ? MS_MPIE : 0)
      csrWrite(CSR_MSTATUS, newMs)
      csrWrite(CSR_MEPC,    epc)
      csrWrite(CSR_MCAUSE,  causeVal)
      csrWrite(CSR_MTVAL,   tval)
      const mtvec = csrRead(CSR_MTVEC)
      const base = mtvec & ~3
      const vectored = (mtvec & 3) === 1 && isInterrupt
      pc.value = (vectored ? base + cause * 4 : base) >>> 0
    } else {
      const oldSie = (mstatus & MS_SIE) !== 0
      const sppBit = privMode.value === 'S' ? MS_SPP : 0
      const newMs = (mstatus & ~(MS_SIE | MS_SPIE | MS_SPP)) >>> 0
        | sppBit | (oldSie ? MS_SPIE : 0)
      csrWrite(CSR_MSTATUS, newMs)
      csrWrite(CSR_SEPC,    epc)
      csrWrite(CSR_SCAUSE,  causeVal)
      csrWrite(CSR_STVAL,   tval)
      pc.value = (csrRead(CSR_STVEC) & ~3) >>> 0
    }
    nextPc.value = pc.value
    lastTrap.value = {
      cycle: cycleCount.value,
      fromMode: privMode.value,
      toMode: dest,
      cause, isInterrupt, epc, tval,
    }
    privMode.value = dest
    trapDepth.value++
  }

  /** MRET: restore PC from mepc, mode from mstatus.MPP, MIE from MPIE. */
  function mretInstr() {
    const mstatus = csrRead(CSR_MSTATUS)
    const mpp  = (mstatus & MS_MPP_MASK) >> MS_MPP_SHIFT
    const mpie = (mstatus & MS_MPIE) !== 0
    const newMs = (mstatus & ~(MS_MIE | MS_MPIE | MS_MPP_MASK)) >>> 0
      | (mpie ? MS_MIE : 0) | MS_MPIE
    csrWrite(CSR_MSTATUS, newMs)
    pc.value     = csrRead(CSR_MEPC) >>> 0
    nextPc.value = pc.value
    privMode.value = modeFromBits(mpp)
    if (trapDepth.value > 0) trapDepth.value--
  }

  /** SRET: restore PC from sepc, mode from SPP, SIE from SPIE. */
  function sretInstr() {
    const mstatus = csrRead(CSR_MSTATUS)
    const spp  = (mstatus & MS_SPP) !== 0
    const spie = (mstatus & MS_SPIE) !== 0
    const newMs = (mstatus & ~(MS_SIE | MS_SPIE | MS_SPP)) >>> 0
      | (spie ? MS_SIE : 0) | MS_SPIE
    csrWrite(CSR_MSTATUS, newMs)
    pc.value     = csrRead(CSR_SEPC) >>> 0
    nextPc.value = pc.value
    privMode.value = spp ? 'S' : 'U'
    if (trapDepth.value > 0) trapDepth.value--
  }

  /** Sample mip & mie at the start of a cycle. Returns true if a trap fired. */
  function irqCheck(): boolean {
    if (!trapsEnabled.value) return false
    const merged = (csrRead(CSR_MIP) | pendingIrq.value) >>> 0
    csrWrite(CSR_MIP, merged)
    const mie = csrRead(CSR_MIE)
    const mstatus = csrRead(CSR_MSTATUS)
    const mideleg = csrRead(CSR_MIDELEG)

    // Priority order per RISC-V spec.
    const order: ReadonlyArray<readonly [number, number]> = [
      [IRQ_MEI, 11], [IRQ_MSI, 3], [IRQ_MTI, 7],
      [IRQ_SEI, 9],  [IRQ_SSI, 1], [IRQ_STI, 5],
    ]
    for (const [mask, cause] of order) {
      if ((merged & mie & mask) === 0) continue
      const delegated = (mideleg & mask) !== 0
      const dest: PrivMode = delegated ? 'S' : 'M'
      let enabled = false
      if (dest === 'M') {
        enabled = privMode.value === 'M' ? (mstatus & MS_MIE) !== 0 : true
      } else {
        if (privMode.value === 'S') enabled = (mstatus & MS_SIE) !== 0
        else if (privMode.value === 'U') enabled = true
      }
      if (enabled) {
        takeTrap(cause, true, pc.value, 0)
        return true
      }
    }
    return false
  }

  /** External IRQ injection — flips a bit in pending_irq that IrqCheck samples. */
  function raiseIrq(kind: 'mti' | 'mei' | 'msi' | 'sti' | 'sei' | 'ssi') {
    const m: Record<string, number> = {
      mti: IRQ_MTI, mei: IRQ_MEI, msi: IRQ_MSI,
      sti: IRQ_STI, sei: IRQ_SEI, ssi: IRQ_SSI,
    }
    pendingIrq.value = (pendingIrq.value | (m[kind] ?? 0)) >>> 0
  }
  function clearIrqs() {
    pendingIrq.value = 0
    csrWrite(CSR_MIP, 0)
  }

  function csrIncrementInstret() {
    csrRegisters.value[0xC02] = ((csrRegisters.value[0xC02] ?? 0) + 1) >>> 0
  }

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
    for (const s of Object.values(busStates.value))     s.state    = 'idle'
    for (const s of Object.values(controlStates.value)) s.asserted = false
  }

  /** Clear only bus visual states (idle), preserve control signal values */
  function clearBusVisuals() {
    for (const s of Object.values(busStates.value))     s.state = 'idle'
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

    // ── 0. IRQ check (only when trap feature is enabled) ────────────────
    if (trapsEnabled.value && irqCheck()) {
      // A trap fired; PC is now at the trap vector. The handler runs next cycle.
      return
    }

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

    // ── SYSTEM (0x73) short-circuit: CSR ops and privileged instructions ──
    if (opcode === 0b1110011) {
      const r1 = registers.value[rs1Num]; if (r1) r1.readA = true
      const rs1Val = regRead(rs1Num)

      if (funct3 === 0) {
        // ECALL / EBREAK / MRET / SRET
        const imm12  = (instr >> 20) & 0xfff
        const funct7 = (instr >> 25) & 0x7f
        const rs2v   = (instr >> 20) & 0x1f
        // MRET (imm 0x302)
        if (funct7 === 0b0011000 && rs2v === 0b00010) {
          if (trapsEnabled.value) { mretInstr(); return }
          // Without traps, MRET just falls through to PC+4 below.
        }
        // SRET (imm 0x102)
        else if (funct7 === 0b0001000 && rs2v === 0b00010) {
          if (trapsEnabled.value) { sretInstr(); return }
        }
        // ECALL
        else if (imm12 === 0x000) {
          if (trapsEnabled.value) {
            const cause = privMode.value === 'M' ? EXC_ECALL_M
                        : privMode.value === 'S' ? EXC_ECALL_S : EXC_ECALL_U
            takeTrap(cause, false, pc.value, 0)
            return
          }
        }
        // EBREAK
        else if (imm12 === 0x001) {
          if (trapsEnabled.value) {
            takeTrap(EXC_BREAKPOINT, false, pc.value, 0)
            return
          }
        }
        // No-op fallthrough — just advance PC.
        nextPc.value = pcPlus4
        pc.value = pcPlus4
        return
      }

      // CSR read-modify-write (funct3 1..7)
      const csrAddr = (instr >> 20) & 0xfff
      const isImm   = (funct3 & 0b100) !== 0
      const src     = isImm ? (rs1Num & 0x1f) : rs1Val
      const oldVal  = csrRead(csrAddr)
      let newVal: number = oldVal
      switch (funct3 & 0b011) {
        case 0b01: newVal = src; break                       // CSRRW(I)
        case 0b10: newVal = (oldVal | src) >>> 0; break      // CSRRS(I)
        case 0b11: newVal = (oldVal & ~src) >>> 0; break     // CSRRC(I)
      }
      // CSRRS/CSRRC with rs1=x0 (or uimm=0) must not write.
      const doWrite = (funct3 & 0b011) === 0b01 ? true
                    : (isImm ? (rs1Num !== 0) : (rs1Num !== 0))
      if (doWrite) csrWrite(csrAddr, newVal)
      if (rd !== 0) regWrite(rd, oldVal)
      setBus('write_data', oldVal >>> 0)
      nextPc.value = pcPlus4
      pc.value = pcPlus4
      return
    }

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

    // ── 6. ALU control + compute ─────────────────────��───────────────────────
    // For I-type ALU, bit 30 is part of immediate, not funct7. Only R-type
    // and I-type shifts (SLLI/SRLI/SRAI) use funct7b30.
    const isRtype = opcode === 0b0110011
    const isShiftImm = opcode === 0b0010011 && (funct3 === 0b101 || funct3 === 0b001)
    const effectiveFunct7b30 = (isRtype || isShiftImm) ? (funct7b30 as 0 | 1) : 0

    const aluCtrlCode = computeAluControl(
      sig.aluOp as 0 | 1 | 2,
      funct3,
      effectiveFunct7b30,
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
      case 3: wbData = 0;           break   // CSR (handled in phased execution)
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
    // Trap state
    privMode.value   = 'M'
    pendingIrq.value = 0
    trapDepth.value  = 0
    lastTrap.value   = null
    // Clear all CSRs back to 0 (preserving the schema).
    for (const k of Object.keys(csrRegisters.value)) {
      csrRegisters.value[Number(k)] = 0
    }
  }

  /**
   * Load a program into ROM.
   * words: array of 32-bit instruction values, starting at baseAddress.
   */
  function loadProgram(words: number[], baseAddress = 0x00000000) {
    const padded = Array.from({ length: ROM_WORDS }, (_, i) =>
      words[i] ?? 0x00000013
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
    const padded = Array.from({ length: ROM_WORDS }, (_, i) =>
      entries[i] ?? { value: 0x00000013, asm: 'nop' }
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

  // ─────────────────────────────────────────────────────────────────────────────
  // State persistence (localStorage)
  // ─────────────────────────────────────────────────────────────────────────────

  const STORAGE_KEY = 'cpuvis_state'

  function saveState(phaseData?: { phase: string; phaseIndex: number }) {
    try {
      const state = {
        registers: registers.value.map(r => ({ name: r.name, xNum: r.xNum, value: r.value })),
        pc: pc.value,
        nextPc: nextPc.value,
        romWords: romWords.value.map(w => ({ address: w.address, value: w.value, asm: w.asm })),
        ramWords: ramWords.value.map(w => ({ address: w.address, value: w.value })),
        ioBaseAddress: ioBaseAddress.value,
        ioData: ioData.value,
        cycleCount: cycleCount.value,
        halted: halted.value,
        lastInstr: lastInstr.value,
        csrRegisters: { ...csrRegisters.value },
        phase: phaseData?.phase ?? 'idle',
        phaseIndex: phaseData?.phaseIndex ?? -1,
      }
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
    } catch { /* ignore storage errors */ }
  }

  function restoreState(): { restored: boolean; phase?: string; phaseIndex?: number } {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (!raw) return { restored: false }
      const state = JSON.parse(raw)

      if (state.registers) {
        state.registers.forEach((r: any, i: number) => {
          if (registers.value[i]) {
            registers.value[i].value = r.value ?? 0
          }
        })
      }
      if (state.pc !== undefined) pc.value = state.pc
      if (state.nextPc !== undefined) nextPc.value = state.nextPc
      if (state.romWords) {
        romWords.value = state.romWords.map((w: any) => ({
          address: w.address, value: w.value, asm: w.asm ?? 'nop', active: false,
        }))
      }
      if (state.ramWords) {
        ramWords.value = state.ramWords.map((w: any) => ({
          address: w.address, value: w.value, active: false,
        }))
      }
      if (state.ioBaseAddress !== undefined) ioBaseAddress.value = state.ioBaseAddress
      if (state.ioData !== undefined) ioData.value = state.ioData
      if (state.cycleCount !== undefined) cycleCount.value = state.cycleCount
      if (state.halted !== undefined) halted.value = state.halted
      if (state.lastInstr !== undefined) lastInstr.value = state.lastInstr
      if (state.csrRegisters) {
        for (const [k, v] of Object.entries(state.csrRegisters)) {
          csrRegisters.value[Number(k)] = v as number
        }
      }

      return {
        restored: true,
        phase: state.phase,
        phaseIndex: state.phaseIndex,
      }
    } catch {
      return { restored: false }
    }
  }

  function clearSavedState() {
    localStorage.removeItem(STORAGE_KEY)
  }

  // Restore on first load
  restoreState()

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
    clearActiveFlags,
    clearBusStates,
    clearBusVisuals,
    setBus,
    setCtrl,
    romRead,
    regRead,
    regWrite,
    ramRead,
    ramWrite,
    isIoAddress,
    csrRegisters,
    csrRead,
    csrWrite,
    csrOp,
    csrIncrementCycle,
    csrIncrementInstret,
    // Trap feature
    trapsEnabled,
    privMode,
    pendingIrq,
    lastTrap,
    trapDepth,
    raiseIrq,
    clearIrqs,
    saveState,
    restoreState,
    clearSavedState,
  }
})
