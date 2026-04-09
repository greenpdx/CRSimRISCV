//! DMA controller for loading code into per-slot I-SRAMs.
//!
//! Memory-mapped registers at `DMA_BASE` (0x4000_1000):
//!   +0x00  DMA_SRC       — source byte address in ext_imem
//!   +0x04  DMA_DST_SLOT  — destination I-SRAM slot (0/1/2)
//!   +0x08  DMA_COUNT     — number of 32-bit words to copy
//!   +0x0C  DMA_TRIGGER   — write any value to start the copy
//!   +0x10  DMA_STATUS    — read: 0 = idle, 1 = busy
//!
//! For the simulator, DMA completes **instantly** when TRIGGER is
//! written: all words are copied from ext_imem to local_isram in
//! the same cycle. A multi-cycle background model can be added
//! later without changing the register interface.

use super::state::{ISRAM_N_SLOTS, ISRAM_WORDS_PER_SLOT};

pub const DMA_BASE: u32   = 0x4000_1000;
pub const DMA_SRC: u32    = DMA_BASE + 0x00;
pub const DMA_DST: u32    = DMA_BASE + 0x04;
pub const DMA_COUNT: u32  = DMA_BASE + 0x08;
pub const DMA_TRIGGER: u32= DMA_BASE + 0x0C;
pub const DMA_STATUS: u32 = DMA_BASE + 0x10;
pub const DMA_END: u32    = DMA_BASE + 0x14;

/// DMA controller state. Lives inside BarrelState.
#[derive(Debug, Clone, Default)]
pub struct DmaController {
    pub src: u32,       // source byte address in ext_imem
    pub dst_slot: u32,  // destination I-SRAM slot (0/1/2)
    pub count: u32,     // word count
    pub busy: bool,     // true while a transfer is in progress
    pub transfers: u64,  // total completed transfers (for stats)
}

impl DmaController {
    pub fn new() -> Self { Self::default() }

    /// Handle a store to a DMA register. Returns true if the
    /// address was in the DMA range (so the caller knows not to
    /// also write to the D-cache or ext_dmem).
    pub fn write_reg(&mut self, addr: u32, val: u32, state_imem: &[u32],
                     isram: &mut [Box<[u32; ISRAM_WORDS_PER_SLOT]>; ISRAM_N_SLOTS]) -> bool {
        if addr < DMA_BASE || addr >= DMA_END { return false; }
        match addr {
            DMA_SRC     => { self.src = val; }
            DMA_DST     => { self.dst_slot = val; }
            DMA_COUNT   => { self.count = val; }
            DMA_TRIGGER => {
                // Instant copy.
                let slot = self.dst_slot as usize;
                if slot < ISRAM_N_SLOTS {
                    let src_word = (self.src >> 2) as usize;
                    let n = (self.count as usize).min(ISRAM_WORDS_PER_SLOT);
                    for i in 0..n {
                        let w = state_imem.get(src_word + i).copied().unwrap_or(0x0000_0013);
                        isram[slot][i] = w;
                    }
                    self.transfers += 1;
                }
                self.busy = false;
            }
            DMA_STATUS  => { /* read-only, ignore writes */ }
            _           => {}
        }
        true
    }

    /// Handle a load from a DMA register. Returns `Some(value)`
    /// if the address is in the DMA range.
    pub fn read_reg(&self, addr: u32) -> Option<u32> {
        if addr < DMA_BASE || addr >= DMA_END { return None; }
        Some(match addr {
            DMA_SRC    => self.src,
            DMA_DST    => self.dst_slot,
            DMA_COUNT  => self.count,
            DMA_STATUS => if self.busy { 1 } else { 0 },
            _          => 0,
        })
    }
}
