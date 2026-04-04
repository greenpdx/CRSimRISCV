// ─────────────────────────────────────────────────────────────────────────────
// ALU operation table — single source of truth
//
// The ALU receives a 4-bit control code from the ALU control unit.
// The ALU control unit derives this from:
//   ALUOp[1:0]  (2 bits, from main decoder)
//   funct3[2:0] (3 bits, from instruction [14:12])
//   funct7[30]  (1 bit,  from instruction [30], distinguishes ADD/SUB, SRL/SRA)
// ─────────────────────────────────────────────────────────────────────────────

export interface AluOpDef {
  /** 4-bit control code sent to ALU */
  code:        number
  /** Short mnemonic shown in UI */
  mnemonic:    string
  /** Mathematical description */
  expression:  string
  /** RV32I instructions that use this op */
  usedBy:      string[]
  /** True if this op can produce a branch decision (zero/negative flag) */
  isBranchOp: boolean
}

export const ALU_OPS: AluOpDef[] = [
  {
    code: 0b0000, mnemonic: 'AND',  expression: 'A & B',
    usedBy: ['AND', 'ANDI'],
    isBranchOp: false,
  },
  {
    code: 0b0001, mnemonic: 'OR',   expression: 'A | B',
    usedBy: ['OR', 'ORI'],
    isBranchOp: false,
  },
  {
    code: 0b0010, mnemonic: 'ADD',  expression: 'A + B',
    usedBy: ['ADD', 'ADDI', 'LW', 'SW', 'JALR', 'AUIPC'],
    isBranchOp: false,
  },
  {
    code: 0b0011, mnemonic: 'XOR',  expression: 'A ^ B',
    usedBy: ['XOR', 'XORI'],
    isBranchOp: false,
  },
  {
    code: 0b0100, mnemonic: 'SLL',  expression: 'A << (B & 0x1f)',
    usedBy: ['SLL', 'SLLI'],
    isBranchOp: false,
  },
  {
    code: 0b0101, mnemonic: 'SRL',  expression: 'A >>> (B & 0x1f)',
    usedBy: ['SRL', 'SRLI'],
    isBranchOp: false,
  },
  {
    code: 0b0110, mnemonic: 'SUB',  expression: 'A - B',
    usedBy: ['SUB', 'BEQ', 'BNE', 'BLT', 'BGE', 'BLTU', 'BGEU'],
    isBranchOp: true,
  },
  {
    code: 0b0111, mnemonic: 'SLT',  expression: 'A < B (signed) ? 1 : 0',
    usedBy: ['SLT', 'SLTI'],
    isBranchOp: true,
  },
  {
    code: 0b1000, mnemonic: 'SLTU', expression: 'A <u B (unsigned) ? 1 : 0',
    usedBy: ['SLTU', 'SLTIU'],
    isBranchOp: true,
  },
  {
    code: 0b1001, mnemonic: 'SRA',  expression: 'A >> (B & 0x1f)',
    usedBy: ['SRA', 'SRAI'],
    isBranchOp: false,
  },
  {
    code: 0b1010, mnemonic: 'LUI',  expression: 'B',
    usedBy: ['LUI'],
    isBranchOp: false,
  },
]

/** Fast lookup: control code → AluOpDef */
export const ALU_OP_BY_CODE = new Map<number, AluOpDef>(
  ALU_OPS.map(op => [op.code, op])
)

// ─────────────────────────────────────────────────────────────────────────────
// ALU flags — outputs alongside the result
// ─────────────────────────────────────────────────────────────────────────────

export interface AluFlags {
  zero:     boolean   // result === 0
  negative: boolean   // result[31] === 1  (signed negative)
  carry:    boolean   // unsigned overflow on ADD/SUB
  overflow: boolean   // signed overflow on ADD/SUB
}

// ─────────────────────────────────────────────────────────────────────────────
// ALU compute function — used by both AluUnit.vue and the simulator core
// ─────────────────────────────────────────────────────────────────────────────

export function aluCompute(
  code: number,
  a:    number,
  b:    number,
): { result: number; flags: AluFlags } {
  const ua = a >>> 0   // unsigned 32-bit
  const ub = b >>> 0
  const sa = a | 0     // signed 32-bit
  const sb = b | 0

  let result = 0

  switch (code) {
    case 0b0000: result = ua & ub;                               break  // AND
    case 0b0001: result = ua | ub;                               break  // OR
    case 0b0010: result = (ua + ub) >>> 0;                       break  // ADD
    case 0b0011: result = ua ^ ub;                               break  // XOR
    case 0b0100: result = (ua << (ub & 0x1f)) >>> 0;             break  // SLL
    case 0b0101: result = ua >>> (ub & 0x1f);                    break  // SRL
    case 0b0110: result = (ua - ub) >>> 0;                       break  // SUB
    case 0b0111: result = sa < sb ? 1 : 0;                       break  // SLT
    case 0b1000: result = ua < ub ? 1 : 0;                       break  // SLTU
    case 0b1001: result = (sa >> (ub & 0x1f)) >>> 0;             break  // SRA
    case 0b1010: result = ub;                                    break  // LUI
    default:     result = 0;
  }

  const r32 = result >>> 0

  // Carry: unsigned overflow on ADD (0010) or borrow on SUB (0110)
  let carry = false
  if (code === 0b0010) carry = (ua + ub) > 0xffffffff
  if (code === 0b0110) carry = ua < ub

  // Overflow: signed overflow on ADD/SUB
  let overflow = false
  if (code === 0b0010) overflow = ((~(sa ^ sb) & (sa ^ (r32 | 0))) & 0x80000000) !== 0
  if (code === 0b0110) overflow = (( (sa ^ sb) & (sa ^ (r32 | 0))) & 0x80000000) !== 0

  return {
    result: r32,
    flags: {
      zero:     r32 === 0,
      negative: (r32 & 0x80000000) !== 0,
      carry,
      overflow,
    },
  }
}
