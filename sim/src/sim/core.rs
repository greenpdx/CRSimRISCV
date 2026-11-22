//! The interface every core offers to the outside (panels, wasm API, tests).
//!
//! A core implements the required methods; reset, loading, stepping by
//! instruction, register / CSR access and interrupt lines are then written
//! once here. The smallest step differs per core: a micro-phase
//! (single-cycle), a clock (pipeline, barrel) or one stage of a
//! clock (out-of-order).
//!
//! The barrel processor has eight harts; this interface shows hart 0 (the
//! boot thread). Its other threads are reached through `BarrelState`.

use crate::sim::mem::Bus;
use crate::sim::system::{SysUnit, TrapInfo};

/// What one `step` did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Boundary {
    /// Instructions that completed in this step.
    pub retired: u32,
    pub halted: bool,
}

/// A memory region the panels list (base byte address, size in words).
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub name: &'static str,
    pub base: u32,
    pub words: u32,
}

pub trait Core {
    fn name(&self) -> &'static str;
    /// Core registers to reset values; memory contents are kept.
    fn reset(&mut self);
    /// The smallest step of this core.
    fn step(&mut self) -> Boundary;
    /// Program counter (of the next instruction to fetch).
    fn pc(&self) -> u32;
    /// Clocks (micro-phase cores: instructions) completed.
    fn cycle(&self) -> u64;
    fn regs(&self) -> &[u32; 32];
    fn regs_mut(&mut self) -> &mut [u32; 32];
    fn sys(&self) -> &SysUnit;
    fn sys_mut(&mut self) -> &mut SysUnit;
    fn bus(&self) -> &dyn Bus;
    fn bus_mut(&mut self) -> &mut dyn Bus;
    /// Stopped for good (a `j .` spin where the core detects one).
    fn halted(&self) -> bool { false }
    /// The memory regions a panel shows, read with `bus().peek_words`.
    fn regions(&self) -> Vec<Region> { Vec::new() }

    // ── Provided ────────────────────────────────────────────────────────

    /// Words into memory from `base` (no timing).
    fn load(&mut self, base: u32, words: &[u32]) {
        for (i, &w) in words.iter().enumerate() { self.bus_mut().poke(base.wrapping_add(4 * i as u32), w); }
    }
    /// Step until an instruction completes; at most `max` steps. True if one did.
    fn step_instr(&mut self, max: u64) -> bool {
        for _ in 0..max {
            let b = self.step();
            if b.retired > 0 { return true; }
            if b.halted { return false; }
        }
        false
    }
    /// Up to `steps` steps, stopping at a halt; instructions completed.
    fn run(&mut self, steps: u64) -> u64 {
        let mut n = 0;
        for _ in 0..steps {
            let b = self.step();
            n += b.retired as u64;
            if b.halted { break; }
        }
        n
    }
    fn reg(&self, i: u8) -> u32 { self.regs()[(i & 31) as usize] }
    /// Write register `i` (x0 stays 0).
    fn poke_reg(&mut self, i: u8, v: u32) {
        if i & 31 != 0 { self.regs_mut()[(i & 31) as usize] = v; }
    }
    /// Raise external interrupt lines (level, until `clear_irq`).
    fn raise_irq(&mut self, mask: u32) { self.sys_mut().raise_irq(mask) }
    fn clear_irq(&mut self) { self.sys_mut().clear_irq() }
    fn csr_read(&self, addr: u16) -> u32 { self.sys().csrs.read(addr) }
    fn csr_write(&mut self, addr: u16, val: u32) { self.sys_mut().csrs.write(addr, val) }
    /// Every CSR the core has, (address, value).
    fn csr_snapshot(&self) -> Vec<(u16, u32)> { self.sys().snapshot() }
    fn last_trap(&self) -> Option<TrapInfo> { self.sys().last_trap }
}

// ── The cores ───────────────────────────────────────────────────────────────

use crate::sim::barrel::BarrelState;
use crate::sim::cpu_state::CpuState;
use crate::sim::ooo::OooCpu;
use crate::sim::phase::Phase;
use crate::sim::pipeline::PipelineState;

impl Core for CpuState {
    fn name(&self) -> &'static str { "single-cycle" }
    fn halted(&self) -> bool { self.halted }
    fn regions(&self) -> Vec<Region> {
        vec![Region { name: "rom", base: 0, words: self.mem.rom.len() as u32 },
             Region { name: "ram", base: crate::sim::cpu_state::RAM_BASE, words: self.mem.ram.len() as u32 }]
    }
    fn reset(&mut self) {
        let mem = std::mem::take(&mut self.mem);
        *self = CpuState { mem, traps_enabled: self.traps_enabled, zmmul: self.zmmul, ..CpuState::default() };
    }
    /// One micro-phase; an instruction completes at PcUpdate.
    fn step(&mut self) -> Boundary {
        if self.halted { return Boundary { retired: 0, halted: true }; }
        let p = self.micro_step();
        Boundary { retired: (p == Phase::PcUpdate) as u32, halted: self.halted }
    }
    fn pc(&self) -> u32 { self.pc }
    fn cycle(&self) -> u64 { self.cycle_count }
    fn regs(&self) -> &[u32; 32] { &self.registers }
    fn regs_mut(&mut self) -> &mut [u32; 32] { &mut self.registers }
    fn sys(&self) -> &SysUnit { &self.sys }
    fn sys_mut(&mut self) -> &mut SysUnit { &mut self.sys }
    fn bus(&self) -> &dyn Bus { &self.mem }
    fn bus_mut(&mut self) -> &mut dyn Bus { &mut self.mem }
}

