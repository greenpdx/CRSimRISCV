//! Memory hierarchy for the memory panel: two cores, each with an L1I and
//! an L1D, one shared inclusive L2 and one physical memory, with MESI
//! coherence between the L1Ds over a snooping bus.
//!
//! - Lines are 16 bytes. L1I / L1D: 2-way × 4 sets. L2: 4-way × 8 sets.
//!   LRU replacement, write-back, write-allocate.
//! - L2 is inclusive: an L2 eviction removes the line from every L1 (a
//!   modified L1D copy is written back with it).
//! - Coherence (L1D): BusRd on a read miss, BusRdX on a write miss, BusUpgr
//!   on a write hit to a Shared line. The other core's L1D supplies a
//!   Modified line (flush to L2) and goes to S (BusRd) or I (BusRdX /
//!   BusUpgr). Writes also invalidate the other core's L1I copy. An
//!   instruction fetch miss flushes any Modified L1D copy first.
//! - Timing (cycle-accurate): an L1 hit takes `lat.l1` cycles; a miss
//!   holds the one bus for `l1 + l2` (L2 hit) or `l1 + l2 + mem` cycles.
//!   A core's fetch side and data side (incl. page-walk reads) are
//!   separate chains of dependent accesses; the core freezes until the
//!   later one finishes. A core that finds the bus busy waits.
//! - Every step leaves its `events` for the panel (cleared each cycle).
//!
//! The cores themselves are pipelines (`sim/pipeline`) with `hier` set:
//! their fetch, loads / stores and page-table walks come through here,
//! and their MMU reports TLB / walk / fault events here.

pub mod cache;

pub use cache::{Cache, Line, Mesi, LINE_BYTES, LINE_WORDS};

/// Physical memory size (bytes), from address 0.
pub const MEM_BYTES: u32 = 64 * 1024;
pub const N_CORES: usize = 2;

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct Latency { pub l1: u64, pub l2: u64, pub mem: u64 }

impl Default for Latency {
    fn default() -> Self { Self { l1: 1, l2: 4, mem: 20 } }
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheId { L1I, L1D, L2 }

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Port { Fetch, Load, Store, Walk }

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusReq { BusRd, BusRdX, BusUpgr }

/// Where an access was satisfied.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome { L1Hit, L2Hit, Memory }

/// What happened, for the panel. `set` / `way` locate cache lines; `pa`
/// is the line (or word) physical address.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[cfg_attr(feature = "wasm", serde(tag = "kind"))]
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    // ── MMU (reported by the core) ──
    /// No translation (M-mode or satp = Bare).
    Bare { core: u8, va: u32 },
    TlbHit { core: u8, idx: u8, va: u32, pa: u32 },
    TlbMiss { core: u8, va: u32 },
    /// One page-table read (level 1 = root, 0 = leaf table).
    Walk { core: u8, level: u8, pte_addr: u32, pte: u32 },
    /// Walk finished: TLB entry `idx` filled with VA → PA.
    TlbFill { core: u8, idx: u8, va: u32, pa: u32 },
    PageFault { core: u8, va: u32, cause: u32 },

    // ── caches / bus ──
    /// One access by a core: where it was satisfied and how long it took.
    Access { core: u8, port: Port, pa: u32, outcome: Outcome, latency: u32 },
    L1Hit { core: u8, cache: CacheId, set: u8, way: u8, pa: u32 },
    L1Miss { core: u8, cache: CacheId, set: u8, pa: u32 },
    L2Hit { core: u8, set: u8, way: u8, pa: u32 },
    L2Miss { core: u8, set: u8, pa: u32 },
    MemRead { core: u8, pa: u32 },
    /// A dirty L2 line written to memory (eviction).
    MemWrite { pa: u32 },
    /// A line filled into `cache` (owner `core`; L2: the requester).
    Fill { core: u8, cache: CacheId, set: u8, way: u8, pa: u32, state: Mesi },
    /// A line removed from `cache` (owner `core`; L2: the requester).
    Evict { core: u8, cache: CacheId, set: u8, way: u8, pa: u32, dirty: bool },
    /// A Modified L1D line copied into L2 (eviction or snoop flush).
    WriteBack { core: u8, set: u8, way: u8, pa: u32 },
    /// A line changed state without a bus transaction (E → M on a write).
    StateChange { core: u8, cache: CacheId, set: u8, way: u8, pa: u32, from: Mesi, to: Mesi },
    /// Core `from`'s bus request seen by core `to`'s cache.
    Snoop { from: u8, to: u8, req: BusReq, cache: CacheId, set: u8, way: u8, pa: u32, before: Mesi, after: Mesi, flush: bool },
    /// The bus was busy: `core` waited `cycles`.
    BusWait { core: u8, cycles: u32 },
    /// `core` is frozen this cycle waiting for memory (`left` more after this).
    Stall { core: u8, left: u32 },
}

