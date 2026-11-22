//! Memory for the out-of-order core, selectable:
//! - `Fixed`: every instruction fetch and load takes the same number of cycles;
//! - `Caches`: the memory-panel hierarchy (L1I / L1D → L2 → memory) as core 0,
//!   so latency depends on hits and misses.
//!
//! Stores are written at commit (posted: no stall).

use crate::sim::mem::{Bus, Pages, Port};
use crate::sim::memsys::{Event, MemSystem};

#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]

#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemMode { Fixed, Caches }

#[derive(Debug, Clone)]
pub struct OooMem {
    pub mode: MemMode,
    /// Fixed mode: latency of a fetch / load
    pub fixed_latency: u64,
    pub ram: Pages,
    /// Caches mode: the hierarchy (this core is core 0)
    pub caches: Box<MemSystem>,
}

impl OooMem {
    pub fn new(mode: MemMode) -> Self {
        Self { mode, fixed_latency: 2, ram: Pages::default(), caches: Box::new(MemSystem::new()) }
    }

    /// Start of a cycle (caches mode: lets the hierarchy time this cycle's accesses).
    pub fn begin_cycle(&mut self, now: u64) {
        if self.mode == MemMode::Caches {
            self.caches.begin_cycle(now);
            self.caches.core_begin(0);
        }
    }

    pub fn load_words(&mut self, pa: u32, words: &[u32]) {
        for (i, &w) in words.iter().enumerate() { self.ram.set(pa.wrapping_add(4 * i as u32), w); }
        self.caches.load(pa, words);
    }

    /// Word as the program sees it (caches included).
    pub fn peek(&self, pa: u32) -> u32 {
        match self.mode { MemMode::Fixed => self.ram.get(pa & !3), MemMode::Caches => self.caches.peek(pa & !3) }
    }

    fn read_word(&mut self, pa: u32, port: Port) -> (u32, u64) {
        match self.mode {
            MemMode::Fixed => (self.ram.get(pa & !3), self.fixed_latency),
            MemMode::Caches => {
                let v = self.caches.read(0, pa & !3, port);
                let lat = self.caches.events.iter().rev().find_map(|e| match e { Event::Access { latency, .. } => Some(*latency), _ => None }).unwrap_or(1);
                (v, lat.max(1) as u64)
            }
        }
    }

    /// Instruction fetch: (word, cycles).
    pub fn fetch(&mut self, pc: u32) -> (u32, u64) { self.read_word(pc, Port::Fetch) }

    /// Load `size` bytes (sign- or zero-extended): (value, cycles).
    pub fn load(&mut self, pa: u32, size: u8, signed: bool) -> (u32, u64) {
        let (w, lat) = self.read_word(pa, Port::Load);
        (extract(w, pa, size, signed), lat)
    }

    /// Store at commit.
    pub fn store(&mut self, pa: u32, val: u32, size: u8) {
        let old = self.peek(pa);
        let w = merge(old, pa, val, size);
        match self.mode {
            MemMode::Fixed => self.ram.set(pa & !3, w),
            MemMode::Caches => self.caches.write(0, pa & !3, w, Port::Store),
        }
    }
}

/// Word view (outside / tests): `read` goes through the caches in caches
/// mode (its latency is dropped), `write` is a committed store.
impl Bus for OooMem {
    fn read(&mut self, addr: u32, port: Port) -> u32 { self.read_word(addr, port).0 }
    fn write(&mut self, addr: u32, word: u32, _port: Port) { self.store(addr, word, 4) }
    fn peek(&self, addr: u32) -> u32 { OooMem::peek(self, addr) }
    fn poke(&mut self, addr: u32, word: u32) { self.load_words(addr & !3, &[word]) }
}

/// funct3 of a load / store of `size` bytes.
const fn funct3(size: u8, signed: bool) -> u8 {
    match (size, signed) { (1, true) => 0, (2, true) => 1, (1, false) => 4, (2, false) => 5, _ => 2 }
}

/// The `size`-byte value at byte address `pa` inside word `w` (a halfword
/// is taken from its aligned half of the word).
pub fn extract(w: u32, pa: u32, size: u8, signed: bool) -> u32 {
    let pa = if size == 2 { pa & !1 } else { pa };
    crate::isa::lsu::load(w, pa, funct3(size, signed))
}

/// Word `old` with `size` bytes of `val` written at byte address `pa`.
pub fn merge(old: u32, pa: u32, val: u32, size: u8) -> u32 {
    crate::isa::lsu::store_merge(old, val, pa, funct3(size, true))
}