impl Core for PipelineState {
    fn name(&self) -> &'static str { "pipeline" }
    fn halted(&self) -> bool { self.halted }
    /// Flat model: ROM and machine RAM (user RAM is not on the outside bus).
    fn regions(&self) -> Vec<Region> {
        use crate::sim::pipeline::mem::{RAM_BASE, ROM_BASE};
        match self.mem.flat() {
            Some(f) => vec![Region { name: "rom", base: ROM_BASE, words: f.rom.len() as u32 },
                            Region { name: "ram", base: RAM_BASE, words: f.machine_ram.len() as u32 }],
            None => Vec::new(),
        }
    }
    fn reset(&mut self) { PipelineState::reset(self) }
    /// One clock; an instruction completes when it leaves WB.
    fn step(&mut self) -> Boundary {
        PipelineState::step(self);
        Boundary { retired: self.last_wb.valid as u32, halted: self.halted }
    }
    fn pc(&self) -> u32 { self.pc }
    fn cycle(&self) -> u64 { self.cycle_count }
    fn regs(&self) -> &[u32; 32] { &self.registers }
    fn regs_mut(&mut self) -> &mut [u32; 32] { &mut self.registers }
    fn sys(&self) -> &SysUnit { &self.sys }
    fn sys_mut(&mut self) -> &mut SysUnit { &mut self.sys }
    fn bus(&self) -> &dyn Bus { &self.mem }
    fn bus_mut(&mut self) -> &mut dyn Bus { &mut self.mem }
}

/// Hart 0's view (registers, CSRs); interrupt lines reach every thread.
impl Core for BarrelState {
    fn name(&self) -> &'static str { "barrel" }
    fn halted(&self) -> bool { self.halted }
    fn regions(&self) -> Vec<Region> {
        vec![Region { name: "imem", base: 0, words: self.mem.imem.len() as u32 },
             Region { name: "dmem", base: crate::sim::barrel::state::EXT_DMEM_BASE, words: self.mem.dmem.len() as u32 }]
    }
    fn reset(&mut self) { BarrelState::reset(self) }
    /// One clock; an instruction (of any thread) completes when it leaves WB.
    fn step(&mut self) -> Boundary {
        BarrelState::step(self);
        Boundary { retired: self.last_wb.valid as u32, halted: self.halted }
    }
    fn pc(&self) -> u32 { self.threads[0].pc }
    fn cycle(&self) -> u64 { self.cycle_count }
    fn regs(&self) -> &[u32; 32] { &self.threads[0].regs }
    fn regs_mut(&mut self) -> &mut [u32; 32] { &mut self.threads[0].regs }
    fn sys(&self) -> &SysUnit { &self.threads[0].sys }
    fn sys_mut(&mut self) -> &mut SysUnit { &mut self.threads[0].sys }
    fn bus(&self) -> &dyn Bus { &self.mem }
    fn bus_mut(&mut self) -> &mut dyn Bus { &mut self.mem }
    fn raise_irq(&mut self, mask: u32) { BarrelState::raise_irq(self, mask) }
    fn clear_irq(&mut self) { BarrelState::clear_irq(self) }
    /// Shared CSRs (mtvec, mie, mip, m*deleg) are one value for all threads.
    fn csr_read(&self, addr: u16) -> u32 { self.read_csr(0, addr) }
    fn csr_write(&mut self, addr: u16, val: u32) { self.write_csr(0, addr, val) }
}

impl Core for OooCpu {
    fn name(&self) -> &'static str { "out-of-order" }
    fn halted(&self) -> bool { self.halted }
    fn reset(&mut self) { OooCpu::reset(self) }
    /// One stage of a clock; instructions complete at Commit.
    fn step(&mut self) -> Boundary {
        let before = self.stats.committed;
        self.step_stage();
        Boundary { retired: (self.stats.committed - before) as u32, halted: self.halted }
    }
    fn pc(&self) -> u32 { self.pc }
    fn cycle(&self) -> u64 { self.cycle }
    fn regs(&self) -> &[u32; 32] { &self.rf }
    fn regs_mut(&mut self) -> &mut [u32; 32] { &mut self.rf }
    fn sys(&self) -> &SysUnit { &self.sys }
    fn sys_mut(&mut self) -> &mut SysUnit { &mut self.sys }
    fn bus(&self) -> &dyn Bus { &self.mem }
    fn bus_mut(&mut self) -> &mut dyn Bus { &mut self.mem }
}
