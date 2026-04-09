//! IRQ controller with routing table.
//!
//! Maps 8 IRQ lines to handler thread slots. When a line is
//! asserted (by hardware or by `inject_irq`), the controller
//! activates the routed slot.
//!
//! MMIO registers at `IRQ_BASE` (0x4000_2000):
//!   +0x00..0x1C  IRQ_ROUTE[0..7]  — which slot handles line N (0xFF = disabled)
//!   +0x20        IRQ_PENDING      — bit N = line N asserted (read-only from sw)
//!   +0x24        IRQ_ACK          — write bit N = clear line N

use super::state::{N_THREADS, Thread};

pub const IRQ_BASE: u32      = 0x4000_2000;
pub const IRQ_ROUTE_BASE: u32= IRQ_BASE;          // +0x00..0x1C
pub const IRQ_PENDING: u32   = IRQ_BASE + 0x20;
pub const IRQ_ACK: u32       = IRQ_BASE + 0x24;
pub const IRQ_END: u32       = IRQ_BASE + 0x28;
pub const N_IRQ_LINES: usize = 8;

#[derive(Debug, Clone)]
pub struct IrqController {
    /// IRQ line N → handler slot (0..7, or 0xFF = disabled).
    pub route: [u8; N_IRQ_LINES],
    /// Bitfield of asserted IRQ lines.
    pub pending: u8,
}

impl Default for IrqController {
    fn default() -> Self {
        Self {
            route: [0xFF; N_IRQ_LINES],
            pending: 0,
        }
    }
}

impl IrqController {
    pub fn new() -> Self { Self::default() }

    /// Assert IRQ line `line` (0..7). The controller will activate
    /// the routed slot on the next tick.
    pub fn assert_line(&mut self, line: u8) {
        if (line as usize) < N_IRQ_LINES {
            self.pending |= 1 << line;
        }
    }

    /// Tick: for each pending+routed line, activate the target
    /// slot. Called at the start of step().
    pub fn tick(&self, threads: &mut [Thread; N_THREADS]) {
        for line in 0..N_IRQ_LINES {
            if self.pending & (1 << line) == 0 { continue; }
            let slot = self.route[line] as usize;
            if slot < N_THREADS && !threads[slot].active {
                threads[slot].active = true;
            }
        }
    }

    /// Handle a store to an IRQ register. Returns true if addr is
    /// in the IRQ MMIO range.
    pub fn write_reg(&mut self, addr: u32, val: u32) -> bool {
        if addr < IRQ_BASE || addr >= IRQ_END { return false; }
        if addr >= IRQ_ROUTE_BASE && addr < IRQ_ROUTE_BASE + (N_IRQ_LINES as u32) * 4 {
            let idx = ((addr - IRQ_ROUTE_BASE) >> 2) as usize;
            if idx < N_IRQ_LINES { self.route[idx] = val as u8; }
        } else if addr == IRQ_ACK {
            self.pending &= !(val as u8);
        }
        // IRQ_PENDING is read-only from software.
        true
    }

    /// Handle a load from an IRQ register.
    pub fn read_reg(&self, addr: u32) -> Option<u32> {
        if addr < IRQ_BASE || addr >= IRQ_END { return None; }
        if addr >= IRQ_ROUTE_BASE && addr < IRQ_ROUTE_BASE + (N_IRQ_LINES as u32) * 4 {
            let idx = ((addr - IRQ_ROUTE_BASE) >> 2) as usize;
            return Some(self.route.get(idx).copied().unwrap_or(0xFF) as u32);
        }
        if addr == IRQ_PENDING { return Some(self.pending as u32); }
        Some(0)
    }
}
