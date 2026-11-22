//! Direct-mapped 4 KiB L1 cache, 16-byte lines, single-port.
//!
//! Geometry:
//!   sets:        256
//!   ways:        1   (direct-mapped)
//!   line size:   16 bytes (4 words)
//!   total size:  4 KiB
//!
//! Address breakdown:
//!   bits[31:12]   = tag       (20 bits)
//!   bits[11:4]    = set index (8 bits)
//!   bits[3:2]     = word offset within line
//!   bits[1:0]     = byte offset (always 0 for word fetches)

use super::state::N_THREADS;
use crate::sim::mem::cache::{Cache as Array, Line};

pub const CACHE_SETS: usize = 256;
pub const CACHE_WAYS: usize = 1;
pub const CACHE_LINE_BYTES: u32 = crate::sim::mem::cache::LINE_BYTES;
pub const CACHE_LINE_WORDS: usize = crate::sim::mem::cache::LINE_WORDS;
pub const CACHE_LINE_MASK: u32 = !(CACHE_LINE_BYTES - 1);

/// One outstanding miss being serviced by the bus.
#[derive(Debug, Clone, Copy)]
pub struct PendingMiss {
    pub line_addr: u32,   // line-aligned
    pub thread: u8,       // primary issuer (also recorded in waiters)
}

/// An L1 controller: the line array (shared cache type, valid-bit lines)
/// plus the barrel's miss handling and hit / miss counters.
#[derive(Debug, Clone)]
pub struct Cache {
    pub name: &'static str,
    pub array: Array<bool>,
    /// One primary miss in flight (B4 v1: no MSHR queue).
    pub pending: Option<PendingMiss>,
    /// Threads currently stalled because they hit either the
    /// primary miss or a "cache busy, retry next cycle" condition.
    /// Cleared when the primary miss completes; those threads will
    /// re-issue on their next dispatch.
    pub waiters: [bool; N_THREADS],
    pub hits: u64,
    pub misses: u64,
}

impl Cache {
    pub fn new(name: &'static str) -> Self {
        Self { name, array: Array::new(CACHE_SETS, CACHE_WAYS), pending: None, waiters: [false; N_THREADS], hits: 0, misses: 0 }
    }

    pub fn reset(&mut self) {
        self.array.clear();
        self.pending = None;
        self.waiters = [false; N_THREADS];
        self.hits = 0;
        self.misses = 0;
    }

    /// Address breakdown helpers.
    pub fn line_addr(addr: u32) -> u32 { addr & CACHE_LINE_MASK }
    pub fn set_index(addr: u32) -> usize { ((addr >> 4) & 0xff) as usize }
    pub fn line_tag(addr: u32) -> u32 { addr >> 12 }
    pub fn word_offset(addr: u32) -> usize { Array::<bool>::word_offset(addr) }

    /// Look up `addr` in the cache. Returns `Some(word)` on hit;
    /// `None` on miss. Updates hit/miss stats.
    pub fn lookup(&mut self, addr: u32) -> Option<u32> {
        let w = self.array.peek(addr);
        if w.is_some() { self.hits += 1 } else { self.misses += 1 }
        w
    }

    /// Install a freshly-fetched line into the cache. Caller is
    /// responsible for having read the 4 words from external memory.
    pub fn install(&mut self, addr: u32, data: [u32; CACHE_LINE_WORDS]) {
        self.array.fill(addr, data, true);
    }

    /// The line holding `addr`, if present (no stat update) — used by
    /// store-hit handling to update the cached copy.
    pub fn peek_line_mut(&mut self, addr: u32) -> Option<&mut Line<bool>> {
        let (s, w) = self.array.find(addr)?;
        Some(self.array.line_mut(s, w))
    }
}
