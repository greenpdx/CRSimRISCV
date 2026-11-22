//! Pieces shared by the 5-stage pipeline and the barrel processor:
//! the generic stage register [`Latch`] and the typed control fields that
//! travel in it.
//!
//! Every enum keeps the numeric code the panels display (`code()`), so the
//! wasm JSON is unchanged.

use std::ops::{Deref, DerefMut};

pub use crate::isa::csr::CsrOp;

/// A stage register: valid bit, the instruction's PC and word, and the
/// stage-specific payload `data`. Derefs to `data`, so payload fields read
/// as `latch.field`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Latch<T> {
    pub valid: bool,
    pub pc: u32,
    pub instr: u32,
    pub data: T,
}

impl<T: Default> Latch<T> {
    /// An empty (invalid) stage.
    pub fn bubble() -> Self { Self::default() }
    /// A valid stage holding `instr` at `pc`.
    pub fn new(pc: u32, instr: u32, data: T) -> Self { Latch { valid: true, pc, instr, data } }
}

impl<T> Deref for Latch<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.data }
}
impl<T> DerefMut for Latch<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.data }
}

/// Write-back source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum WbSel {
    #[default]
    Alu = 0,
    Mem = 1,
    Pc4 = 2,
    /// CSR old value; carried in `mem_data` (pipeline).
    Csr = 3,
}

impl WbSel {
    pub const fn code(self) -> u8 { self as u8 }
    /// From the decoder table's code (0..3).
    pub fn from_code(c: u8) -> Self {
        match c {
            1 => WbSel::Mem,
            2 => WbSel::Pc4,
            3 => WbSel::Csr,
            _ => WbSel::Alu,
        }
    }
    /// The value written back (and forwarded from MA/WB).
    pub fn select(self, alu: u32, mem: u32, pc4: u32) -> u32 {
        match self {
            WbSel::Alu => alu,
            WbSel::Mem | WbSel::Csr => mem,
            WbSel::Pc4 => pc4,
        }
    }
}

/// Next-PC source (shown by the pipeline panel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PcSel {
    #[default]
    Pc4 = 0,
    PcImm = 1,
    Rs1Imm = 2,
}

impl PcSel {
    pub const fn code(self) -> u8 { self as u8 }
    pub fn from_code(c: u8) -> Self {
        match c {
            1 => PcSel::PcImm,
            2 => PcSel::Rs1Imm,
            _ => PcSel::Pc4,
        }
    }
}

/// Memory access of an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemKind {
    #[default]
    None,
    Load,
    Store,
}

impl MemKind {
    pub fn from_flags(read: bool, write: bool) -> Self {
        if write { MemKind::Store } else if read { MemKind::Load } else { MemKind::None }
    }
    pub fn is_load(self) -> bool { self == MemKind::Load }
    pub fn is_store(self) -> bool { self == MemKind::Store }
    pub fn is_access(self) -> bool { self != MemKind::None }
}

/// Synchronous trap (or xRET) an instruction carries down the pipe.
/// Codes are the pipeline's; the barrel only uses Illegal..Mret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TrapKind {
    #[default]
    None = 0,
    Illegal = 1,
    Ecall = 2,
    Ebreak = 3,
    Mret = 4,
    Sret = 5,
    InstrPageFault = 6,
    LoadPageFault = 7,
    StorePageFault = 8,
}

impl TrapKind {
    pub const fn code(self) -> u8 { self as u8 }
    pub fn is_some(self) -> bool { self != TrapKind::None }
}

/// Trap kind of a decoded SYSTEM funct3 = 0 instruction (WFI: none).
pub fn system_trap(i: &crate::isa::instr::Instr) -> TrapKind {
    use crate::isa::instr::Instr;
    match i {
        Instr::Ecall => TrapKind::Ecall,
        Instr::Ebreak => TrapKind::Ebreak,
        Instr::Mret => TrapKind::Mret,
        Instr::Sret => TrapKind::Sret,
        Instr::Wfi => TrapKind::None,
        _ => TrapKind::Illegal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for c in 0..4 { assert_eq!(WbSel::from_code(c).code(), c); }
        for c in 0..3 { assert_eq!(PcSel::from_code(c).code(), c); }
        assert_eq!(TrapKind::StorePageFault.code(), 8);
        assert_eq!(MemKind::from_flags(true, false), MemKind::Load);
        assert_eq!(MemKind::from_flags(false, true), MemKind::Store);
        assert_eq!(MemKind::from_flags(false, false), MemKind::None);
    }

    #[test]
    fn wb_select() {
        assert_eq!(WbSel::Alu.select(1, 2, 3), 1);
        assert_eq!(WbSel::Mem.select(1, 2, 3), 2);
        assert_eq!(WbSel::Csr.select(1, 2, 3), 2);
        assert_eq!(WbSel::Pc4.select(1, 2, 3), 3);
    }

    #[test]
    fn latch_derefs_to_payload() {
        #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
        struct P { x: u32 }
        let mut l = Latch::new(0x40, 0x13, P { x: 5 });
        assert!(l.valid && l.pc == 0x40 && l.x == 5);
        l.x = 7;
        assert_eq!(l.data.x, 7);
        assert!(!Latch::<P>::bubble().valid);
    }
}
