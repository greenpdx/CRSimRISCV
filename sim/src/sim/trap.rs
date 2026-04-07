//! Privilege levels, CSR file, trap handling for the visualizer.
//!
//! Models the M and S privilege modes with M-↔-S delegation through
//! `mideleg`/`medeleg`. Both asynchronous interrupts (MTI/MEI/MSI/STI/SEI/SSI)
//! and synchronous exceptions (illegal instruction, ecall, ebreak) take the
//! same trap path.

use std::collections::BTreeMap;

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
    // M-mode
    pub const MSTATUS: u16 = 0x300;
    pub const MEDELEG: u16 = 0x302;
    pub const MIDELEG: u16 = 0x303;
    pub const MIE:     u16 = 0x304;
    pub const MTVEC:   u16 = 0x305;
    pub const MSCRATCH:u16 = 0x340;
    pub const MEPC:    u16 = 0x341;
    pub const MCAUSE:  u16 = 0x342;
    pub const MTVAL:   u16 = 0x343;
    pub const MIP:     u16 = 0x344;
    pub const MHARTID: u16 = 0xF14;
}

// ── mstatus / sstatus bit positions ────────────────────────────────────────
// We model the subset relevant to traps:
//   SIE  bit 1
//   MIE  bit 3
//   SPIE bit 5
//   MPIE bit 7
//   SPP  bit 8       (1 bit)
//   MPP  bits[12:11] (2 bits)
const M_MSTATUS_SIE:  u32 = 1 << 1;
const M_MSTATUS_MIE:  u32 = 1 << 3;
const M_MSTATUS_SPIE: u32 = 1 << 5;
const M_MSTATUS_MPIE: u32 = 1 << 7;
const M_MSTATUS_SPP:  u32 = 1 << 8;
const M_MSTATUS_MPP_SHIFT: u32 = 11;
const M_MSTATUS_MPP_MASK:  u32 = 0x3 << M_MSTATUS_MPP_SHIFT;

// sstatus is a *view* of mstatus restricted to the S-visible bits.
// Bits SIE, SPIE, SPP are visible; UPIE etc. would be too in a full impl.
const SSTATUS_VIEW_MASK: u32 =
    M_MSTATUS_SIE | M_MSTATUS_SPIE | M_MSTATUS_SPP;

// ── Interrupt-pending / enable bit layout (mip/mie) ────────────────────────
// User-Software   bit 0  (USI — not modeled, bit reserved)
// Supervisor-Soft bit 1  (SSI)
// Machine-Soft    bit 3  (MSI)
// Supervisor-Tmr  bit 5  (STI)
// Machine-Tmr     bit 7  (MTI)
// Supervisor-Ext  bit 9  (SEI)
// Machine-Ext     bit 11 (MEI)
pub const IRQ_SSI: u32 = 1 << 1;
pub const IRQ_MSI: u32 = 1 << 3;
pub const IRQ_STI: u32 = 1 << 5;
pub const IRQ_MTI: u32 = 1 << 7;
pub const IRQ_SEI: u32 = 1 << 9;
pub const IRQ_MEI: u32 = 1 << 11;

// sie/sip view: only the S-visible bits
const SIRQ_VIEW_MASK: u32 = IRQ_SSI | IRQ_STI | IRQ_SEI;

// Synchronous exception cause codes (mcause without the interrupt bit)
pub const EXC_INSTR_ADDR_MISALIGNED: u32 = 0;
pub const EXC_INSTR_ACCESS_FAULT:    u32 = 1;
pub const EXC_ILLEGAL_INSTR:         u32 = 2;
pub const EXC_BREAKPOINT:            u32 = 3;
pub const EXC_LOAD_ADDR_MISALIGNED:  u32 = 4;
pub const EXC_LOAD_ACCESS_FAULT:     u32 = 5;
pub const EXC_STORE_ADDR_MISALIGNED: u32 = 6;
pub const EXC_STORE_ACCESS_FAULT:    u32 = 7;
pub const EXC_ECALL_FROM_U:          u32 = 8;
pub const EXC_ECALL_FROM_S:          u32 = 9;
pub const EXC_ECALL_FROM_M:          u32 = 11;

