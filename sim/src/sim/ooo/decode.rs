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

/// The core's view of an instruction (`isa::instr::decode`): its unit, its
/// registers and its immediate.
pub fn decode(i: u32) -> Decoded {
    use crate::isa::alu::AluOp as A;
    use crate::isa::instr::{decode, Instr};
    let alu = |op: A| match op {
        A::Add => AluOp::Add, A::Sub => AluOp::Sub, A::Sll => AluOp::Sll, A::Slt => AluOp::Slt,
        A::Sltu => AluOp::Sltu, A::Xor => AluOp::Xor, A::Srl => AluOp::Srl, A::Sra => AluOp::Sra,
        A::Or => AluOp::Or, A::And => AluOp::And, A::PassA | A::PassB => unreachable!("no RV32I encoding"),
    };
    let d = |op, unit, rd, rs1, rs2, imm| Decoded { op, unit, rd, rs1, rs2, imm };
    let sys = |op| d(op, Unit::Sys, 0, 0, 0, 0);
    match decode(i) {
        Instr::Lui { rd, imm } => d(Op::Alu { op: AluOp::Lui, imm: true }, Unit::Alu, rd, 0, 0, imm),
        Instr::Auipc { rd, imm } => d(Op::Alu { op: AluOp::Auipc, imm: true }, Unit::Alu, rd, 0, 0, imm),
        Instr::Jal { rd, imm } => d(Op::Jal, Unit::Br, rd, 0, 0, imm),
        Instr::Jalr { rd, rs1, imm } => d(Op::Jalr, Unit::Br, rd, rs1, 0, imm),
        Instr::Branch { rs1, rs2, imm, .. } => d(Op::Branch { f3: ((i >> 12) & 7) as u8 }, Unit::Br, 0, rs1, rs2, imm),
        Instr::Load { rd, rs1, imm, size, signed } => d(Op::Load { size, signed: signed && size < 4 }, Unit::Lsu, rd, rs1, 0, imm),
        Instr::Store { rs1, rs2, imm, size } => d(Op::Store { size }, Unit::Lsu, 0, rs1, rs2, imm),
        Instr::OpImm { op, rd, rs1, imm } => d(Op::Alu { op: alu(op), imm: true }, Unit::Alu, rd, rs1, 0, imm),
        Instr::Op { op, rd, rs1, rs2 } => d(Op::Alu { op: alu(op), imm: false }, Unit::Alu, rd, rs1, rs2, 0),
        Instr::Mul { op, rd, rs1, rs2 } => d(Op::Mul { f3: op }, Unit::Mul, rd, rs1, rs2, 0),
        Instr::Fence | Instr::FenceI | Instr::Wfi | Instr::SfenceVma { .. } => sys(Op::Fence),
        Instr::Ecall => sys(Op::Ecall),
        Instr::Ebreak => sys(Op::Ebreak),
        Instr::Mret => sys(Op::Mret),
        // CSRRW/S/C use rs1; the I forms carry zimm in the rs1 field
        Instr::Csr { op, imm_form, rd, src, csr } => Decoded {
            op: Op::Csr { f3: op.to_u8() | (imm_form as u8) << 2, csr }, unit: Unit::Sys,
            rd, rs1: if imm_form { 0 } else { src }, rs2: 0, imm: src as u32,
        },
        // SRET (no S-mode), custom-0, reserved encodings
        Instr::Sret | Instr::Custom0 { .. } | Instr::Illegal =>
            Decoded { op: Op::Illegal, unit: Unit::Sys, rd: 0, rs1: 0, rs2: 0, imm: i },
    }
}

/// ALU result (both operands already chosen: `b` = rs2 or the immediate).
pub fn alu(op: AluOp, a: u32, b: u32, pc: u32) -> u32 {
    use crate::isa::alu::{alu, AluOp as A};
    let (op, a) = match op {
        AluOp::Add => (A::Add, a), AluOp::Sub => (A::Sub, a), AluOp::Sll => (A::Sll, a),
        AluOp::Slt => (A::Slt, a), AluOp::Sltu => (A::Sltu, a), AluOp::Xor => (A::Xor, a),
        AluOp::Srl => (A::Srl, a), AluOp::Sra => (A::Sra, a), AluOp::Or => (A::Or, a),
        AluOp::And => (A::And, a), AluOp::Lui => (A::PassB, a), AluOp::Auipc => (A::Add, pc),
    };
    alu(op, a, b)
}

/// MUL family (Zmmul).
pub fn mul(f3: u8, a: u32, b: u32) -> u32 { crate::isa::mul::mul(f3, a, b) }

/// Conditional branch outcome (funct3 010 / 011 never decode as branches).
pub fn branch_taken(f3: u8, a: u32, b: u32) -> bool {
    use crate::isa::branch::{taken, Cond};
    match Cond::from_funct3(f3) { Some(c) => taken(c, a, b), None => false }
}
