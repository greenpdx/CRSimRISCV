// ─────────────────────────────────────────────────────────────────────────────
// ALU Control Unit — truth table
//
// Sits between the main decoder and the ALU.
//
// Inputs:
//   aluOp[1:0]   from main decoder  (2 bits)
//   funct3[2:0]  from instruction[14:12]  (3 bits)
//   funct7b30    instruction[30]  (1 bit — distinguishes ADD/SUB, SRL/SRA)
//
// Output:
//   aluControl[3:0]  — 4-bit code sent directly to ALU
//
// When aluOp = 00 → always output ADD (0010), ignore funct fields
// When aluOp = 01 → always output SUB (0110), ignore funct fields
// When aluOp = 10 → decode funct3 + funct7[30]
// ─────────────────────────────────────────────────────────────────────────────

export interface AluControlEntry {
  aluOp:      0 | 1 | 2
  funct3:     number | null   // null = don't care
  funct7b30:  0 | 1 | null    // null = don't care
  aluControl: number          // 4-bit output
  mnemonic:   string
  note:       string
}

export const ALU_CONTROL_TABLE: AluControlEntry[] = [
  // ── aluOp 00: force ADD (load / store / AUIPC / JALR) ───────────────────
  { aluOp: 0, funct3: null, funct7b30: null, aluControl: 0b0010, mnemonic: 'ADD', note: 'load/store/AUIPC/JALR address' },

  // ── aluOp 01: force SUB (branch comparison) ─────────────────────────────
  { aluOp: 1, funct3: null, funct7b30: null, aluControl: 0b0110, mnemonic: 'SUB', note: 'branch compare A-B' },

  // ── aluOp 10: R-type and I-ALU — decode funct3 + funct7[30] ─────────────
  { aluOp: 2, funct3: 0b000, funct7b30: 0, aluControl: 0b0010, mnemonic: 'ADD',  note: 'ADD / ADDI' },
  { aluOp: 2, funct3: 0b000, funct7b30: 1, aluControl: 0b0110, mnemonic: 'SUB',  note: 'SUB (R-type only)' },
  { aluOp: 2, funct3: 0b001, funct7b30: 0, aluControl: 0b0100, mnemonic: 'SLL',  note: 'SLL / SLLI' },
  { aluOp: 2, funct3: 0b010, funct7b30: 0, aluControl: 0b0111, mnemonic: 'SLT',  note: 'SLT / SLTI' },
  { aluOp: 2, funct3: 0b011, funct7b30: 0, aluControl: 0b1000, mnemonic: 'SLTU', note: 'SLTU / SLTIU' },
  { aluOp: 2, funct3: 0b100, funct7b30: 0, aluControl: 0b0011, mnemonic: 'XOR',  note: 'XOR / XORI' },
  { aluOp: 2, funct3: 0b101, funct7b30: 0, aluControl: 0b0101, mnemonic: 'SRL',  note: 'SRL / SRLI' },
  { aluOp: 2, funct3: 0b101, funct7b30: 1, aluControl: 0b1001, mnemonic: 'SRA',  note: 'SRA / SRAI' },
  { aluOp: 2, funct3: 0b110, funct7b30: 0, aluControl: 0b0001, mnemonic: 'OR',   note: 'OR / ORI' },
  { aluOp: 2, funct3: 0b111, funct7b30: 0, aluControl: 0b0000, mnemonic: 'AND',  note: 'AND / ANDI' },

  // ── LUI: aluOp 10, funct3=000 but special — handled by opcode in decoder
  //    Decoder sets aluOp=10 for LUI and the ALU control outputs LUI pass-through
  { aluOp: 2, funct3: 0b000, funct7b30: 0, aluControl: 0b1010, mnemonic: 'LUI',  note: 'LUI pass-through B (overridden by opcode)' },
]

// ─────────────────────────────────────────────────────────────────────────────
// Compute function — used by AluControlUnit.vue and simulator core
// ─────────────────────────────────────────────────────────────────────────────

export function computeAluControl(
  aluOp:     0 | 1 | 2,
  funct3:    number,
  funct7b30: 0 | 1,
  isLui:     boolean = false,
): number {
  if (aluOp === 0) return 0b0010   // ADD
  if (aluOp === 1) return 0b0110   // SUB
  if (isLui)       return 0b1010   // LUI pass-through

  // aluOp === 2: decode funct fields
  const f3 = funct3 & 0x7
  const f7 = funct7b30 & 0x1

  const match = ALU_CONTROL_TABLE.find(e =>
    e.aluOp === 2 &&
    e.funct3     === f3 &&
    (e.funct7b30 === null || e.funct7b30 === f7)
  )

  return match?.aluControl ?? 0b0010  // fallback to ADD
}
