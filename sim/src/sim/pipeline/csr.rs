//! Zicsr helpers for the pipeline: CSR addresses, op decode and the list
//! of CSRs the view shows. The CSR file, trap entry / return and IRQ
//! sampling are the shared system unit (`crate::sim::system::SysUnit`).
//! PMP CSRs are storage only (no enforcement).

// ── M-mode CSR addresses ─────────────────────────────────────────────
pub const CSR_MSTATUS:   u16 = 0x300;
pub const CSR_MISA:      u16 = 0x301;
pub const CSR_MIE:       u16 = 0x304;
pub const CSR_MTVEC:     u16 = 0x305;
pub const CSR_MSCRATCH:  u16 = 0x340;
pub const CSR_MEPC:      u16 = 0x341;
pub const CSR_MCAUSE:    u16 = 0x342;
pub const CSR_MTVAL:     u16 = 0x343;
pub const CSR_MIP:       u16 = 0x344;

pub const CSR_MVENDORID: u16 = 0xF11;
pub const CSR_MARCHID:   u16 = 0xF12;
pub const CSR_MIMPID:    u16 = 0xF13;
pub const CSR_MHARTID:   u16 = 0xF14;

// ── S-mode CSR addresses (Phase 4.1, mostly views over M-mode) ───────
pub const CSR_SSTATUS:  u16 = 0x100;
pub const CSR_SIE:      u16 = 0x104;
pub const CSR_STVEC:    u16 = 0x105;
pub const CSR_SSCRATCH: u16 = 0x140;
pub const CSR_SEPC:     u16 = 0x141;
pub const CSR_SCAUSE:   u16 = 0x142;
pub const CSR_STVAL:    u16 = 0x143;
pub const CSR_SIP:      u16 = 0x144;
pub const CSR_SATP:     u16 = 0x180;

pub const CSR_MEDELEG:  u16 = 0x302;
pub const CSR_MIDELEG:  u16 = 0x303;

/// Bits visible/writable through `sstatus` (subset view of mstatus).
/// SIE(1), SPIE(5), SPP(8), SUM(18), MXR(19).
pub const SSTATUS_MASK: u32 = (1<<1) | (1<<5) | (1<<8) | (1<<18) | (1<<19);
/// Bits visible through `sie`/`sip`: SSIE/SSIP(1), STIE/STIP(5), SEIE/SEIP(9).
pub const SI_MASK: u32 = (1<<1) | (1<<5) | (1<<9);

// ── PMP CSR addresses (8 entries) ────────────────────────────────────
pub const CSR_PMPCFG0:   u16 = 0x3A0;
pub const CSR_PMPCFG1:   u16 = 0x3A1;
pub const CSR_PMPADDR0:  u16 = 0x3B0;
// pmpaddr1..7 are 0x3B1..0x3B7

/// CSR operation kind decoded from a SYSTEM instruction's funct3.
/// 0 = none (not a CSR op).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsrOp { None, Rw, Rs, Rc }

impl CsrOp {
    pub fn from_u8(v: u8) -> CsrOp {
        match v { 1 => CsrOp::Rw, 2 => CsrOp::Rs, 3 => CsrOp::Rc, _ => CsrOp::None }
    }
    pub fn to_u8(self) -> u8 {
        match self { CsrOp::None => 0, CsrOp::Rw => 1, CsrOp::Rs => 2, CsrOp::Rc => 3 }
    }
}

/// Decode SYSTEM funct3 → CsrOp. funct3 bit2 selects immediate variant
/// (CSRRWI/CSRRSI/CSRRCI), but the operation is the same; bit2 is
/// handled by the caller.
pub fn csr_op_from_funct3(funct3: u8) -> CsrOp {
    match funct3 & 0b011 {
        0b001 => CsrOp::Rw,
        0b010 => CsrOp::Rs,
        0b011 => CsrOp::Rc,
        _ => CsrOp::None,
    }
}

/// True if `funct3` is an immediate-form CSR op (CSRRWI/CSRRSI/CSRRCI).
pub fn csr_is_imm_form(funct3: u8) -> bool { (funct3 & 0b100) != 0 }

/// The CSRs the pipeline view lists (wasm snapshot), in order. Storage,
/// trap entry / return and IRQ sampling are the shared system unit
/// (`crate::sim::system::SysUnit`, `PipelineState::sys`).
pub fn known_addrs() -> &'static [u16] {
    &[
        CSR_MSTATUS, CSR_MISA, CSR_MIE, CSR_MTVEC, CSR_MSCRATCH,
        CSR_MEPC, CSR_MCAUSE, CSR_MTVAL, CSR_MIP,
        CSR_MVENDORID, CSR_MARCHID, CSR_MIMPID, CSR_MHARTID,
        CSR_PMPCFG0, CSR_PMPCFG1,
        CSR_PMPADDR0, CSR_PMPADDR0+1, CSR_PMPADDR0+2, CSR_PMPADDR0+3,
        CSR_PMPADDR0+4, CSR_PMPADDR0+5, CSR_PMPADDR0+6, CSR_PMPADDR0+7,
        CSR_SSTATUS, CSR_SIE, CSR_STVEC, CSR_SSCRATCH, CSR_SEPC,
        CSR_SCAUSE, CSR_STVAL, CSR_SIP, CSR_SATP,
        CSR_MEDELEG, CSR_MIDELEG,
    ]
}
