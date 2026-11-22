//! Memory interfaces shared by the cores.
//!
//! - [`Bus`]: a core's memory as seen from its datapath (`read` / `write`,
//!   with the [`Port`] that asks) and from the outside (`peek` / `poke`: no
//!   timing, no side effects — the panels, tests and program loading).
//! - [`Pages`]: physical RAM of any size, sparse 4 KiB pages.

/// Who is accessing memory.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Port { Fetch, Load, Store, Walk }

/// A core's memory. Addresses are byte addresses; every access is the
/// aligned word holding `addr`.
pub trait Bus {
    /// Word for the datapath (may update timing, statistics, caches).
    fn read(&mut self, addr: u32, port: Port) -> u32;
    /// Word write from the datapath.
    fn write(&mut self, addr: u32, word: u32, port: Port);
    /// Word as the program would see it, with no side effects.
    fn peek(&self, addr: u32) -> u32;
    /// Set a word directly (program loading, panels, tests).
    fn poke(&mut self, addr: u32, word: u32);

    /// Store `wdata` into the byte lanes `bw` of the word holding `addr`.
    fn write_lanes(&mut self, addr: u32, wdata: u32, bw: u8, port: Port) {
        let m = crate::isa::lsu::byte_mask(bw);
        let old = self.peek(addr);
        self.write(addr, (old & !m) | (wdata & m), port);
    }
    /// `words` from `addr` on, as `peek` sees them.
    fn peek_words(&self, addr: u32, n: u32) -> Vec<u32> {
        (0..n).map(|i| self.peek(addr.wrapping_add(4 * i))).collect()
    }
}

/// Physical RAM of any size: sparse 4 KiB pages, made on first write;
/// words never written read 0.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Default)]
pub struct Pages { pages: std::collections::HashMap<u32, Vec<u32>> }

impl Pages {
    pub fn get(&self, pa: u32) -> u32 { self.pages.get(&(pa >> 12)).map_or(0, |p| p[((pa >> 2) & 0x3ff) as usize]) }
    pub fn set(&mut self, pa: u32, v: u32) { self.pages.entry(pa >> 12).or_insert_with(|| vec![0; 1024])[((pa >> 2) & 0x3ff) as usize] = v; }
}
