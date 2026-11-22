//! The barrel processor's memories: external instruction memory at 0,
//! external data memory at `EXT_DMEM_BASE`, the per-slot local instruction
//! scratchpads (I-SRAM) at `ISRAM_BASE`, and the I/O word. Caches, the store
//! buffer and the bus arbiter that sit in front of them are in the core.

use super::state::{EXT_DMEM_BASE, EXT_DMEM_WORDS, EXT_IMEM_WORDS, ISRAM_BASE, ISRAM_BYTES_PER_SLOT,
    ISRAM_N_SLOTS, ISRAM_TOP, ISRAM_WORDS_PER_SLOT};
use crate::sim::mem::{Bus, Port};

const NOP: u32 = 0x0000_0013;

#[derive(Debug, Clone)]
pub struct BarrelMem {
    pub imem: Vec<u32>,
    pub dmem: Vec<u32>,
    pub io_data: u32,
    /// Local 16 KiB instruction scratchpads, one per slot 0/1/2. Single-cycle,
    /// never misses, only fetchable by the owning slot.
    pub isram: [Box<[u32; ISRAM_WORDS_PER_SLOT]>; ISRAM_N_SLOTS],
}

impl Default for BarrelMem {
    fn default() -> Self {
        Self {
            imem: vec![NOP; EXT_IMEM_WORDS],
            dmem: vec![0; EXT_DMEM_WORDS],
            io_data: 0,
            isram: std::array::from_fn(|_| Box::new([NOP; ISRAM_WORDS_PER_SLOT])),
        }
    }
}

impl BarrelMem {
    pub fn imem_read(&self, addr: u32) -> u32 { self.imem.get((addr >> 2) as usize).copied().unwrap_or(0) }

    fn dmem_index(addr: u32) -> usize { ((addr & !3).wrapping_sub(EXT_DMEM_BASE) >> 2) as usize }
    pub fn dmem_read(&self, addr: u32) -> u32 { self.dmem.get(Self::dmem_index(addr)).copied().unwrap_or(0) }
    pub fn dmem_write(&mut self, addr: u32, val: u32) {
        if let Some(w) = self.dmem.get_mut(Self::dmem_index(addr)) { *w = val; }
    }

    /// Word at local byte offset `byte_off` of slot `slot`'s I-SRAM
    /// (out of range: NOP).
    pub fn isram_read(&self, slot: u8, byte_off: u32) -> u32 {
        let idx = (byte_off >> 2) as usize;
        match self.isram.get(slot as usize) {
            Some(s) if idx < ISRAM_WORDS_PER_SLOT => s[idx],
            _ => NOP,
        }
    }

    /// Load words into slot `slot`'s I-SRAM; words past its size are ignored.
    pub fn load_isram(&mut self, slot: u8, words: &[u32]) {
        if let Some(dst) = self.isram.get_mut(slot as usize) {
            for (d, w) in dst.iter_mut().zip(words) { *d = *w; }
        }
    }

    /// `Some(slot)` if `pc` falls in slot N's I-SRAM range (N = 0, 1, 2).
    pub fn isram_slot_of(pc: u32) -> Option<u8> {
        (ISRAM_BASE..ISRAM_TOP).contains(&pc).then(|| ((pc - ISRAM_BASE) / ISRAM_BYTES_PER_SLOT) as u8)
    }
}

/// Outside view of the address map (no caches, no store buffer): I-SRAM,
/// external data memory, external instruction memory below its end.
impl Bus for BarrelMem {
    fn read(&mut self, addr: u32, _port: Port) -> u32 { self.peek(addr) }
    fn write(&mut self, addr: u32, word: u32, _port: Port) { self.poke(addr, word) }
    fn peek(&self, addr: u32) -> u32 {
        if let Some(s) = Self::isram_slot_of(addr) {
            return self.isram_read(s, addr - ISRAM_BASE - s as u32 * ISRAM_BYTES_PER_SLOT);
        }
        if addr.wrapping_sub(EXT_DMEM_BASE) < 4 * self.dmem.len() as u32 { return self.dmem_read(addr); }
        self.imem_read(addr)
    }
    fn poke(&mut self, addr: u32, word: u32) {
        if let Some(s) = Self::isram_slot_of(addr) {
            let i = ((addr - ISRAM_BASE - s as u32 * ISRAM_BYTES_PER_SLOT) >> 2) as usize;
            self.isram[s as usize][i] = word;
        } else if addr.wrapping_sub(EXT_DMEM_BASE) < 4 * self.dmem.len() as u32 {
            self.dmem_write(addr, word);
        } else if let Some(w) = self.imem.get_mut((addr >> 2) as usize) {
            *w = word;
        }
    }
}
