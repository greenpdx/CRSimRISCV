//! RV32I + Zmmul decode for the out-of-order core: which unit an
//! instruction goes to, its registers and its immediate.

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// ALU 0 / ALU 1 (1 cycle)
    Alu,
    /// Pipelined multiplier (3 cycles)
    Mul,
    /// Branch unit: conditional branches, JAL, JALR (1 cycle)
    Br,
    /// Load / store unit, through the load / store queue
    Lsu,
    /// CSR, ECALL, EBREAK, MRET, FENCE, WFI: wait for an empty ROB, run at commit
    Sys,
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOp { Add, Sub, Sll, Slt, Sltu, Xor, Srl, Sra, Or, And, Lui, Auipc }

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// ALU operation; `imm` = second operand is the immediate
    Alu { op: AluOp, imm: bool },
    /// MUL / MULH / MULHSU / MULHU (funct3 0..3)
    Mul { f3: u8 },
    /// BEQ … BGEU (funct3)
    Branch { f3: u8 },
    Jal,
    Jalr,
    /// size 1 / 2 / 4 bytes
    Load { size: u8, signed: bool },
    Store { size: u8 },
    Csr { f3: u8, csr: u16 },
    Ecall,
    Ebreak,
    Mret,
    /// FENCE, FENCE.I, SFENCE.VMA, WFI: no effect here (memory is in order)
    Fence,
    Illegal,
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded {
    pub op: Op,
    pub unit: Unit,
    /// Destination register (0 = none)
    pub rd: u8,
    /// Source registers (0 = not used / x0)
    pub rs1: u8,
    pub rs2: u8,
    pub imm: u32,
}

const fn imm_i(i: u32) -> u32 { ((i as i32) >> 20) as u32 }
const fn imm_s(i: u32) -> u32 { ((((i as i32) >> 25) << 5) as u32) | ((i >> 7) & 0x1f) }
const fn imm_b(i: u32) -> u32 {
    let v = ((i >> 31) & 1) << 12 | ((i >> 7) & 1) << 11 | ((i >> 25) & 0x3f) << 5 | ((i >> 8) & 0xf) << 1;
    ((v << 19) as i32 >> 19) as u32
}
const fn imm_u(i: u32) -> u32 { i & 0xffff_f000 }
const fn imm_j(i: u32) -> u32 {
    let v = ((i >> 31) & 1) << 20 | ((i >> 12) & 0xff) << 12 | ((i >> 20) & 1) << 11 | ((i >> 21) & 0x3ff) << 1;
    ((v << 11) as i32 >> 11) as u32
}