/// Per-core counters.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    pub tlb_hits: u32, pub tlb_misses: u32, pub walks: u32, pub page_faults: u32,
    pub l1i_hits: u32, pub l1i_misses: u32, pub l1d_hits: u32, pub l1d_misses: u32,
    pub l2_hits: u32, pub l2_misses: u32,
    pub accesses: u32, pub total_latency: u64,
    pub stall_cycles: u32, pub bus_wait_cycles: u32,
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct MemSystem {
    /// Physical memory, word-addressed (`MEM_BYTES / 4` words from 0).
    pub mem: Vec<u32>,
    pub l1i: [Cache; N_CORES],
    pub l1d: [Cache; N_CORES],
    pub l2: Cache,
    pub lat: Latency,
    /// Current cycle (set by `begin_cycle`).
    pub now: u64,
    /// First cycle the bus is free.
    pub bus_free: u64,
    /// Cycle each core's fetch-side / data-side work of this cycle finishes.
    chain_i: [u64; N_CORES],
    chain_d: [u64; N_CORES],
    /// The core's current walk belongs to its fetch side.
    walk_for_fetch: [bool; N_CORES],
    /// Cycles each core stays frozen.
    pub stall: [u32; N_CORES],
    pub events: Vec<Event>,
    pub stats: [Stats; N_CORES],
}

impl Default for MemSystem {
    fn default() -> Self { Self::new() }
}

impl MemSystem {
    pub fn new() -> Self {
        Self {
            mem: vec![0; (MEM_BYTES / 4) as usize],
            l1i: [Cache::new(4, 2), Cache::new(4, 2)],
            l1d: [Cache::new(4, 2), Cache::new(4, 2)],
            l2: Cache::new(8, 4),
            lat: Latency::default(),
            now: 0, bus_free: 0,
            chain_i: [0; N_CORES], chain_d: [0; N_CORES],
            walk_for_fetch: [false; N_CORES],
            stall: [0; N_CORES],
            events: Vec::new(),
            stats: [Stats::default(); N_CORES],
        }
    }

    /// Write words into physical memory at byte address `pa` (no caches).
    pub fn load(&mut self, pa: u32, words: &[u32]) {
        for (i, &w) in words.iter().enumerate() {
            if let Some(slot) = self.mem.get_mut((pa / 4) as usize + i) { *slot = w; }
        }
    }

    /// Memory word as the whole system sees it (a Modified L1D or dirty L2
    /// copy wins over memory). For the panel / tests; no side effects.
    pub fn peek(&self, pa: u32) -> u32 {
        let w = ((pa / 4) as usize) % LINE_WORDS;
        for c in &self.l1d {
            if let Some((s, way)) = c.find(pa) { if c.line(s, way).state == Mesi::M { return c.line(s, way).data[w]; } }
        }
        if let Some((s, way)) = self.l2.find(pa) { return self.l2.line(s, way).data[w]; }
        self.mem.get((pa / 4) as usize).copied().unwrap_or(0)
    }

    // ── cycle protocol (driven by system::MemPanelSystem / the cores) ──

    /// Start cycle `now`: clear the panel events.
    pub fn begin_cycle(&mut self, now: u64) {
        self.now = now;
        self.events.clear();
    }

    /// A core starts its cycle. Returns true if it is frozen (waiting for
    /// memory) and must not advance.
    pub fn core_begin(&mut self, core: usize) -> bool {
        if self.stall[core] > 0 {
            self.stall[core] -= 1;
            self.stats[core].stall_cycles += 1;
            self.events.push(Event::Stall { core: core as u8, left: self.stall[core] });
            return true;
        }
        self.chain_i[core] = self.now;
        self.chain_d[core] = self.now;
        false
    }

