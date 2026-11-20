//! Privilege modes, the CSR file and Zicsr execution (shared by all cores).

use std::collections::BTreeMap;
use super::irq::{IRQ_SSI, IRQ_STI, IRQ_SEI};

#[cfg_attr(feature = "wasm", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PrivMode { U = 0, S = 1, M = 3 }

impl PrivMode {
    pub fn as_str(self) -> &'static str { match self { PrivMode::U=>"U", PrivMode::S=>"S", PrivMode::M=>"M" } }
    pub fn from_bits(b: u8) -> PrivMode { match b & 0x3 { 1 => PrivMode::S, 0 => PrivMode::U, _ => PrivMode::M } }
}

// ── Standard CSR addresses ─────────────────────────────────────────────────
pub mod addr {
    // S-mode
    pub const SSTATUS: u16 = 0x100;
    pub const SIE:     u16 = 0x104;
    pub const STVEC:   u16 = 0x105;
    pub const SSCRATCH:u16 = 0x140;
    pub const SEPC:    u16 = 0x141;
    pub const SCAUSE:  u16 = 0x142;
    pub const STVAL:   u16 = 0x143;
    pub const SIP:     u16 = 0x144;
    pub const SATP:    u16 = 0x180;
    // M-mode
    pub const MSTATUS: u16 = 0x300;
    pub const MISA:    u16 = 0x301;
    pub const MEDELEG: u16 = 0x302;
    pub const MIDELEG: u16 = 0x303;
    pub const MIE:     u16 = 0x304;
    pub const MTVEC:   u16 = 0x305;
    pub const MSCRATCH:u16 = 0x340;
    pub const MEPC:    u16 = 0x341;
    pub const MCAUSE:  u16 = 0x342;
    pub const MTVAL:   u16 = 0x343;
    pub const MIP:     u16 = 0x344;
    pub const MVENDORID: u16 = 0xF11;
    pub const MARCHID: u16 = 0xF12;
    pub const MIMPID:  u16 = 0xF13;
    pub const MHARTID: u16 = 0xF14;
    // Unprivileged counters (read-only views; the core advances them)
    pub const CYCLE:   u16 = 0xC00;
    pub const TIME:    u16 = 0xC01;
    pub const INSTRET: u16 = 0xC02;
}

// ── mstatus / sstatus bit positions ────────────────────────────────────────
// We model the subset relevant to traps:
//   SIE  bit 1
//   MIE  bit 3
//   SPIE bit 5
//   MPIE bit 7
//   SPP  bit 8       (1 bit)
//   MPP  bits[12:11] (2 bits)
pub const M_MSTATUS_SIE:  u32 = 1 << 1;
pub const M_MSTATUS_MIE:  u32 = 1 << 3;
pub const M_MSTATUS_SPIE: u32 = 1 << 5;
pub const M_MSTATUS_MPIE: u32 = 1 << 7;
pub const M_MSTATUS_SPP:  u32 = 1 << 8;
pub const M_MSTATUS_MPP_SHIFT: u32 = 11;
pub const M_MSTATUS_MPP_MASK:  u32 = 0x3 << M_MSTATUS_MPP_SHIFT;

/// mstatus.SUM (bit 18) — S-mode may access U pages (Sv32).
pub const M_MSTATUS_SUM:  u32 = 1 << 18;
/// mstatus.MXR (bit 19) — loads from executable pages (Sv32).
pub const M_MSTATUS_MXR:  u32 = 1 << 19;

// sstatus is a *view* of mstatus restricted to the S-visible bits:
// SIE, SPIE, SPP and the Sv32 bits SUM, MXR.
pub const SSTATUS_VIEW_MASK: u32 =
    M_MSTATUS_SIE | M_MSTATUS_SPIE | M_MSTATUS_SPP | M_MSTATUS_SUM | M_MSTATUS_MXR;

// sie/sip view: only the S-visible bits
pub const SIRQ_VIEW_MASK: u32 = IRQ_SSI | IRQ_STI | IRQ_SEI;

// ── CsrFile ────────────────────────────────────────────────────────────────
#[cfg_attr(feature = "wasm", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default)]
pub struct CsrFile {
    regs: BTreeMap<u16, u32>,
}

