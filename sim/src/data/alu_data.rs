//! Single-cycle ALU: the 4-bit ALU control codes the panel shows, computed by
//! the shared ALU (`isa::alu`). Flags as the panel shows them: `carry` is the
//! borrow for SUB (1 = `a < b` unsigned), the carry out for ADD.

use crate::isa::alu::{self as isa_alu, AluOp};

/// ALU operation of a single-cycle ALU control code (`None` = unused code).
pub const fn op_of_code(code: u8) -> Option<AluOp> {
    Some(match code {
        0b0000 => AluOp::And,
        0b0001 => AluOp::Or,
        0b0010 => AluOp::Add,
        0b0011 => AluOp::Xor,
        0b0100 => AluOp::Sll,
        0b0101 => AluOp::Srl,
        0b0110 => AluOp::Sub,
        0b0111 => AluOp::Slt,
        0b1000 => AluOp::Sltu,
        0b1001 => AluOp::Sra,
        0b1010 => AluOp::PassB,   // LUI
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AluFlags {
    pub zero: bool,
    pub negative: bool,
    /// ADD: carry out · SUB: borrow (`a < b` unsigned) · others 0
    pub carry: bool,
    pub overflow: bool,
}

impl AluFlags {
    /// Flags in the shared form (carry = adder carry out) for a SUB result.
    pub const fn of_sub(self) -> isa_alu::Flags {
        isa_alu::Flags { zero: self.zero, negative: self.negative, carry: !self.carry, overflow: self.overflow }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AluResult {
    pub result: u32,
    pub flags: AluFlags,
}

/// The single-cycle ALU: control `code`, inputs `a`, `b`. An unused code
/// outputs 0.
pub const fn alu_compute(code: u8, a: u32, b: u32) -> AluResult {
    let Some(op) = op_of_code(code) else {
        return AluResult { result: 0, flags: AluFlags { zero: true, negative: false, carry: false, overflow: false } };
    };
    let f = isa_alu::flags(op, a, b);
    let borrow = matches!(op, AluOp::Sub);
    AluResult {
        result: isa_alu::alu(op, a, b),
        flags: AluFlags { zero: f.zero, negative: f.negative, carry: if borrow { !f.carry } else { f.carry }, overflow: f.overflow },
    }
}