    /// A core finished its cycle: freeze it until its slower side is done.
    pub fn core_end(&mut self, core: usize) {
        let done = self.chain_i[core].max(self.chain_d[core]);
        self.stall[core] = done.saturating_sub(self.now + 1) as u32;
    }

    /// The core's next page-table reads translate a fetch (true) or a load / store.
    pub fn set_walk_for_fetch(&mut self, core: usize, fetch: bool) { self.walk_for_fetch[core] = fetch; }

    pub fn event(&mut self, e: Event) {
        match &e {
            Event::TlbHit { core, .. } => self.stats[*core as usize].tlb_hits += 1,
            Event::TlbMiss { core, .. } => self.stats[*core as usize].tlb_misses += 1,
            Event::Walk { core, .. } => self.stats[*core as usize].walks += 1,
            Event::PageFault { core, .. } => self.stats[*core as usize].page_faults += 1,
            _ => {}
        }
        self.events.push(e);
    }

    // ── accesses ────────────────────────────────────────────────────────

    pub fn read(&mut self, core: usize, pa: u32, port: Port) -> u32 { self.access(core, pa, None, port) }
    pub fn write(&mut self, core: usize, pa: u32, val: u32, port: Port) { self.access(core, pa, Some(val), port); }

    /// One word access by `core` at physical address `pa` (`Some(v)` writes).
    fn access(&mut self, core: usize, pa: u32, wval: Option<u32>, port: Port) -> u32 {
        let fetch = port == Port::Fetch;
        let on_i_chain = fetch || (port == Port::Walk && self.walk_for_fetch[core]);
        let start = (if on_i_chain { self.chain_i[core] } else { self.chain_d[core] }).max(self.now);
        let c = core as u8;
        let word = ((pa / 4) as usize) % LINE_WORDS;
        let line_pa = pa & !(LINE_BYTES - 1);

        // Outside physical memory: uncached, reads 0
        if pa >= MEM_BYTES {
            let done = start + self.lat.mem;
            self.finish(core, on_i_chain, start, done, port, pa, Outcome::Memory);
            return 0;
        }

        let cid = if fetch { CacheId::L1I } else { CacheId::L1D };
        let (found, miss_set) = {
            let l1 = if fetch { &self.l1i[core] } else { &self.l1d[core] };
            (l1.find(pa), l1.split(pa).0)
        };
        let (done, outcome, set, way) = if let Some((set, way)) = found {
            // ── L1 hit ──
            if fetch { self.stats[core].l1i_hits += 1 } else { self.stats[core].l1d_hits += 1 }
            self.events.push(Event::L1Hit { core: c, cache: cid, set: set as u8, way: way as u8, pa });
            let mut done = start + self.lat.l1;
            if wval.is_some() {
                match self.l1d[core].line(set, way).state {
                    Mesi::E => self.state_change(core, set, way, line_pa, Mesi::E, Mesi::M),
                    Mesi::S => {
                        // BusUpgr: invalidate the other copies, then own it
                        let bus = self.bus_start(core, start);
                        self.snoop(core, line_pa, BusReq::BusUpgr);
                        done = bus + self.lat.l1;
                        self.bus_free = done;
                        self.state_change(core, set, way, line_pa, Mesi::S, Mesi::M);
                    }
                    _ => {}
                }
            }
            (done, Outcome::L1Hit, set, way)
        } else {
            // ── L1 miss: one bus transaction ──
            let set = miss_set;
            if fetch { self.stats[core].l1i_misses += 1 } else { self.stats[core].l1d_misses += 1 }
            self.events.push(Event::L1Miss { core: c, cache: cid, set: set as u8, pa });
            let bus = self.bus_start(core, start);
            let req = if wval.is_some() { BusReq::BusRdX } else { BusReq::BusRd };
            let shared = self.snoop(core, line_pa, if fetch { BusReq::BusRd } else { req });
            if fetch { self.flush_own_dirty(core, line_pa); }
            let (l2set, l2way, outcome) = self.l2_get(core, line_pa);
            let lat = self.lat.l1 + self.lat.l2 + if outcome == Outcome::Memory { self.lat.mem } else { 0 };
            let state = if fetch { Mesi::S } else if wval.is_some() { Mesi::M } else if shared { Mesi::S } else { Mesi::E };
            let data = self.l2.line(l2set, l2way).data;
            let way = self.l1_fill(core, fetch, line_pa, data, state);
            let done = bus + lat;
            self.bus_free = done;
            (done, outcome, set, way)
        };

        let cache = if fetch { &mut self.l1i[core] } else { &mut self.l1d[core] };
        cache.touch(set, way);
        let line = cache.line_mut(set, way);
        let v = line.data[word];
        if let Some(val) = wval { line.data[word] = val; }
        self.finish(core, on_i_chain, start, done, port, pa, outcome);
        v
    }

