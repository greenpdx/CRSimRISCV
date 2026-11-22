//! System unit shared by all cores: privilege modes (M / S / U), CSR file,
//! Zicsr execution, trap entry / return and interrupt sampling.
//!
//! A core owns one [`SysUnit`] and calls it at fixed points:
//! - instruction boundary: [`SysUnit::sample_irq`] → [`SysUnit::trap`]
//! - CSR instruction: [`SysUnit::csr`]
//! - ECALL / EBREAK / illegal / misaligned: [`SysUnit::ecall_cause`] + [`SysUnit::trap`]
//! - MRET / SRET: [`SysUnit::xret`]
//!
//! Future memory-side units (cache, MMU) live beside this under `sim/mem`.

pub mod csr;
pub mod irq;
pub mod trap;

pub use csr::{CsrFile, PrivMode};
use crate::isa::csr::{addr, csr_exec, CsrResult};
pub use irq::*;
pub use trap::*;

#[cfg_attr(feature = "wasm", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct SysUnit {
    pub csrs: CsrFile,
    pub priv_mode: PrivMode,
    /// External IRQ lines, ORed into mip when sampled (level-triggered).
    pub pending_irq: u32,
    pub last_trap: Option<TrapInfo>,
    /// Traps entered minus MRET / SRET taken ("inside a handler" indicator).
    pub trap_depth: u32,
}

impl Default for SysUnit {
    fn default() -> Self { Self::new() }
}

impl SysUnit {
    pub fn new() -> Self {
        Self { csrs: CsrFile::new(), priv_mode: PrivMode::M, pending_irq: 0, last_trap: None, trap_depth: 0 }
    }

    pub fn raise_irq(&mut self, mask: u32) { self.pending_irq |= mask; }

    pub fn clear_irq(&mut self) {
        self.pending_irq = 0;
        self.csrs.write(addr::MIP, 0);
    }

    /// Merge the external lines into mip; return the interrupt cause that
    /// should be taken now (highest priority, enabled, globally on), if any.
    pub fn sample_irq(&mut self) -> Option<u32> {
        let mip = self.csrs.read(addr::MIP) | self.pending_irq;
        self.csrs.write(addr::MIP, mip);
        interrupt_destination(&self.csrs, self.priv_mode, 0).map(|(cause, _)| cause)
    }

    /// Would an interrupt be taken at the next boundary? (no state change)
    pub fn irq_would_take(&self) -> Option<u32> {
        interrupt_destination(&self.csrs, self.priv_mode, self.pending_irq).map(|(c, _)| c)
    }

    /// Execute a CSR instruction: returns the old value (for rd).
    pub fn csr(&mut self, csr_addr: u16, funct3: u8, rs1: u8, rs1_val: u32) -> CsrResult {
        let r = csr_exec(self.csrs.read(csr_addr), funct3, rs1, rs1_val);
        if r.write { self.csrs.write(csr_addr, r.new); }
        r
    }

    /// Enter a trap; returns the handler PC.
    pub fn trap(&mut self, cause: u32, is_interrupt: bool, epc: u32, tval: u32, cycle: u64) -> u32 {
        let (new_pc, new_mode, mut info) =
            take_trap(&mut self.csrs, self.priv_mode, cause, is_interrupt, epc, tval);
        info.cycle = cycle;
        self.last_trap = Some(info);
        self.priv_mode = new_mode;
        self.trap_depth = self.trap_depth.saturating_add(1);
        new_pc
    }

    /// Trap vector the next trap with this cause would use (no state change).
    pub fn vector_for(&self, cause: u32, is_interrupt: bool) -> u32 {
        let mut c = self.csrs.clone();
        take_trap(&mut c, self.priv_mode, cause, is_interrupt, 0, 0).0
    }

    /// MRET (`sret = false`) or SRET: restore status / mode; returns the return PC.
    pub fn xret(&mut self, sret_: bool) -> u32 {
        let (epc, mode) = if sret_ { sret(&mut self.csrs) } else { mret(&mut self.csrs) };
        self.priv_mode = mode;
        self.trap_depth = self.trap_depth.saturating_sub(1);
        epc
    }

    /// Return PC MRET / SRET would use (no state change).
    pub fn xret_target(&self, sret_: bool) -> u32 {
        self.csrs.read(if sret_ { addr::SEPC } else { addr::MEPC })
    }

    /// ECALL cause for the current privilege mode.
    pub fn ecall_cause(&self) -> u32 {
        match self.priv_mode {
            PrivMode::U => EXC_ECALL_FROM_U,
            PrivMode::S => EXC_ECALL_FROM_S,
            PrivMode::M => EXC_ECALL_FROM_M,
        }
    }

    pub fn in_trap(&self) -> bool { self.trap_depth > 0 }

    pub fn snapshot(&self) -> Vec<(u16, u32)> {
        self.csrs.raw().iter().map(|(&a, &v)| (a, v)).collect()
    }
}