// Asynchronous interrupt cause codes (mcause with the high bit set)
pub const INT_SSI: u32 = 1;
pub const INT_MSI: u32 = 3;
pub const INT_STI: u32 = 5;
pub const INT_MTI: u32 = 7;
pub const INT_SEI: u32 = 9;
pub const INT_MEI: u32 = 11;

// ── CsrFile ────────────────────────────────────────────────────────────────
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
            addr::MHARTID,
        ] { f.regs.insert(a, 0); }
        f
    }

    pub fn read(&self, addr: u16) -> u32 {
        match addr {
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
            // Read-only CSRs ignored; everything else stored verbatim.
            addr::MHARTID => {}
            _ => { self.regs.insert(addr, val); }
        }
    }

    /// Mutate one bit-field of mstatus directly. Used by trap entry/exit.
    pub fn mstatus_clear_set(&mut self, clear: u32, set: u32) {
        let cur = self.regs.get(&addr::MSTATUS).copied().unwrap_or(0);
        self.regs.insert(addr::MSTATUS, (cur & !clear) | set);
    }

    pub fn raw(&self) -> &BTreeMap<u16, u32> { &self.regs }
}

// ── TrapInfo for the visualizer history ────────────────────────────────────
#[derive(Debug, Clone, Copy)]
pub struct TrapInfo {
    pub cycle: u64,
    pub from_mode: PrivMode,
    pub to_mode: PrivMode,
    pub cause: u32,
    pub is_interrupt: bool,
    pub epc: u32,
    pub tval: u32,
}

// ── Helpers used by step()/cpu_state ───────────────────────────────────────

/// Is interrupt `bit` enabled and pending given the current state?
/// Returns the effective destination mode (M or S) per delegation rules.
pub fn interrupt_destination(
    csrs: &CsrFile,
    cur_mode: PrivMode,
    pending_irq: u32,
) -> Option<(u32, PrivMode)> {
    let mip = csrs.read(addr::MIP) | pending_irq;
    let mie = csrs.read(addr::MIE);
    let mideleg = csrs.read(addr::MIDELEG);
    let mstatus = csrs.read(addr::MSTATUS);

    // Priority order: MEI, MSI, MTI, SEI, SSI, STI (RISC-V spec)
    let priority = [
        (IRQ_MEI, INT_MEI), (IRQ_MSI, INT_MSI), (IRQ_MTI, INT_MTI),
        (IRQ_SEI, INT_SEI), (IRQ_SSI, INT_SSI), (IRQ_STI, INT_STI),
    ];
    for (mask, cause) in priority {
        if (mip & mie & mask) == 0 { continue; }
        // Determine destination mode via delegation
        let delegated = (mideleg & mask) != 0;
        let dest = if delegated { PrivMode::S } else { PrivMode::M };
        // Check global enable for the destination mode
        let enabled = match dest {
            PrivMode::M => match cur_mode {
                PrivMode::M => (mstatus & M_MSTATUS_MIE) != 0,
                _ => true, // higher-priv interrupts always enabled when in lower priv
            }
            PrivMode::S => match cur_mode {
                PrivMode::S => (mstatus & M_MSTATUS_SIE) != 0,
                PrivMode::U => true,
                PrivMode::M => false, // M can't take S-delegated trap
            }
            PrivMode::U => false,
        };
        if enabled { return Some((cause, dest)); }
    }
    None
}

