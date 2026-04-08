//! Zicsr CSR file — Phase 1 (storage only, no trap delivery, no PMP enforcement).
//!
//! Holds the M-mode basic CSRs and 8 PMP entries. Reads return 0 for
//! unmapped addresses; writes to unmapped or read-only CSRs are
//! silently ignored. WARL masking is minimal in v1.

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

#[derive(Debug, Clone)]
pub struct CsrFile {
    pub mstatus:   u32,
    pub misa:      u32,
    pub mie:       u32,
    pub mtvec:     u32,
    pub mscratch:  u32,
    pub mepc:      u32,
    pub mcause:    u32,
    pub mtval:     u32,
    pub mip:       u32,

    // Read-only informational
    pub mvendorid: u32,
    pub marchid:   u32,
    pub mimpid:    u32,
    pub mhartid:   u32,

    // PMP — 8 entries
    pub pmpcfg0:  u32,
    pub pmpcfg1:  u32,
    pub pmpaddr:  [u32; 8],

    // S-mode (Phase 4.1). sstatus/sie/sip are masked views over the
    // M-mode counterparts; the rest are independent storage.
    pub stvec:    u32,
    pub sscratch: u32,
    pub sepc:     u32,
    pub scause:   u32,
    pub stval:    u32,
    pub satp:     u32,
    pub medeleg:  u32,
    pub mideleg:  u32,
}

impl Default for CsrFile {
    fn default() -> Self {
        Self {
            mstatus: 0, misa: 0x4000_0100,  // MXL=1 (RV32), I extension bit
            mie: 0, mtvec: 0, mscratch: 0, mepc: 0, mcause: 0, mtval: 0, mip: 0,
            mvendorid: 0, marchid: 0, mimpid: 0, mhartid: 0,
            pmpcfg0: 0, pmpcfg1: 0, pmpaddr: [0; 8],
            stvec: 0, sscratch: 0, sepc: 0, scause: 0, stval: 0,
            satp: 0, medeleg: 0, mideleg: 0,
        }
    }
}

impl CsrFile {
    pub fn new() -> Self { Self::default() }

    pub fn read(&self, addr: u16) -> u32 {
        match addr {
            CSR_MSTATUS   => self.mstatus,
            CSR_MISA      => self.misa,
            CSR_MIE       => self.mie,
            CSR_MTVEC     => self.mtvec,
            CSR_MSCRATCH  => self.mscratch,
            CSR_MEPC      => self.mepc,
            CSR_MCAUSE    => self.mcause,
            CSR_MTVAL     => self.mtval,
            CSR_MIP       => self.mip,
            CSR_MVENDORID => self.mvendorid,
            CSR_MARCHID   => self.marchid,
            CSR_MIMPID    => self.mimpid,
            CSR_MHARTID   => self.mhartid,
            CSR_PMPCFG0   => self.pmpcfg0,
            CSR_PMPCFG1   => self.pmpcfg1,
            a if (CSR_PMPADDR0..CSR_PMPADDR0 + 8).contains(&a) =>
                self.pmpaddr[(a - CSR_PMPADDR0) as usize],

            // S-mode views over M-mode storage
            CSR_SSTATUS => self.mstatus & SSTATUS_MASK,
            CSR_SIE     => self.mie & SI_MASK,
            CSR_SIP     => self.mip & SI_MASK,
            CSR_STVEC   => self.stvec,
            CSR_SSCRATCH=> self.sscratch,
            CSR_SEPC    => self.sepc,
            CSR_SCAUSE  => self.scause,
            CSR_STVAL   => self.stval,
            CSR_SATP    => self.satp,
            CSR_MEDELEG => self.medeleg,
            CSR_MIDELEG => self.mideleg,

            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, val: u32) {
        match addr {
            CSR_MSTATUS   => self.mstatus  = val,
            CSR_MISA      => { /* WARL: ignore writes in v1 */ }
            CSR_MIE       => self.mie      = val,
            CSR_MTVEC     => self.mtvec    = val,
            CSR_MSCRATCH  => self.mscratch = val,
            CSR_MEPC      => self.mepc     = val & !1, // IALIGN=32
            CSR_MCAUSE    => self.mcause   = val,
            CSR_MTVAL     => self.mtval    = val,
            CSR_MIP       => self.mip      = val,
            // mvendorid/marchid/mimpid/mhartid are read-only
            CSR_MVENDORID | CSR_MARCHID | CSR_MIMPID | CSR_MHARTID => {}
            CSR_PMPCFG0   => self.pmpcfg0 = val,
            CSR_PMPCFG1   => self.pmpcfg1 = val,
            a if (CSR_PMPADDR0..CSR_PMPADDR0 + 8).contains(&a) =>
                self.pmpaddr[(a - CSR_PMPADDR0) as usize] = val,

            // S-mode views: only the masked bits affect underlying storage.
            CSR_SSTATUS => {
                self.mstatus = (self.mstatus & !SSTATUS_MASK) | (val & SSTATUS_MASK);
            }
            CSR_SIE => {
                self.mie = (self.mie & !SI_MASK) | (val & SI_MASK);
            }
            CSR_SIP => {
                self.mip = (self.mip & !SI_MASK) | (val & SI_MASK);
            }
            CSR_STVEC    => self.stvec = val,
            CSR_SSCRATCH => self.sscratch = val,
            CSR_SEPC     => self.sepc = val & !1,
            CSR_SCAUSE   => self.scause = val,
            CSR_STVAL    => self.stval = val,
            CSR_SATP     => self.satp = val,  // Phase 4.1: storage only
            CSR_MEDELEG  => self.medeleg = val,
            CSR_MIDELEG  => self.mideleg = val,

            _ => {}
        }
    }

    /// True if `addr` is a CSR known to this file (so the wasm/JS layer
    /// can iterate the populated set).
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
}
