//! Conditional branches: the condition from funct3 and its decision, either
//! from a comparator or from the ALU flags of `rs1 - rs2`.

use crate::alu::Flags;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[cfg_attr(feature = "ts", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond { Eq, Ne, Lt, Ge, Ltu, Geu }

impl Cond {
    /// BEQ / BNE / BLT / BGE / BLTU / BGEU; `None` for funct3 010 / 011.
    pub const fn from_funct3(funct3: u8) -> Option<Cond> {
        match funct3 & 7 {
            0b000 => Some(Cond::Eq),
            0b001 => Some(Cond::Ne),
            0b100 => Some(Cond::Lt),
            0b101 => Some(Cond::Ge),
            0b110 => Some(Cond::Ltu),
            0b111 => Some(Cond::Geu),
            _ => None,
        }
    }
}

/// Comparator: is the branch `cond` taken for `rs1`, `rs2`?
pub const fn taken(cond: Cond, rs1: u32, rs2: u32) -> bool {
    match cond {
        Cond::Eq  => rs1 == rs2,
        Cond::Ne  => rs1 != rs2,
        Cond::Lt  => (rs1 as i32) < (rs2 as i32),
        Cond::Ge  => (rs1 as i32) >= (rs2 as i32),
        Cond::Ltu => rs1 < rs2,
        Cond::Geu => rs1 >= rs2,
    }
}

/// The same decision from the flags of the ALU computing `rs1 - rs2`:
/// signed less-than is N ≠ V; unsigned less-than is no carry out (borrow).
pub const fn taken_by_flags(cond: Cond, f: Flags) -> bool {
    match cond {
        Cond::Eq  => f.zero,
        Cond::Ne  => !f.zero,
        Cond::Lt  => f.negative != f.overflow,
        Cond::Ge  => f.negative == f.overflow,
        Cond::Ltu => !f.carry,
        Cond::Geu => f.carry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alu::{flags, AluOp};

    #[test]
    fn flags_agree_with_the_comparator() {
        let v = [0u32, 1, 2, 5, 0x7fff_fffe, 0x7fff_ffff, 0x8000_0000, 0x8000_0001, 0xffff_fffb, 0xffff_ffff];
        for f3 in [0u8, 1, 4, 5, 6, 7] {
            let c = Cond::from_funct3(f3).unwrap();
            for &a in &v { for &b in &v {
                assert_eq!(taken_by_flags(c, flags(AluOp::Sub, a, b)), taken(c, a, b), "{c:?} {a:#x} {b:#x}");
            }}
        }
        assert_eq!(Cond::from_funct3(0b010), None);
    }
}