/// Take a trap (interrupt or exception). Returns the new PC and new privilege mode.
/// Updates mstatus / mepc|sepc / mcause|scause / mtval|stval as appropriate.
pub fn take_trap(
    csrs: &mut CsrFile,
    cur_mode: PrivMode,
    cause: u32,
    is_interrupt: bool,
    epc: u32,
    tval: u32,
) -> (u32, PrivMode, TrapInfo) {
    let medeleg = csrs.read(addr::MEDELEG);
    let mideleg = csrs.read(addr::MIDELEG);

    // Should the trap be delegated to S?
    // Delegation only happens when current mode is < M (RISC-V rule).
    let bit_mask = 1u32 << cause.min(31);
    let delegated_to_s = matches!(cur_mode, PrivMode::S | PrivMode::U) &&
        (if is_interrupt { mideleg & bit_mask } else { medeleg & bit_mask }) != 0;

    let dest = if delegated_to_s { PrivMode::S } else { PrivMode::M };

    let mcause_val = if is_interrupt { 0x8000_0000 | cause } else { cause };

    if dest == PrivMode::M {
        let mstatus = csrs.read(addr::MSTATUS);
        let old_mie = (mstatus & M_MSTATUS_MIE) != 0;
        // Save old MIE → MPIE, save old mode → MPP, clear MIE.
        let mpp_bits = (cur_mode as u32 & 0x3) << M_MSTATUS_MPP_SHIFT;
        csrs.mstatus_clear_set(
            M_MSTATUS_MIE | M_MSTATUS_MPIE | M_MSTATUS_MPP_MASK,
            mpp_bits | (if old_mie { M_MSTATUS_MPIE } else { 0 }),
        );
        csrs.write(addr::MEPC,   epc);
        csrs.write(addr::MCAUSE, mcause_val);
        csrs.write(addr::MTVAL,  tval);
        let mtvec = csrs.read(addr::MTVEC);
        let new_pc = trap_vector(mtvec, cause, is_interrupt);
        let info = TrapInfo { cycle: 0, from_mode: cur_mode, to_mode: dest,
            cause, is_interrupt, epc, tval };
        return (new_pc, dest, info);
    }

    // Delegated to S
    let mstatus = csrs.read(addr::MSTATUS);
    let old_sie = (mstatus & M_MSTATUS_SIE) != 0;
    let spp_bit = if matches!(cur_mode, PrivMode::S) { M_MSTATUS_SPP } else { 0 };
    csrs.mstatus_clear_set(
        M_MSTATUS_SIE | M_MSTATUS_SPIE | M_MSTATUS_SPP,
        spp_bit | (if old_sie { M_MSTATUS_SPIE } else { 0 }),
    );
    csrs.write(addr::SEPC,   epc);
    csrs.write(addr::SCAUSE, mcause_val);
    csrs.write(addr::STVAL,  tval);
    let stvec = csrs.read(addr::STVEC);
    let new_pc = trap_vector(stvec, cause, is_interrupt);
    let info = TrapInfo { cycle: 0, from_mode: cur_mode, to_mode: dest,
        cause, is_interrupt, epc, tval };
    (new_pc, dest, info)
}

fn trap_vector(tvec: u32, cause: u32, is_interrupt: bool) -> u32 {
    let base = tvec & !0x3;
    let mode = tvec & 0x3;
    if mode == 1 && is_interrupt {
        // Vectored mode: only for interrupts
        base.wrapping_add(cause * 4)
    } else {
        base
    }
}

/// MRET: restore mstatus.MIE from MPIE, restore mode from MPP, jump to mepc.
/// Returns (new_pc, new_mode).
pub fn mret(csrs: &mut CsrFile) -> (u32, PrivMode) {
    let mstatus = csrs.read(addr::MSTATUS);
    let mpp = ((mstatus & M_MSTATUS_MPP_MASK) >> M_MSTATUS_MPP_SHIFT) as u8;
    let mpie = (mstatus & M_MSTATUS_MPIE) != 0;
    let new_mode = PrivMode::from_bits(mpp);

    csrs.mstatus_clear_set(
        M_MSTATUS_MIE | M_MSTATUS_MPIE | M_MSTATUS_MPP_MASK,
        // restore MIE from MPIE; set MPIE=1; clear MPP to U
        (if mpie { M_MSTATUS_MIE } else { 0 }) | M_MSTATUS_MPIE,
    );
    let epc = csrs.read(addr::MEPC);
    (epc, new_mode)
}

/// SRET: restore mstatus.SIE from SPIE, restore mode from SPP, jump to sepc.
pub fn sret(csrs: &mut CsrFile) -> (u32, PrivMode) {
    let mstatus = csrs.read(addr::MSTATUS);
    let spp = (mstatus & M_MSTATUS_SPP) != 0;
    let spie = (mstatus & M_MSTATUS_SPIE) != 0;
    let new_mode = if spp { PrivMode::S } else { PrivMode::U };

    csrs.mstatus_clear_set(
        M_MSTATUS_SIE | M_MSTATUS_SPIE | M_MSTATUS_SPP,
        (if spie { M_MSTATUS_SIE } else { 0 }) | M_MSTATUS_SPIE,
    );
    let epc = csrs.read(addr::SEPC);
    (epc, new_mode)
}
