//! CSR addresses and Zicsr arithmetic (CSRRW / CSRRS / CSRRC and the
//! immediate forms). The CSR *file* (storage, views, counters) is core state
//! and lives in the simulator.

mod names;
pub use names::{csr_desc, csr_name};

/// Standard CSR addresses.
pub mod addr {
    // S-mode
    pub const SSTATUS:   u16 = 0x100;
    pub const SIE:       u16 = 0x104;
    pub const STVEC:     u16 = 0x105;
    pub const SSCRATCH:  u16 = 0x140;
    pub const SEPC:      u16 = 0x141;
    pub const SCAUSE:    u16 = 0x142;
    pub const STVAL:     u16 = 0x143;
    pub const SIP:       u16 = 0x144;
    pub const SATP:      u16 = 0x180;
    // M-mode
    pub const MSTATUS:   u16 = 0x300;
    pub const MISA:      u16 = 0x301;
    pub const MEDELEG:   u16 = 0x302;
    pub const MIDELEG:   u16 = 0x303;
    pub const MIE:       u16 = 0x304;
    pub const MTVEC:     u16 = 0x305;
    pub const MSCRATCH:  u16 = 0x340;
    pub const MEPC:      u16 = 0x341;
    pub const MCAUSE:    u16 = 0x342;
    pub const MTVAL:     u16 = 0x343;
    pub const MIP:       u16 = 0x344;
    pub const PMPCFG0:   u16 = 0x3A0;
    pub const PMPCFG1:   u16 = 0x3A1;
    /// pmpaddr0; pmpaddr1..7 follow at +1..+7
    pub const PMPADDR0:  u16 = 0x3B0;
    pub const MVENDORID: u16 = 0xF11;
    pub const MARCHID:   u16 = 0xF12;
    pub const MIMPID:    u16 = 0xF13;
    pub const MHARTID:   u16 = 0xF14;
    // Unprivileged counters (read-only views; the core advances them)
    pub const CYCLE:     u16 = 0xC00;
    pub const TIME:      u16 = 0xC01;
    pub const INSTRET:   u16 = 0xC02;
}

/// CSR operation from funct3[1:0].
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CsrOp { #[default] None, Rw, Rs, Rc }

impl CsrOp {
    /// From SYSTEM funct3 (bit 2, the immediate form, is ignored).
    pub const fn from_funct3(funct3: u8) -> CsrOp {
        match funct3 & 0b011 { 0b01 => CsrOp::Rw, 0b10 => CsrOp::Rs, 0b11 => CsrOp::Rc, _ => CsrOp::None }
    }
    /// From the 2-bit code 0 none · 1 RW · 2 RS · 3 RC.
    pub const fn from_u8(v: u8) -> CsrOp { CsrOp::from_funct3(v) }
    pub const fn to_u8(self) -> u8 {
        match self { CsrOp::None => 0, CsrOp::Rw => 1, CsrOp::Rs => 2, CsrOp::Rc => 3 }
    }
}

/// CSRRWI / CSRRSI / CSRRCI: funct3 bit 2 (rs1 field = zimm).
pub const fn is_imm_form(funct3: u8) -> bool { funct3 & 0b100 != 0 }

/// Result of a CSR instruction: rd ← `old`; CSR ← `new` when `write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsrResult { pub old: u32, pub new: u32, pub write: bool }

/// CSRRW / CSRRS / CSRRC and the immediate forms (funct3 bit 2 = zimm).
/// `rs1` is the register number (= zimm for the immediate forms).
/// RS / RC with rs1 = x0 (zimm = 0) do not write.
pub const fn csr_exec(old: u32, funct3: u8, rs1: u8, rs1_val: u32) -> CsrResult {
    let src = if is_imm_form(funct3) { rs1 as u32 } else { rs1_val };
    match funct3 & 0b011 {
        0b01 => CsrResult { old, new: src, write: true },
        0b10 => CsrResult { old, new: old | src, write: rs1 != 0 },
        0b11 => CsrResult { old, new: old & !src, write: rs1 != 0 },
        _    => CsrResult { old, new: old, write: false },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops() {
        assert_eq!(CsrOp::from_funct3(0b101), CsrOp::Rw);
        assert_eq!(CsrOp::from_funct3(0b100), CsrOp::None);
        assert_eq!(csr_exec(0b1100, 0b010, 5, 0b0011), CsrResult { old: 0b1100, new: 0b1111, write: true });
        assert_eq!(csr_exec(0b1100, 0b011, 0, 0b0100), CsrResult { old: 0b1100, new: 0b1000, write: false });
        assert_eq!(csr_exec(0b1100, 0b111, 4, 0), CsrResult { old: 0b1100, new: 0b1000, write: true });   // csrrci zimm = 4
    }
}
