// ─────────────────────────────────────────────────────────────────────────────
// RV32I Decoder / Control Unit — signal table
//
// Input:  opcode[6:0]  (instruction bits [6:0])
// Output: 9 control signals
//
// ALUOp encoding:
//   00 = force ADD  (load, store, AUIPC, JALR, JAL)
//   01 = force SUB  (branch comparison)
//   10 = use funct3/funct7 (R-type and I-ALU)
//
// WBSel encoding:
//   00 = ALU result
//   01 = Memory read data
//   10 = PC + 4  (JAL / JALR link address)
//
// PCSel encoding (replaces separate branch + jump bits):
//   00 = PC + 4              (sequential — default)
//   01 = PC + imm            (branches if taken, JAL — BranchAdder output)
//   10 = (rs1 + imm) & ~1   (JALR — ALU result with bit0 cleared)
//
//   NOTE: For branch instructions (opcode 1100011) pcSel=01 is conditional —
//   the step() function gates it with the ALU flag for the specific funct3.
//   For JAL pcSel=01 is unconditional. For JALR pcSel=10 is unconditional.
//
// ImmSel encoding:
//   000 = I-type   imm[11:0]
//   001 = S-type   imm[11:5|4:0]
//   010 = B-type   imm[12|10:5|4:1|11]
//   011 = U-type   imm[31:12]
//   100 = J-type   imm[20|10:1|11|19:12]
//   101 = none     (R-type — no immediate)
// ─────────────────────────────────────────────────────────────────────────────

export interface ControlSignals {
  regWrite:  boolean
  aluSrc:    boolean        // false=rs2, true=immediate
  aluOp:     0 | 1 | 2     // 2-bit
  memRead:   boolean
  memWrite:  boolean
  wbSel:     0 | 1 | 2     // 2-bit: 0=ALU, 1=Mem, 2=PC+4
  pcSel:     0 | 1 | 2     // 2-bit: 0=PC+4, 1=PC+imm, 2=(rs1+imm)&~1
  immSel:    0|1|2|3|4|5   // 3-bit immediate format selector
}

export type ImmFormat = 'I' | 'S' | 'B' | 'U' | 'J' | 'none'

export interface OpcodeEntry {
  opcode:    number
  opcodeBin: string
  name:      string
  instrs:    string[]
  signals:   ControlSignals
  immFormat: ImmFormat
}

