//! Trap entry (exceptions and interrupts), MRET / SRET.

use super::csr::*;
use crate::isa::csr::addr;

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

// ── TrapInfo for the visualizer history ────────────────────────────────────
#[cfg_attr(feature = "wasm", derive(serde::Serialize, serde::Deserialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
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
