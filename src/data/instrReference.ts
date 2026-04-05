// ─────────────────────────────────────────────────────────────────────────────
// RV32I Instruction Reference — grouped by operand pattern
// ─────────────────────────────────────────────────────────────────────────────

export interface InstrVariant {
  mnemonic: string
  description: string
}

export interface InstrGroup {
  label: string
  operands: string
  format: string
  variants: InstrVariant[]
  example: string
}

export const INSTR_GROUPS: InstrGroup[] = [
  // ── R-type: op RD, RS1, RS2 ──
  {
    label: 'ADD/SUB',
    operands: 'RD, RS1, RS2',
    format: 'R-type',
    variants: [
      { mnemonic: 'ADD', description: 'Add the value in register RS1 and the value in register RS2, store the result in register RD' },
      { mnemonic: 'SUB', description: 'Subtract the value in register RS2 from the value in register RS1, store the result in register RD' },
    ],
    example: 'ADD X3, X1, X2',
  },
  {
    label: 'AND/OR/XOR',
    operands: 'RD, RS1, RS2',
    format: 'R-type',
    variants: [
      { mnemonic: 'AND', description: 'Perform bitwise AND on register RS1 and register RS2, store the result in register RD. Each bit in the result is 1 only if both corresponding bits are 1' },
      { mnemonic: 'OR',  description: 'Perform bitwise OR on register RS1 and register RS2, store the result in register RD. Each bit in the result is 1 if either corresponding bit is 1' },
      { mnemonic: 'XOR', description: 'Perform bitwise XOR on register RS1 and register RS2, store the result in register RD. Each bit in the result is 1 if the corresponding bits differ' },
    ],
    example: 'AND X3, X1, X2',
  },
  {
    label: 'SLL/SRL/SRA',
    operands: 'RD, RS1, RS2',
    format: 'R-type',
    variants: [
      { mnemonic: 'SLL', description: 'Shift the value in register RS1 left by the number of positions specified in the lower 5 bits of register RS2. Vacated bits are filled with zeros. Store the result in RD' },
      { mnemonic: 'SRL', description: 'Shift the value in register RS1 right by the number of positions specified in the lower 5 bits of register RS2. Vacated bits are filled with zeros (logical shift). Store the result in RD' },
      { mnemonic: 'SRA', description: 'Shift the value in register RS1 right by the number of positions specified in the lower 5 bits of register RS2. Vacated bits are filled with copies of the sign bit (arithmetic shift). Store the result in RD' },
    ],
    example: 'SLL X3, X1, X2',
  },
  {
    label: 'SLT/SLTU',
    operands: 'RD, RS1, RS2',
    format: 'R-type',
    variants: [
      { mnemonic: 'SLT',  description: 'Compare register RS1 and register RS2 as signed integers. If RS1 is less than RS2, set register RD to 1; otherwise set it to 0' },
      { mnemonic: 'SLTU', description: 'Compare register RS1 and register RS2 as unsigned integers. If RS1 is less than RS2, set register RD to 1; otherwise set it to 0' },
    ],
    example: 'SLT X3, X1, X2',
  },

  // ── I-type ALU: op RD, RS1, IMM ──
  {
    label: 'ADDI',
    operands: 'RD, RS1, IMM',
    format: 'I-type',
    variants: [
      { mnemonic: 'ADDI', description: 'Add the value in register RS1 and the 12-bit signed immediate value IMM, store the result in register RD. IMM ranges from -2048 to 2047' },
    ],
    example: 'ADDI X3, X0, 7',
  },
  {
    label: 'ANDI/ORI/XORI',
    operands: 'RD, RS1, IMM',
    format: 'I-type',
    variants: [
      { mnemonic: 'ANDI', description: 'Perform bitwise AND on register RS1 and the 12-bit sign-extended immediate IMM, store the result in register RD' },
      { mnemonic: 'ORI',  description: 'Perform bitwise OR on register RS1 and the 12-bit sign-extended immediate IMM, store the result in register RD' },
      { mnemonic: 'XORI', description: 'Perform bitwise XOR on register RS1 and the 12-bit sign-extended immediate IMM, store the result in register RD. XORI RD, RS1, -1 performs bitwise NOT' },
    ],
    example: 'ANDI X3, X1, 0xFF',
  },
  {
    label: 'SLTI/SLTIU',
    operands: 'RD, RS1, IMM',
    format: 'I-type',
    variants: [
      { mnemonic: 'SLTI',  description: 'Compare register RS1 with the sign-extended 12-bit immediate as signed integers. If RS1 is less than IMM, set register RD to 1; otherwise set it to 0' },
      { mnemonic: 'SLTIU', description: 'Compare register RS1 with the sign-extended 12-bit immediate as unsigned integers. If RS1 is less than IMM, set register RD to 1; otherwise set it to 0. Note: SLTIU RD, RS1, 1 sets RD to 1 if RS1 equals zero' },
    ],
    example: 'SLTI X3, X1, 10',
  },
  {
    label: 'SLLI/SRLI/SRAI',
    operands: 'RD, RS1, SHAMT',
    format: 'I-type',
    variants: [
      { mnemonic: 'SLLI', description: 'Shift the value in register RS1 left by SHAMT bit positions (0-31). Vacated bits are filled with zeros. Store the result in register RD' },
      { mnemonic: 'SRLI', description: 'Shift the value in register RS1 right by SHAMT bit positions (0-31). Vacated bits are filled with zeros (logical shift). Store the result in register RD' },
      { mnemonic: 'SRAI', description: 'Shift the value in register RS1 right by SHAMT bit positions (0-31). Vacated bits are filled with copies of the sign bit (arithmetic shift). Store the result in register RD' },
    ],
    example: 'SLLI X3, X1, 4',
  },

  // ── Load: op RD, OFFSET(RS1) ──
  {
    label: 'LW/LH/LB/LHU/LBU',
    operands: 'RD, OFFSET(RS1)',
    format: 'I-type (Load)',
    variants: [
      { mnemonic: 'LW',  description: 'Load a 32-bit word from memory at address RS1 + OFFSET into register RD' },
      { mnemonic: 'LH',  description: 'Load a 16-bit halfword from memory at address RS1 + OFFSET, sign-extend it to 32 bits, and store in register RD' },
      { mnemonic: 'LB',  description: 'Load an 8-bit byte from memory at address RS1 + OFFSET, sign-extend it to 32 bits, and store in register RD' },
      { mnemonic: 'LHU', description: 'Load a 16-bit halfword from memory at address RS1 + OFFSET, zero-extend it to 32 bits, and store in register RD' },
      { mnemonic: 'LBU', description: 'Load an 8-bit byte from memory at address RS1 + OFFSET, zero-extend it to 32 bits, and store in register RD' },
    ],
    example: 'LW X3, 0(X5)',
  },

  // ── Store: op RS2, OFFSET(RS1) ──
  {
    label: 'SW/SH/SB',
    operands: 'RS2, OFFSET(RS1)',
    format: 'S-type',
    variants: [
      { mnemonic: 'SW', description: 'Store the 32-bit value from register RS2 into memory at address RS1 + OFFSET' },
      { mnemonic: 'SH', description: 'Store the lower 16 bits of register RS2 into memory at address RS1 + OFFSET' },
      { mnemonic: 'SB', description: 'Store the lower 8 bits of register RS2 into memory at address RS1 + OFFSET' },
    ],
    example: 'SW X4, 4(X5)',
  },

  // ── Branch: op RS1, RS2, LABEL ──
  {
    label: 'BEQ/BNE',
    operands: 'RS1, RS2, LABEL',
    format: 'B-type',
    variants: [
      { mnemonic: 'BEQ', description: 'Compare register RS1 and register RS2. If they are equal, jump to LABEL (PC-relative offset). Otherwise continue to the next instruction' },
      { mnemonic: 'BNE', description: 'Compare register RS1 and register RS2. If they are not equal, jump to LABEL (PC-relative offset). Otherwise continue to the next instruction' },
    ],
    example: 'BEQ X3, X0, DONE',
  },
  {
    label: 'BLT/BGE/BLTU/BGEU',
    operands: 'RS1, RS2, LABEL',
    format: 'B-type',
    variants: [
      { mnemonic: 'BLT',  description: 'Compare RS1 and RS2 as signed integers. If RS1 < RS2, jump to LABEL. Otherwise continue to the next instruction' },
      { mnemonic: 'BGE',  description: 'Compare RS1 and RS2 as signed integers. If RS1 >= RS2, jump to LABEL. Otherwise continue to the next instruction' },
      { mnemonic: 'BLTU', description: 'Compare RS1 and RS2 as unsigned integers. If RS1 < RS2, jump to LABEL. Otherwise continue to the next instruction' },
      { mnemonic: 'BGEU', description: 'Compare RS1 and RS2 as unsigned integers. If RS1 >= RS2, jump to LABEL. Otherwise continue to the next instruction' },
    ],
    example: 'BLT X3, X4, SKIP',
  },

  // ── U-type: op RD, IMM ──
  {
    label: 'LUI/AUIPC',
    operands: 'RD, IMM',
    format: 'U-type',
    variants: [
      { mnemonic: 'LUI',   description: 'Load the 20-bit immediate IMM into the upper 20 bits of register RD (bits 31:12), setting the lower 12 bits to zero. Used to build large constants together with ADDI' },
      { mnemonic: 'AUIPC', description: 'Add the 20-bit immediate IMM (shifted left by 12) to the current PC value and store the result in register RD. Used for PC-relative addressing of large offsets' },
    ],
    example: 'LUI X5, 0x10',
  },

  // ── Jump ──
  {
    label: 'JAL',
    operands: 'RD, LABEL',
    format: 'J-type',
    variants: [
      { mnemonic: 'JAL', description: 'Jump to LABEL (PC-relative offset up to ±1MB). Store the address of the next instruction (PC+4) in register RD as the return address. Use RD=X1 for function calls, RD=X0 for unconditional jumps' },
    ],
    example: 'JAL X1, FUNC',
  },
  {
    label: 'JALR',
    operands: 'RD, RS1, IMM',
    format: 'I-type',
    variants: [
      { mnemonic: 'JALR', description: 'Jump to the address computed as RS1 + IMM (with bit 0 cleared). Store PC+4 in register RD as the return address. Used for function returns (JALR X0, X1, 0) and indirect jumps' },
    ],
    example: 'JALR X0, X1, 0',
  },

  // ── Pseudo-instructions ──
  {
    label: 'NOP/RET',
    operands: '(none)',
    format: 'Pseudo',
    variants: [
      { mnemonic: 'NOP', description: 'No operation. The processor advances to the next instruction without doing anything. Encoded as ADDI X0, X0, 0' },
      { mnemonic: 'RET', description: 'Return from a function. Jumps to the address stored in the return address register X1. Encoded as JALR X0, X1, 0' },
    ],
    example: 'NOP',
  },
  {
    label: 'MV/LI',
    operands: 'RD, RS/IMM',
    format: 'Pseudo',
    variants: [
      { mnemonic: 'MV', description: 'Copy the value from register RS into register RD. Encoded as ADDI RD, RS, 0' },
      { mnemonic: 'LI', description: 'Load the immediate value IMM into register RD. For small values (-2048 to 2047), encoded as ADDI RD, X0, IMM' },
    ],
    example: 'LI X3, 42',
  },
  {
    label: 'J/CALL',
    operands: 'LABEL',
    format: 'Pseudo',
    variants: [
      { mnemonic: 'J',    description: 'Unconditional jump to LABEL. Does not save a return address. Encoded as JAL X0, LABEL' },
      { mnemonic: 'CALL', description: 'Call a function at LABEL. Saves the return address (PC+4) in register X1 (RA). Encoded as JAL X1, LABEL' },
    ],
    example: 'J LOOP',
  },
]