export const OPCODE_TABLE: OpcodeEntry[] = [
  {
    opcode: 0b0110011, opcodeBin: '0110011', name: 'R-type',
    instrs: ['ADD','SUB','AND','OR','XOR','SLL','SRL','SRA','SLT','SLTU'],
    immFormat: 'none',
    signals: {
      regWrite: true,  aluSrc: false, aluOp: 2,
      memRead:  false, memWrite: false,
      wbSel: 0, pcSel: 0, immSel: 5,
    },
  },
  {
    opcode: 0b0010011, opcodeBin: '0010011', name: 'I-ALU',
    instrs: ['ADDI','ANDI','ORI','XORI','SLLI','SRLI','SRAI','SLTI','SLTIU'],
    immFormat: 'I',
    signals: {
      regWrite: true,  aluSrc: true,  aluOp: 2,
      memRead:  false, memWrite: false,
      wbSel: 0, pcSel: 0, immSel: 0,
    },
  },
  {
    opcode: 0b0000011, opcodeBin: '0000011', name: 'Load',
    instrs: ['LW','LH','LB','LHU','LBU'],
    immFormat: 'I',
    signals: {
      regWrite: true,  aluSrc: true,  aluOp: 0,
      memRead:  true,  memWrite: false,
      wbSel: 1, pcSel: 0, immSel: 0,
    },
  },
  {
    opcode: 0b0100011, opcodeBin: '0100011', name: 'Store',
    instrs: ['SW','SH','SB'],
    immFormat: 'S',
    signals: {
      regWrite: false, aluSrc: true,  aluOp: 0,
      memRead:  false, memWrite: true,
      wbSel: 0, pcSel: 0, immSel: 1,
    },
  },
  {
    // pcSel=1 but CONDITIONAL — step() gates with flag from funct3
    opcode: 0b1100011, opcodeBin: '1100011', name: 'Branch',
    instrs: ['BEQ','BNE','BLT','BGE','BLTU','BGEU'],
    immFormat: 'B',
    signals: {
      regWrite: false, aluSrc: false, aluOp: 1,
      memRead:  false, memWrite: false,
      wbSel: 0, pcSel: 1, immSel: 2,
    },
  },
  {
    opcode: 0b0110111, opcodeBin: '0110111', name: 'LUI',
    instrs: ['LUI'],
    immFormat: 'U',
    signals: {
      regWrite: true,  aluSrc: true,  aluOp: 2,
      memRead:  false, memWrite: false,
      wbSel: 0, pcSel: 0, immSel: 3,
    },
  },
  {
    opcode: 0b0010111, opcodeBin: '0010111', name: 'AUIPC',
    instrs: ['AUIPC'],
    immFormat: 'U',
    signals: {
      regWrite: true,  aluSrc: true,  aluOp: 0,
      memRead:  false, memWrite: false,
      wbSel: 0, pcSel: 0, immSel: 3,
    },
  },
  {
    // JAL: pcSel=1 unconditional, PC + J-imm
    opcode: 0b1101111, opcodeBin: '1101111', name: 'JAL',
    instrs: ['JAL'],
    immFormat: 'J',
    signals: {
      regWrite: true,  aluSrc: true,  aluOp: 0,
      memRead:  false, memWrite: false,
      wbSel: 2, pcSel: 1, immSel: 4,
    },
  },
  {
    // JALR: pcSel=2 unconditional, (rs1 + I-imm) & ~1 from ALU result
    opcode: 0b1100111, opcodeBin: '1100111', name: 'JALR',
    instrs: ['JALR'],
    immFormat: 'I',
    signals: {
      regWrite: true,  aluSrc: true,  aluOp: 0,
      memRead:  false, memWrite: false,
      wbSel: 2, pcSel: 2, immSel: 0,
    },
  },
]

export const OPCODE_BY_VALUE = new Map<number, OpcodeEntry>(
  OPCODE_TABLE.map(e => [e.opcode, e])
)

export const IMM_FORMAT_LABEL: Record<ImmFormat, string> = {
  I:    'I-type  imm[11:0]',
  S:    'S-type  imm[11:5|4:0]',
  B:    'B-type  imm[12|10:5|4:1|11]',
  U:    'U-type  imm[31:12]',
  J:    'J-type  imm[20|10:1|11|19:12]',
  none: 'R-type  no immediate',
}

export const WB_SEL_LABEL: Record<0|1|2, string> = {
  0: 'ALU result',
  1: 'Mem read',
  2: 'PC + 4',
}

export const ALU_OP_LABEL: Record<0|1|2, string> = {
  0: 'force ADD',
  1: 'force SUB',
  2: 'use funct',
}

// NEW — label for the 2-bit pcSel signal
export const PC_SEL_LABEL: Record<0|1|2, string> = {
  0: 'PC + 4',
  1: 'PC + imm  (branch/JAL)',
  2: '(rs1+imm)&~1  (JALR)',
}

// Branch condition per funct3 — which ALU flag to test
export type BranchCondition = 'zero' | '!zero' | 'neg' | '!neg' | 'carry' | '!carry'

export const BRANCH_CONDITION: Record<number, BranchCondition> = {
  0b000: 'zero',   // BEQ  — equal        → zero=1
  0b001: '!zero',  // BNE  — not equal    → zero=0
  0b100: 'neg',    // BLT  — less than    → negative=1
  0b101: '!neg',   // BGE  — >=           → negative=0
  0b110: 'carry',  // BLTU — unsigned <   → carry=1
  0b111: '!carry', // BGEU — unsigned >=  → carry=0
}
