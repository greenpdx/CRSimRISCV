//! The integer ALU (RV32I operations) and its status flags.
//!
//! Every core computes through [`alu`] / [`flags`]; each keeps its own select
//! codes for display (`code()` tables live with the core).

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[cfg_attr(feature = "ts", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOp { Add, Sub, Sll, Slt, Sltu, Xor, Srl, Sra, Or, And, PassA, PassB }

impl AluOp {
    /// OP / OP-IMM operation from funct3 and instr[30] (`alt`: SUB / SRA / SRAI).
    /// `alt` must already be 0 for OP-IMM except the shifts (ADDI with
    /// imm[10] = 1 is still ADD).
    pub const fn from_funct3(funct3: u8, alt: bool) -> AluOp {
        match funct3 & 7 {
            0b000 => if alt { AluOp::Sub } else { AluOp::Add },
            0b001 => AluOp::Sll,
            0b010 => AluOp::Slt,
            0b011 => AluOp::Sltu,
            0b100 => AluOp::Xor,
            0b101 => if alt { AluOp::Sra } else { AluOp::Srl },
            0b110 => AluOp::Or,
            _     => AluOp::And,
        }
    }
}

/// Result of `op` on `a`, `b` (shift amounts use `b[4:0]`).
pub const fn alu(op: AluOp, a: u32, b: u32) -> u32 {
    let sh = b & 0x1f;
    match op {
        AluOp::Add  => a.wrapping_add(b),
        AluOp::Sub  => a.wrapping_sub(b),
        AluOp::Sll  => a << sh,
        AluOp::Slt  => ((a as i32) < (b as i32)) as u32,
        AluOp::Sltu => (a < b) as u32,
        AluOp::Xor  => a ^ b,
        AluOp::Srl  => a >> sh,
        AluOp::Sra  => ((a as i32) >> sh) as u32,
        AluOp::Or   => a | b,
        AluOp::And  => a & b,
        AluOp::PassA => a,
        AluOp::PassB => b,
    }
}

/// ALU status flags.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[cfg_attr(feature = "ts", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// result == 0
    pub zero: bool,
    /// result[31]
    pub negative: bool,
    /// adder carry out: ADD unsigned overflow; SUB `a >= b` (no borrow). 0 for other ops.
    pub carry: bool,
    /// signed overflow of ADD / SUB. 0 for other ops.
    pub overflow: bool,
}

/// Flags of `op` on `a`, `b`.
pub const fn flags(op: AluOp, a: u32, b: u32) -> Flags {
    let q = alu(op, a, b);
    let (carry, overflow) = match op {
        AluOp::Add => (a.overflowing_add(b).1, ((a ^ q) & (b ^ q)) >> 31 != 0),
        AluOp::Sub => (a >= b, ((a ^ b) & (a ^ q)) >> 31 != 0),
        _ => (false, false),
    };
    Flags { zero: q == 0, negative: q >> 31 != 0, carry, overflow }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops() {
        assert_eq!(alu(AluOp::Sub, 3, 5), 0xffff_fffe);
        assert_eq!(alu(AluOp::Sra, 0x8000_0000, 33), 0xc000_0000);   // shamt = b[4:0]
        assert_eq!(alu(AluOp::Slt, 0xffff_ffff, 0), 1);
        assert_eq!(alu(AluOp::Sltu, 0xffff_ffff, 0), 0);
        assert_eq!(AluOp::from_funct3(0b101, true), AluOp::Sra);
    }

    #[test]
    fn add_sub_flags() {
        assert_eq!(flags(AluOp::Add, 0xffff_ffff, 1), Flags { zero: true, negative: false, carry: true, overflow: false });
        assert_eq!(flags(AluOp::Add, 0x7fff_ffff, 1), Flags { zero: false, negative: true, carry: false, overflow: true });
        assert_eq!(flags(AluOp::Sub, 3, 5), Flags { zero: false, negative: true, carry: false, overflow: false });
        assert_eq!(flags(AluOp::Sub, 0, 0x8000_0000), Flags { zero: false, negative: true, carry: false, overflow: true });
        assert_eq!(flags(AluOp::Xor, 5, 5), Flags { zero: true, ..Flags::default() });
    }
}
