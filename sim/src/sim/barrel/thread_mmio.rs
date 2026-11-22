//! MMIO interface for cross-thread register and CSR access.
//!
//! Mapped at `THREAD_MMIO_BASE` (0x4000_4000). Only accessible
//! from thread 0 in M-mode. Other threads/modes get an access
//! fault (the caller in stage_ma must check).
//!
//! Layout per slot (0x100 bytes each):
//!   +0x000..0x07C  regs[0..31]   (32 × 4 = 0x80 bytes)
//!   +0x080         mstatus
//!   +0x084         mepc
//!   +0x088         mcause
//!   +0x08C         mtval
//!   +0x090         mscratch
//!   +0x094         satp
//!   +0x098         priv_mode     (read: u8 as u32; write: low 2 bits)
//!   +0x09C         pc            (read/write)
//!
//! Total: 8 slots × 0x100 = 0x800 bytes.

use crate::isa::csr::addr;
use crate::sim::system::PrivMode;
use super::state::{BarrelState, N_THREADS};

pub const TMMIO_BASE: u32 = 0x4000_4000;
pub const TMMIO_SLOT_SIZE: u32 = 0x100;
pub const TMMIO_END: u32 = TMMIO_BASE + (N_THREADS as u32) * TMMIO_SLOT_SIZE;

pub fn is_tmmio(addr: u32) -> bool {
    addr >= TMMIO_BASE && addr < TMMIO_END
}

/// Check access permission. Returns true if allowed.
pub fn check_access(thread: u8, priv_mode: u8) -> bool {
    thread == 0 && priv_mode == 3
}

pub fn read(state: &BarrelState, addr: u32) -> u32 {
    let off = addr - TMMIO_BASE;
    let slot = (off / TMMIO_SLOT_SIZE) as usize;
    let field = off % TMMIO_SLOT_SIZE;
    if slot >= N_THREADS { return 0; }
    let t = &state.threads[slot];
    if field < 0x80 {
        let reg = (field >> 2) as usize;
        if reg < 32 { t.regs[reg] } else { 0 }
    } else {
        match field {
            0x80 => t.sys.csrs.read(addr::MSTATUS),
            0x84 => t.sys.csrs.read(addr::MEPC),
            0x88 => t.sys.csrs.read(addr::MCAUSE),
            0x8C => t.sys.csrs.read(addr::MTVAL),
            0x90 => t.sys.csrs.read(addr::MSCRATCH),
            0x94 => t.sys.csrs.read(addr::SATP),
            0x98 => t.priv_bits() as u32,
            0x9C => t.pc,
            _ => 0,
        }
    }
}

pub fn write(state: &mut BarrelState, addr: u32, val: u32) {
    let off = addr - TMMIO_BASE;
    let slot = (off / TMMIO_SLOT_SIZE) as usize;
    let field = off % TMMIO_SLOT_SIZE;
    if slot >= N_THREADS { return; }
    let t = &mut state.threads[slot];
    if field < 0x80 {
        let reg = (field >> 2) as usize;
        if reg > 0 && reg < 32 { t.regs[reg] = val; }
    } else {
        match field {
            0x80 => t.sys.csrs.write(addr::MSTATUS, val),
            0x84 => t.sys.csrs.write(addr::MEPC, val),
            0x88 => t.sys.csrs.write(addr::MCAUSE, val),
            0x8C => t.sys.csrs.write(addr::MTVAL, val),
            0x90 => t.sys.csrs.write(addr::MSCRATCH, val),
            0x94 => t.sys.csrs.write(addr::SATP, val),
            0x98 => t.sys.priv_mode = PrivMode::from_bits((val & 0x3) as u8),
            0x9C => t.pc       = val,
            _ => {}
        }
    }
}
