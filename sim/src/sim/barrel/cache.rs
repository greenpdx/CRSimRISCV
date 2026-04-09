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

pub const CACHE_SETS: usize = 256;
pub const CACHE_WAYS: usize = 1;
pub const CACHE_LINE_BYTES: u32 = 16;
pub const CACHE_LINE_WORDS: usize = 4;
pub const CACHE_LINE_MASK: u32 = !(CACHE_LINE_BYTES - 1);

#[derive(Debug, Clone, Copy, Default)]
pub struct CacheLine {
    pub valid: bool,
    pub tag: u32,
    pub data: [u32; CACHE_LINE_WORDS],
}

/// One outstanding miss being serviced by the bus.
#[derive(Debug, Clone, Copy)]
pub struct PendingMiss {
    pub line_addr: u32,   // line-aligned
    pub thread: u8,       // primary issuer (also recorded in waiters)
}

#[derive(Debug, Clone)]
pub struct Cache {
    pub name: &'static str,
    pub lines: Box<[CacheLine; CACHE_SETS]>,
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
        Self {
            name,
            lines: Box::new([CacheLine::default(); CACHE_SETS]),
            pending: None,
            waiters: [false; N_THREADS],
            hits: 0,
            misses: 0,
        }
    }

    pub fn reset(&mut self) {
        for l in self.lines.iter_mut() { *l = CacheLine::default(); }
        self.pending = None;
        self.waiters = [false; N_THREADS];
        self.hits = 0;
        self.misses = 0;
    }

    /// Address breakdown helpers.
    pub fn line_addr(addr: u32) -> u32 { addr & CACHE_LINE_MASK }
    pub fn set_index(addr: u32) -> usize {
        ((addr >> 4) & 0xff) as usize
    }
    pub fn line_tag(addr: u32) -> u32 { addr >> 12 }
    pub fn word_offset(addr: u32) -> usize {
        ((addr >> 2) & 0x3) as usize
    }

    /// Look up `addr` in the cache. Returns `Some(word)` on hit;
    /// `None` on miss. Updates hit/miss stats.
    pub fn lookup(&mut self, addr: u32) -> Option<u32> {
        let idx = Self::set_index(addr);
        let tag = Self::line_tag(addr);
        let line = &self.lines[idx];
        if line.valid && line.tag == tag {
            self.hits += 1;
            Some(line.data[Self::word_offset(addr)])
        } else {
            self.misses += 1;
            None
        }
    }

    /// Install a freshly-fetched line into the cache. Caller is
    /// responsible for having read the 4 words from external memory.
    pub fn install(&mut self, addr: u32, data: [u32; CACHE_LINE_WORDS]) {
        let idx = Self::set_index(addr);
        let tag = Self::line_tag(addr);
        self.lines[idx] = CacheLine { valid: true, tag, data };
    }

    /// Look up a line directly (no stat update) — used by store
    /// hit handling to update the cached copy without changing
    /// hit/miss counters.
    pub fn peek_line_mut(&mut self, addr: u32) -> Option<&mut CacheLine> {
        let idx = Self::set_index(addr);
        let tag = Self::line_tag(addr);
        let l = &mut self.lines[idx];
        if l.valid && l.tag == tag { Some(l) } else { None }
    }
}
