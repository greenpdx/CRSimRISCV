//! The barrel core's thread instructions: custom-0 opcode (0001011), the
//! operation in funct7, funct3 = 0. Only the barrel core implements them;
//! on every other core the custom-0 opcode is illegal.
//!
//!   fork    rd, rs1, rs2   start a thread at address rs1 with argument rs2; rd ← its slot, or -1
//!   join    rs1            wait until thread rs1 is done
//!   done                   end this thread and wake its joiners
//!   yield                  give up one scheduler round
//!   tid     rd             rd ← this thread's number
//!   forkret rd             like `done`, passing the value in rd back as the return value

/// One thread instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadOp { Fork, Join, Done, Yield, Tid, Forkret }

/// Which register operands an instruction is written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadOperands { RdRs1Rs2, Rs1, None, Rd }

pub const OPCODE_CUSTOM0: u32 = 0b0001011;

impl ThreadOp {
    pub const ALL: [ThreadOp; 6] = [ThreadOp::Fork, ThreadOp::Join, ThreadOp::Done, ThreadOp::Yield, ThreadOp::Tid, ThreadOp::Forkret];

    pub const fn mnemonic(self) -> &'static str {
        match self {
            ThreadOp::Fork => "fork", ThreadOp::Join => "join", ThreadOp::Done => "done",
            ThreadOp::Yield => "yield", ThreadOp::Tid => "tid", ThreadOp::Forkret => "forkret",
        }
    }

    pub const fn funct7(self) -> u32 {
        match self {
            ThreadOp::Fork => 0, ThreadOp::Join => 1, ThreadOp::Done => 2,
            ThreadOp::Yield => 3, ThreadOp::Tid => 4, ThreadOp::Forkret => 5,
        }
    }

    pub const fn from_funct7(f7: u32) -> Option<ThreadOp> {
        Some(match f7 {
            0 => ThreadOp::Fork, 1 => ThreadOp::Join, 2 => ThreadOp::Done,
            3 => ThreadOp::Yield, 4 => ThreadOp::Tid, 5 => ThreadOp::Forkret,
            _ => return None,
        })
    }

    /// `forkret` reads its value from the register in the rd field.
    pub const fn operands(self) -> ThreadOperands {
        match self {
            ThreadOp::Fork => ThreadOperands::RdRs1Rs2,
            ThreadOp::Join => ThreadOperands::Rs1,
            ThreadOp::Done | ThreadOp::Yield => ThreadOperands::None,
            ThreadOp::Tid | ThreadOp::Forkret => ThreadOperands::Rd,
        }
    }
}

/// Encode a thread instruction; fields the instruction does not use are 0.
pub const fn encode(op: ThreadOp, rd: u8, rs1: u8, rs2: u8) -> u32 {
    let (rd, rs1, rs2) = match op.operands() {
        ThreadOperands::RdRs1Rs2 => (rd, rs1, rs2),
        ThreadOperands::Rs1 => (0, rs1, 0),
        ThreadOperands::None => (0, 0, 0),
        ThreadOperands::Rd => (rd, 0, 0),
    };
    op.funct7() << 25 | (rs2 as u32 & 31) << 20 | (rs1 as u32 & 31) << 15 | (rd as u32 & 31) << 7 | OPCODE_CUSTOM0
}

/// The thread instruction a word encodes, if it is one.
pub const fn decode(w: u32) -> Option<ThreadOp> {
    if w & 0x7f != OPCODE_CUSTOM0 || (w >> 12) & 7 != 0 { return None; }
    ThreadOp::from_funct7(w >> 25)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings() {
        assert_eq!(encode(ThreadOp::Fork, 5, 10, 11), 0x00b5_028b);
        assert_eq!(encode(ThreadOp::Join, 0, 6, 0), 0x0203_000b);
        assert_eq!(encode(ThreadOp::Done, 0, 0, 0), 0x0400_000b);
        assert_eq!(encode(ThreadOp::Yield, 0, 0, 0), 0x0600_000b);
        assert_eq!(encode(ThreadOp::Tid, 7, 0, 0), 0x0800_038b);
        assert_eq!(encode(ThreadOp::Forkret, 10, 0, 0), 0x0a00_050b);
        for op in ThreadOp::ALL { assert_eq!(decode(encode(op, 3, 4, 5)), Some(op)); }
        assert_eq!(decode(0x0c00_000b), None);          // funct7 6: not a thread instruction
        assert_eq!(decode(0x0000_100b), None);          // funct3 ≠ 0
    }
}
