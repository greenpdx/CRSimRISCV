// ─────────────────────────────────────────────────────────────────────────────
// RV32I Immediate Generator — bit extraction table
//
// Input:  32-bit instruction word + ImmSel[2:0] from decoder
// Output: 32-bit sign-extended immediate
// ─────────────────────────────────────────────────────────────────────────────

export type ImmSel = 0 | 1 | 2 | 3 | 4 | 5   // matches controlData.ts

export interface ImmFormatDef {
  sel:         ImmSel
  name:        string       // 'I', 'S', 'B', 'U', 'J'
  description: string
  bits:        string       // human-readable bit extraction pattern
  usedBy:      string[]     // instruction types
}

export const IMM_FORMAT_DEFS: ImmFormatDef[] = [
  {
    sel: 0, name: 'I',
    description: 'Sign-extended 12-bit immediate',
    bits: 'inst[31:20]',
    usedBy: ['ADDI','SLTI','SLTIU','XORI','ORI','ANDI','SLLI','SRLI','SRAI','LW','LH','LB','LHU','LBU','JALR'],
  },
  {
    sel: 1, name: 'S',
    description: 'Sign-extended 12-bit store offset',
    bits: 'inst[31:25] | inst[11:7]',
    usedBy: ['SW','SH','SB'],
  },
  {
    sel: 2, name: 'B',
    description: 'Sign-extended 13-bit branch offset (LSB=0)',
    bits: 'inst[31] | inst[7] | inst[30:25] | inst[11:8] | 0',
    usedBy: ['BEQ','BNE','BLT','BGE','BLTU','BGEU'],
  },
  {
    sel: 3, name: 'U',
    description: '20-bit upper immediate (zero-padded lower 12)',
    bits: 'inst[31:12] | 000000000000',
    usedBy: ['LUI','AUIPC'],
  },
  {
    sel: 4, name: 'J',
    description: 'Sign-extended 21-bit jump offset (LSB=0)',
    bits: 'inst[31] | inst[19:12] | inst[20] | inst[30:21] | 0',
    usedBy: ['JAL'],
  },
  {
    sel: 5, name: 'none',
    description: 'No immediate (R-type)',
    bits: '—',
    usedBy: ['ADD','SUB','AND','OR','XOR','SLL','SRL','SRA','SLT','SLTU'],
  },
]

export const IMM_FORMAT_BY_SEL = new Map<ImmSel, ImmFormatDef>(
  IMM_FORMAT_DEFS.map(f => [f.sel, f])
)

// ─────────────────────────────────────────────────────────────────────────────
// Immediate extraction — pure function
// Used by ImmGenerator.vue and the simulator core
// ─────────────────────────────────────────────────────────────────────────────

export function extractImmediate(inst: number, sel: ImmSel): number {
  const i = inst >>> 0   // treat as unsigned 32-bit

  switch (sel) {

    case 0: { // I-type: inst[31:20], sign-extended
      const raw = (i >> 20) & 0xfff
      return signExtend(raw, 12)
    }

    case 1: { // S-type: inst[31:25] | inst[11:7], sign-extended
      const raw = ((i >> 25) & 0x7f) << 5 | (i >> 7) & 0x1f
      return signExtend(raw, 12)
    }

    case 2: { // B-type: inst[31]|inst[7]|inst[30:25]|inst[11:8], LSB=0
      const bit12  = (i >> 31) & 0x1
      const bit11  = (i >>  7) & 0x1
      const bit10_5 = (i >> 25) & 0x3f
      const bit4_1  = (i >>  8) & 0xf
      const raw = bit12 << 12 | bit11 << 11 | bit10_5 << 5 | bit4_1 << 1
      return signExtend(raw, 13)
    }

    case 3: { // U-type: inst[31:12] << 12, lower 12 bits = 0
      return (i & 0xfffff000) | 0    // already 32-bit, just sign as int32
    }

    case 4: { // J-type: inst[31]|inst[19:12]|inst[20]|inst[30:21], LSB=0
      const bit20    = (i >> 31) & 0x1
      const bit19_12 = (i >> 12) & 0xff
      const bit11    = (i >> 20) & 0x1
      const bit10_1  = (i >> 21) & 0x3ff
      const raw = bit20 << 20 | bit19_12 << 12 | bit11 << 11 | bit10_1 << 1
      return signExtend(raw, 21)
    }

    case 5: // R-type — no immediate
    default:
      return 0
  }
}

/** Sign-extend an n-bit value to 32-bit signed integer */
function signExtend(value: number, bits: number): number {
  const shift = 32 - bits
  return (value << shift) >> shift
}