    fn finish(&mut self, core: usize, on_i_chain: bool, start: u64, done: u64, port: Port, pa: u32, outcome: Outcome) {
        if on_i_chain { self.chain_i[core] = done } else { self.chain_d[core] = done }
        let latency = (done - start) as u32;
        self.stats[core].accesses += 1;
        self.stats[core].total_latency += latency as u64;
        self.events.push(Event::Access { core: core as u8, port, pa, outcome, latency });
    }

    /// First cycle `core` can use the bus (waiting if it is busy).
    fn bus_start(&mut self, core: usize, start: u64) -> u64 {
        let bus = start.max(self.bus_free);
        if bus > start {
            let cycles = (bus - start) as u32;
            self.stats[core].bus_wait_cycles += cycles;
            self.events.push(Event::BusWait { core: core as u8, cycles });
        }
        bus
    }

    fn state_change(&mut self, core: usize, set: usize, way: usize, pa: u32, from: Mesi, to: Mesi) {
        self.l1d[core].line_mut(set, way).state = to;
        self.events.push(Event::StateChange { core: core as u8, cache: CacheId::L1D, set: set as u8, way: way as u8, pa, from, to });
    }

    /// Bus request by `core` for `line_pa`, seen by the other core's caches.
    /// Returns true if the other L1D still holds the line afterwards.
    fn snoop(&mut self, core: usize, line_pa: u32, req: BusReq) -> bool {
        let mut shared = false;
        for other in (0..N_CORES).filter(|&o| o != core) {
            if let Some((set, way)) = self.l1d[other].find(line_pa) {
                let before = self.l1d[other].line(set, way).state;
                let flush = before == Mesi::M;
                if flush { self.flush_to_l2(other, set, way, line_pa); }
                let after = if req == BusReq::BusRd { Mesi::S } else { Mesi::I };
                self.l1d[other].line_mut(set, way).state = after;
                shared |= after != Mesi::I;
                self.events.push(Event::Snoop { from: core as u8, to: other as u8, req, cache: CacheId::L1D,
                    set: set as u8, way: way as u8, pa: line_pa, before, after, flush });
            }
            if req != BusReq::BusRd {
                if let Some((set, way)) = self.l1i[other].find(line_pa) {
                    self.l1i[other].line_mut(set, way).state = Mesi::I;
                    self.events.push(Event::Snoop { from: core as u8, to: other as u8, req, cache: CacheId::L1I,
                        set: set as u8, way: way as u8, pa: line_pa, before: Mesi::S, after: Mesi::I, flush: false });
                }
            }
        }
        shared
    }

    /// Before an instruction fetch fills L1I: write back the core's own
    /// Modified L1D copy of the line (it becomes Shared).
    fn flush_own_dirty(&mut self, core: usize, line_pa: u32) {
        if let Some((set, way)) = self.l1d[core].find(line_pa) {
            if self.l1d[core].line(set, way).state == Mesi::M {
                self.flush_to_l2(core, set, way, line_pa);
                self.state_change(core, set, way, line_pa, Mesi::M, Mesi::S);
            }
        }
    }

    /// Copy a Modified L1D line into its (inclusive) L2 line, marking L2 dirty.
    fn flush_to_l2(&mut self, core: usize, set: usize, way: usize, line_pa: u32) {
        let data = self.l1d[core].line(set, way).data;
        if let Some((s2, w2)) = self.l2.find(line_pa) {
            let l = self.l2.line_mut(s2, w2);
            l.data = data;
            l.state = Mesi::M;
        }
        self.events.push(Event::WriteBack { core: core as u8, set: set as u8, way: way as u8, pa: line_pa });
    }