impl CsrFile {
    pub fn new() -> Self {
        let mut f = CsrFile { regs: BTreeMap::new() };
        // Pre-populate the trap CSRs so they always show in snapshots.
        for &a in &[
            addr::MSTATUS, addr::MTVEC, addr::MIE, addr::MIP, addr::MEPC,
            addr::MCAUSE, addr::MTVAL, addr::MSCRATCH, addr::MIDELEG, addr::MEDELEG,
            addr::SSTATUS, addr::STVEC, addr::SIE, addr::SIP, addr::SEPC,
            addr::SCAUSE, addr::STVAL, addr::SSCRATCH,
            addr::MHARTID, addr::CYCLE, addr::INSTRET,
        ] { f.regs.insert(a, 0); }
        f
    }

    pub fn read(&self, addr: u16) -> u32 {
        match addr {
            addr::TIME => self.regs.get(&addr::CYCLE).copied().unwrap_or(0),
            // sstatus / sie / sip are *views* into the m-mode regs
            addr::SSTATUS => self.regs.get(&addr::MSTATUS).copied().unwrap_or(0) & SSTATUS_VIEW_MASK,
            addr::SIE     => self.regs.get(&addr::MIE).copied().unwrap_or(0) & SIRQ_VIEW_MASK,
            addr::SIP     => self.regs.get(&addr::MIP).copied().unwrap_or(0) & SIRQ_VIEW_MASK,
            _ => self.regs.get(&addr).copied().unwrap_or(0),
        }
    }

    pub fn write(&mut self, addr: u16, val: u32) {
        match addr {
            addr::SSTATUS => {
                let old = self.regs.get(&addr::MSTATUS).copied().unwrap_or(0);
                let new = (old & !SSTATUS_VIEW_MASK) | (val & SSTATUS_VIEW_MASK);
                self.regs.insert(addr::MSTATUS, new);
            }
            addr::SIE => {
                let old = self.regs.get(&addr::MIE).copied().unwrap_or(0);
                let new = (old & !SIRQ_VIEW_MASK) | (val & SIRQ_VIEW_MASK);
                self.regs.insert(addr::MIE, new);
            }
            addr::SIP => {
                let old = self.regs.get(&addr::MIP).copied().unwrap_or(0);
                let new = (old & !SIRQ_VIEW_MASK) | (val & SIRQ_VIEW_MASK);
                self.regs.insert(addr::MIP, new);
            }
            // xEPC: bit 0 is always 0 (IALIGN = 32 / 16)
            addr::MEPC | addr::SEPC => { self.regs.insert(addr, val & !1); }
            // Read-only CSRs ignored (the core sets them with set_counter);
            // everything else stored verbatim.
            addr::MISA | addr::MVENDORID | addr::MARCHID | addr::MIMPID |
            addr::MHARTID | addr::CYCLE | addr::TIME | addr::INSTRET => {}
            _ => { self.regs.insert(addr, val); }
        }
    }

    /// Mutate one bit-field of mstatus directly. Used by trap entry/exit.
    pub fn mstatus_clear_set(&mut self, clear: u32, set: u32) {
        let cur = self.regs.get(&addr::MSTATUS).copied().unwrap_or(0);
        self.regs.insert(addr::MSTATUS, (cur & !clear) | set);
    }

    pub fn raw(&self) -> &BTreeMap<u16, u32> { &self.regs }

    /// Advance a counter CSR (cycle / instret) — hardware side, ignores read-only.
    pub fn bump(&mut self, addr: u16) {
        let v = self.regs.get(&addr).copied().unwrap_or(0).wrapping_add(1);
        self.regs.insert(addr, v);
    }

    /// Set a counter / read-only CSR from the core (hardware side), e.g.
    /// cycle, instret, misa, mhartid.
    pub fn set_counter(&mut self, addr: u16, v: u32) { self.regs.insert(addr, v); }
}


// ── Zicsr execution ────────────────────────────────────────────────────────

/// Result of a CSR instruction: rd ← `old`; CSR ← `new` when `write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsrResult { pub old: u32, pub new: u32, pub write: bool }

/// CSRRW / CSRRS / CSRRC and the immediate forms (funct3 bit 2 = zimm).
/// `rs1` is the register number (= zimm for the immediate forms).
/// RS / RC with rs1 = x0 (zimm = 0) do not write.
pub fn csr_exec(old: u32, funct3: u8, rs1: u8, rs1_val: u32) -> CsrResult {
    let src = if funct3 & 0b100 != 0 { rs1 as u32 } else { rs1_val };
    match funct3 & 0b011 {
        0b01 => CsrResult { old, new: src, write: true },
        0b10 => CsrResult { old, new: old | src, write: rs1 != 0 },
        0b11 => CsrResult { old, new: old & !src, write: rs1 != 0 },
        _    => CsrResult { old, new: old, write: false },
    }
}
