//! The memory panel's machine: two 5-stage pipeline cores sharing one
//! [`MemSystem`] (their own L1I / L1D, one L2, one physical memory).
//!
//! Both cores start at 0x0 in M-mode; `mhartid` is 0 / 1 so one program
//! can send them different ways. Each cycle both cores step; the order
//! alternates so neither always wins the bus.

use std::cell::RefCell;
use std::rc::Rc;

use super::{MemSystem, N_CORES};
use crate::sim::pipeline::state::Hier;
use crate::sim::pipeline::PipelineState;
use crate::sim::system::addr;

pub struct MemPanelSystem {
    pub cores: Vec<PipelineState>,
    pub mem: Rc<RefCell<MemSystem>>,
    pub cycle: u64,
}

impl Default for MemPanelSystem {
    fn default() -> Self { Self::new() }
}

impl MemPanelSystem {
    pub fn new() -> Self {
        let mem = Rc::new(RefCell::new(MemSystem::new()));
        let cores = (0..N_CORES).map(|i| {
            let mut c = PipelineState::new();
            c.hier = Some(Hier { core: i as u8, sys: mem.clone() });
            c.sys.csrs.set_counter(addr::MHARTID, i as u32);
            c
        }).collect();
        Self { cores, mem, cycle: 0 }
    }

    /// Write words into physical memory at byte address `pa`.
    pub fn load(&mut self, pa: u32, words: &[u32]) { self.mem.borrow_mut().load(pa, words); }

    /// One clock for both cores.
    pub fn step(&mut self) {
        self.mem.borrow_mut().begin_cycle(self.cycle);
        let order: [usize; N_CORES] = if self.cycle % 2 == 0 { [0, 1] } else { [1, 0] };
        for c in order { self.cores[c].step(); }
        self.cycle += 1;
    }

    /// Memory word as the system sees it (caches included).
    pub fn peek(&self, pa: u32) -> u32 { self.mem.borrow().peek(pa) }
}