    /// The L2 line for `line_pa`, filling it from memory on a miss.
    fn l2_get(&mut self, core: usize, line_pa: u32) -> (usize, usize, Outcome) {
        let c = core as u8;
        if let Some((set, way)) = self.l2.find(line_pa) {
            self.stats[core].l2_hits += 1;
            self.l2.touch(set, way);
            self.events.push(Event::L2Hit { core: c, set: set as u8, way: way as u8, pa: line_pa });
            return (set, way, Outcome::L2Hit);
        }
        let (set, tag) = self.l2.split(line_pa);
        self.stats[core].l2_misses += 1;
        self.events.push(Event::L2Miss { core: c, set: set as u8, pa: line_pa });
        let way = self.l2.victim(set);
        let victim = *self.l2.line(set, way);
        if victim.valid() {
            let vpa = self.l2.line_addr(set, victim.tag);
            self.back_invalidate(core, vpa, set, way);
            let v = *self.l2.line(set, way);
            if v.state == Mesi::M {
                self.load(vpa, &v.data);
                self.events.push(Event::MemWrite { pa: vpa });
            }
            self.events.push(Event::Evict { core: c, cache: CacheId::L2, set: set as u8, way: way as u8, pa: vpa, dirty: v.state == Mesi::M });
        }
        self.events.push(Event::MemRead { core: c, pa: line_pa });
        let base = (line_pa / 4) as usize;
        let mut data = [0u32; LINE_WORDS];
        for (i, d) in data.iter_mut().enumerate() { *d = self.mem.get(base + i).copied().unwrap_or(0); }
        *self.l2.line_mut(set, way) = Line { state: Mesi::E, tag, lru: 0, data };
        self.l2.touch(set, way);
        self.events.push(Event::Fill { core: c, cache: CacheId::L2, set: set as u8, way: way as u8, pa: line_pa, state: Mesi::E });
        (set, way, Outcome::Memory)
    }

    /// Inclusion: an L2 victim leaves every L1. A Modified L1D copy is
    /// merged into the victim first (so it reaches memory).
    fn back_invalidate(&mut self, requester: usize, vpa: u32, l2set: usize, l2way: usize) {
        let _ = requester;
        for core in 0..N_CORES {
            if let Some((set, way)) = self.l1d[core].find(vpa) {
                let l = *self.l1d[core].line(set, way);
                if l.state == Mesi::M {
                    let v = self.l2.line_mut(l2set, l2way);
                    v.data = l.data;
                    v.state = Mesi::M;
                }
                self.l1d[core].line_mut(set, way).state = Mesi::I;
                self.events.push(Event::Evict { core: core as u8, cache: CacheId::L1D, set: set as u8, way: way as u8, pa: vpa, dirty: l.state == Mesi::M });
            }
            if let Some((set, way)) = self.l1i[core].find(vpa) {
                self.l1i[core].line_mut(set, way).state = Mesi::I;
                self.events.push(Event::Evict { core: core as u8, cache: CacheId::L1I, set: set as u8, way: way as u8, pa: vpa, dirty: false });
            }
        }
    }

    /// Put a line into the core's L1I / L1D (evicting the LRU line; a
    /// Modified victim is written back to L2). Returns the way.
    fn l1_fill(&mut self, core: usize, fetch: bool, line_pa: u32, data: [u32; LINE_WORDS], state: Mesi) -> usize {
        let c = core as u8;
        let cid = if fetch { CacheId::L1I } else { CacheId::L1D };
        let cache = if fetch { &self.l1i[core] } else { &self.l1d[core] };
        let (set, tag) = cache.split(line_pa);
        let way = cache.victim(set);
        let victim = *cache.line(set, way);
        if victim.valid() {
            let vpa = cache.line_addr(set, victim.tag);
            if !fetch && victim.state == Mesi::M { self.flush_to_l2(core, set, way, vpa); }
            self.events.push(Event::Evict { core: c, cache: cid, set: set as u8, way: way as u8, pa: vpa, dirty: victim.state == Mesi::M });
        }
        let cache = if fetch { &mut self.l1i[core] } else { &mut self.l1d[core] };
        *cache.line_mut(set, way) = Line { state, tag, lru: 0, data };
        self.events.push(Event::Fill { core: c, cache: cid, set: set as u8, way: way as u8, pa: line_pa, state });
        way
    }
}

#[cfg(test)]
mod tests;