pub fn decode(i: u32) -> Decoded {
    let opc = i & 0x7f;
    let rd = ((i >> 7) & 0x1f) as u8;
    let f3 = ((i >> 12) & 7) as u8;
    let rs1 = ((i >> 15) & 0x1f) as u8;
    let rs2 = ((i >> 20) & 0x1f) as u8;
    let f7 = (i >> 25) as u8;
    let ill = Decoded { op: Op::Illegal, unit: Unit::Sys, rd: 0, rs1: 0, rs2: 0, imm: i };
    let d = |op, unit, rd, rs1, rs2, imm| Decoded { op, unit, rd, rs1, rs2, imm };
    match opc {
        0x37 => d(Op::Alu { op: AluOp::Lui, imm: true }, Unit::Alu, rd, 0, 0, imm_u(i)),
        0x17 => d(Op::Alu { op: AluOp::Auipc, imm: true }, Unit::Alu, rd, 0, 0, imm_u(i)),
        0x6f => d(Op::Jal, Unit::Br, rd, 0, 0, imm_j(i)),
        0x67 if f3 == 0 => d(Op::Jalr, Unit::Br, rd, rs1, 0, imm_i(i)),
        0x63 if f3 != 2 && f3 != 3 => d(Op::Branch { f3 }, Unit::Br, 0, rs1, rs2, imm_b(i)),
        0x03 => match f3 {
            0 => d(Op::Load { size: 1, signed: true }, Unit::Lsu, rd, rs1, 0, imm_i(i)),
            1 => d(Op::Load { size: 2, signed: true }, Unit::Lsu, rd, rs1, 0, imm_i(i)),
            2 => d(Op::Load { size: 4, signed: false }, Unit::Lsu, rd, rs1, 0, imm_i(i)),
            4 => d(Op::Load { size: 1, signed: false }, Unit::Lsu, rd, rs1, 0, imm_i(i)),
            5 => d(Op::Load { size: 2, signed: false }, Unit::Lsu, rd, rs1, 0, imm_i(i)),
            _ => ill,
        },
        0x23 if f3 <= 2 => d(Op::Store { size: 1 << f3 }, Unit::Lsu, 0, rs1, rs2, imm_s(i)),
        0x13 => {
            let op = match (f3, f7) {
                (0, _) => AluOp::Add, (2, _) => AluOp::Slt, (3, _) => AluOp::Sltu,
                (4, _) => AluOp::Xor, (6, _) => AluOp::Or, (7, _) => AluOp::And,
                (1, 0) => AluOp::Sll, (5, 0) => AluOp::Srl, (5, 0x20) => AluOp::Sra,
                _ => return ill,
            };
            d(Op::Alu { op, imm: true }, Unit::Alu, rd, rs1, 0, imm_i(i))
        }
        0x33 if f7 == 1 => if f3 <= 3 { d(Op::Mul { f3 }, Unit::Mul, rd, rs1, rs2, 0) } else { ill },   // Zmmul: no divide
        0x33 => {
            let op = match (f3, f7) {
                (0, 0) => AluOp::Add, (0, 0x20) => AluOp::Sub, (1, 0) => AluOp::Sll,
                (2, 0) => AluOp::Slt, (3, 0) => AluOp::Sltu, (4, 0) => AluOp::Xor,
                (5, 0) => AluOp::Srl, (5, 0x20) => AluOp::Sra, (6, 0) => AluOp::Or, (7, 0) => AluOp::And,
                _ => return ill,
            };
            d(Op::Alu { op, imm: false }, Unit::Alu, rd, rs1, rs2, 0)
        }
        0x0f => d(Op::Fence, Unit::Sys, 0, 0, 0, 0),
        0x73 => match f3 {
            0 => match i >> 20 {
                0 if rd == 0 && rs1 == 0 => d(Op::Ecall, Unit::Sys, 0, 0, 0, 0),
                1 if rd == 0 && rs1 == 0 => d(Op::Ebreak, Unit::Sys, 0, 0, 0, 0),
                0x302 => d(Op::Mret, Unit::Sys, 0, 0, 0, 0),
                0x105 => d(Op::Fence, Unit::Sys, 0, 0, 0, 0),                       // WFI
                _ if f7 == 0x09 => d(Op::Fence, Unit::Sys, 0, 0, 0, 0),             // SFENCE.VMA
                _ => ill,
            },
            4 => ill,
            _ => {
                // CSRRW/S/C use rs1; the I forms (f3 ≥ 5) carry zimm in the rs1 field
                let src = if f3 < 4 { rs1 } else { 0 };
                Decoded { op: Op::Csr { f3, csr: (i >> 20) as u16 }, unit: Unit::Sys, rd, rs1: src, rs2: 0, imm: rs1 as u32 }
            }
        },
        _ => ill,
    }
}

/// ALU result (both operands already chosen: `b` = rs2 or the immediate).
pub fn alu(op: AluOp, a: u32, b: u32, pc: u32) -> u32 {
    match op {
        AluOp::Add => a.wrapping_add(b),
        AluOp::Sub => a.wrapping_sub(b),
        AluOp::Sll => a << (b & 31),
        AluOp::Slt => ((a as i32) < (b as i32)) as u32,
        AluOp::Sltu => (a < b) as u32,
        AluOp::Xor => a ^ b,
        AluOp::Srl => a >> (b & 31),
        AluOp::Sra => ((a as i32) >> (b & 31)) as u32,
        AluOp::Or => a | b,
        AluOp::And => a & b,
        AluOp::Lui => b,
        AluOp::Auipc => pc.wrapping_add(b),
    }
}

/// MUL family (Zmmul).
pub fn mul(f3: u8, a: u32, b: u32) -> u32 {
    match f3 {
        0 => a.wrapping_mul(b),
        1 => ((a as i32 as i64 * b as i32 as i64) >> 32) as u32,
        2 => ((a as i32 as i128 * b as u64 as i128) >> 32) as u32,
        _ => ((a as u64 * b as u64) >> 32) as u32,
    }
}

/// Conditional branch outcome.
pub fn branch_taken(f3: u8, a: u32, b: u32) -> bool {
    match f3 {
        0 => a == b,
        1 => a != b,
        4 => (a as i32) < (b as i32),
        5 => (a as i32) >= (b as i32),
        6 => a < b,
        _ => a >= b,
    }
}
