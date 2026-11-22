//! Memory hierarchy for the memory panel: two cores, each with an L1I and
//! an L1D, one shared inclusive L2 and one physical memory, with MESI
//! coherence between the L1Ds over a snooping bus.
//!
//! - Memory map: the boot ROM at `ROM_BASE` (uncached, read-only, one bus
//!   read per word); everything else is RAM of any size (sparse pages).
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
pub mod examples;
pub mod system;

pub use cache::{Cache, Line, Mesi, LINE_BYTES, LINE_WORDS};
use crate::sim::mem::{Pages, Port};
pub use crate::sim::pipeline::tlb::{EntryCmp, EntryFlush, TLB_ENTRIES};

pub const N_CORES: usize = 2;

/// Boot ROM (flash): uncached, read-only, read word by word over the bus.
/// Everything else in the 32-bit physical space is RAM.
pub const ROM_BASE: u32 = 0xF000_0000;
pub const ROM_BYTES: u32 = 0x1_0000;
pub fn is_rom(pa: u32) -> bool { pa.wrapping_sub(ROM_BASE) < ROM_BYTES }

#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]

#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy)]
pub struct Latency { pub l1: u64, pub l2: u64, pub mem: u64, /** per ROM word (uncached) */ pub rom: u64 }

impl Default for Latency {
    fn default() -> Self { Self { l1: 1, l2: 4, mem: 20, rom: 8 } }
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]

#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheId { L1I, L1D, L2 }

#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]

#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusReq { BusRd, BusRdX, BusUpgr }

/// Where an access was satisfied.
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome { L1Hit, L2Hit, Memory, /** uncached boot ROM */ Rom }

/// What the page-table walker found in a PTE.
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PteVerdict {
    /// V = 1, R = W = X = 0: points to the next-level table.
    Pointer,
    /// V = 1 and R / X set: a leaf (the translation).
    Leaf,
    /// V = 0: page fault.
    Invalid,
    /// W = 1 without R: reserved encoding, page fault.
    Reserved,
    /// Level-1 leaf with PPN[0] ≠ 0: misaligned superpage, page fault.
    Misaligned,
    /// A pointer at level 0 (no more levels): page fault.
    NoLeaf,
}

/// One way's tag comparator in a cache lookup.
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WayCmp { pub valid: bool, pub tag: u32, pub hit: bool }

/// What happened, for the panel. `set` / `way` locate cache lines; `pa`
/// is the line (or word) physical address.
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[cfg_attr(feature = "wasm", serde(tag = "kind"))]
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    // ── MMU (reported by the core; `port` = the access being translated:
    //    Fetch, or Load / Store) ──
    /// No translation (M-mode or satp = Bare).
    Bare { core: u8, port: Port, va: u32 },
    /// Every translation starts here: VPN + ASID compared against all
    /// entries of the ITLB (Fetch) or DTLB (Load / Store) at once.
    /// `plru_*` = the replacement tree before / after (a hit touches it).
    TlbLookup { core: u8, port: Port, va: u32, asid: u16, cmp: [EntryCmp; TLB_ENTRIES], hit: Option<u8>, plru_before: u8, plru_after: u8 },
    /// Permission check of the leaf PTE (R / W / X, U with SUM, MXR).
    PermCheck { core: u8, port: Port, pte: u32, priv_mode: u8, sum: bool, mxr: bool, ok: bool },
    /// Walker: address of the PTE at `level` = base (satp.PPN or the
    /// pointer PTE's PPN, × 4096) + VPN[level] × 4.
    PteAddr { core: u8, port: Port, level: u8, base: u32, vpn: u32, addr: u32 },
    /// Walker: what the PTE read at `level` is.
    PteCheck { core: u8, port: Port, level: u8, pte: u32, verdict: PteVerdict },
    /// Hardware A / D update written back to the PTE.
    AdUpdate { core: u8, port: Port, pte_addr: u32, old: u32, new: u32 },
    /// Walker: the TLB entry the new translation goes into.
    TlbVictim { core: u8, port: Port, idx: u8, was_invalid: bool, plru_before: u8, plru_after: u8 },
    /// SFENCE.VMA rs1, rs2 (`va` / `asid` = None for x0): each entry's
    /// match gates in the ITLB and DTLB, and which were invalidated.
    Sfence { core: u8, va: Option<u32>, asid: Option<u16>, itlb: [EntryFlush; TLB_ENTRIES], dtlb: [EntryFlush; TLB_ENTRIES] },
    TlbHit { core: u8, port: Port, idx: u8, va: u32, pa: u32 },
    TlbMiss { core: u8, port: Port, va: u32 },
    /// One page-table read (level 1 = root, 0 = leaf table).
    Walk { core: u8, port: Port, level: u8, pte_addr: u32, pte: u32 },
    /// Walk finished: TLB entry `idx` filled with VA → PA.
    TlbFill { core: u8, port: Port, idx: u8, va: u32, pa: u32 },
    PageFault { core: u8, port: Port, va: u32, cause: u32 },

    // ── caches / bus ──
    /// One access by a core: where it was satisfied and how long it took.
    Access { core: u8, port: Port, pa: u32, outcome: Outcome, latency: u32 },
    /// A cache lookup: `pa` split into tag / set (index) / offset; the set's
    /// ways compared (valid ∧ tag =) before any fill. Then L1Hit / L1Miss
    /// (or L2Hit / L2Miss) says what came of it.
    Lookup { core: u8, cache: CacheId, pa: u32, set: u8, tag: u32, ways: Vec<WayCmp> },
    L1Hit { core: u8, cache: CacheId, set: u8, way: u8, pa: u32 },
    L1Miss { core: u8, cache: CacheId, set: u8, pa: u32 },
    L2Hit { core: u8, set: u8, way: u8, pa: u32 },
    L2Miss { core: u8, set: u8, pa: u32 },
    MemRead { core: u8, pa: u32 },
    /// A dirty L2 line written to memory (eviction).
    MemWrite { pa: u32 },
    /// One uncached word read from the boot ROM over the bus.
    RomRead { core: u8, pa: u32, val: u32 },
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
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    pub tlb_hits: u32, pub tlb_misses: u32, pub walks: u32, pub page_faults: u32,
    pub l1i_hits: u32, pub l1i_misses: u32, pub l1d_hits: u32, pub l1d_misses: u32,
    pub l2_hits: u32, pub l2_misses: u32, pub rom_reads: u32,
    pub accesses: u32, pub total_latency: u64,
    pub stall_cycles: u32, pub bus_wait_cycles: u32,
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]

#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone)]
pub struct MemSystem {
    /// Physical RAM (any size, sparse).
    pub ram: Pages,
    /// Boot ROM contents (`ROM_BYTES` from `ROM_BASE`).
    pub rom: Vec<u32>,
    #[cfg_attr(feature = "wasm", tsify(type = "Cache<Mesi>[]"))]
    pub l1i: [Cache; N_CORES],
    #[cfg_attr(feature = "wasm", tsify(type = "Cache<Mesi>[]"))]
    pub l1d: [Cache; N_CORES],
    #[cfg_attr(feature = "wasm", tsify(type = "Cache<Mesi>"))]
    pub l2: Cache,
    pub lat: Latency,
    /// Current cycle (set by `begin_cycle`).
    pub now: u64,
    /// First cycle the bus is free.
    pub bus_free: u64,
    /// Cycle each core's fetch-side / data-side work of this cycle finishes.
    chain_i: [u64; N_CORES],
    chain_d: [u64; N_CORES],
    /// The access the core is translating now (fetch side if Fetch).
    pub xlat_port: [Port; N_CORES],
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
            ram: Pages::default(),
            rom: vec![0; (ROM_BYTES / 4) as usize],
            l1i: [Cache::new(4, 2), Cache::new(4, 2)],
            l1d: [Cache::new(4, 2), Cache::new(4, 2)],
            l2: Cache::new(8, 4),
            lat: Latency::default(),
            now: 0, bus_free: 0,
            chain_i: [0; N_CORES], chain_d: [0; N_CORES],
            xlat_port: [Port::Load; N_CORES],
            stall: [0; N_CORES],
            events: Vec::new(),
            stats: [Stats::default(); N_CORES],
        }
    }

    /// Write words into physical memory (RAM or ROM) at byte address `pa` (no caches).
    pub fn load(&mut self, pa: u32, words: &[u32]) {
        for (i, &w) in words.iter().enumerate() {
            let a = pa.wrapping_add(4 * i as u32);
            if is_rom(a) { self.rom[((a - ROM_BASE) / 4) as usize] = w } else { self.ram.set(a, w) }
        }
    }

    /// The word in ROM / RAM itself (no caches).
    pub fn raw(&self, pa: u32) -> u32 {
        if is_rom(pa) { self.rom[((pa - ROM_BASE) / 4) as usize] } else { self.ram.get(pa) }
    }

    /// Memory word as the whole system sees it (a Modified L1D or dirty L2
    /// copy wins over memory). For the panel / tests; no side effects.
    pub fn peek(&self, pa: u32) -> u32 {
        let w = ((pa / 4) as usize) % LINE_WORDS;
        for c in &self.l1d {
            if let Some((s, way)) = c.find(pa) { if c.line(s, way).state == Mesi::M { return c.line(s, way).data[w]; } }
        }
        if is_rom(pa) { return self.raw(pa); }
        if let Some((s, way)) = self.l2.find(pa) { return self.l2.line(s, way).data[w]; }
        self.ram.get(pa)
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

    /// The access the core is about to translate (its page-table reads
    /// belong to that side; its MMU events carry it).
    pub fn set_xlat_port(&mut self, core: usize, port: Port) { self.xlat_port[core] = port; }

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
        let on_i_chain = fetch || (port == Port::Walk && self.xlat_port[core] == Port::Fetch);
        let start = (if on_i_chain { self.chain_i[core] } else { self.chain_d[core] }).max(self.now);
        let c = core as u8;
        let word = ((pa / 4) as usize) % LINE_WORDS;
        let line_pa = pa & !(LINE_BYTES - 1);

        // Boot ROM: uncached — one bus transaction per word, no cache fill.
        // Writes are ignored (read-only).
        if is_rom(pa) {
            let bus = self.bus_start(core, start);
            let done = bus + self.lat.rom;
            self.bus_free = done;
            let val = self.raw(pa & !3);
            self.stats[core].rom_reads += 1;
            self.events.push(Event::RomRead { core: c, pa: pa & !3, val });
            self.finish(core, on_i_chain, start, done, port, pa, Outcome::Rom);
            return if wval.is_none() { val } else { 0 };
        }

        let cid = if fetch { CacheId::L1I } else { CacheId::L1D };
        let (found, miss_set) = {
            let l1 = if fetch { &self.l1i[core] } else { &self.l1d[core] };
            (l1.find(pa), l1.split(pa).0)
        };
        let lk = self.lookup_event(c, cid, pa);
        self.events.push(lk);
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

    /// The tag comparators of `cache` for `pa` (state before the access).
    fn lookup_event(&self, core: u8, cache: CacheId, pa: u32) -> Event {
        let ch = match cache { CacheId::L1I => &self.l1i[core as usize], CacheId::L1D => &self.l1d[core as usize], CacheId::L2 => &self.l2 };
        let (set, tag) = ch.split(pa);
        let ways = (0..ch.ways).map(|w| { let l = ch.line(set, w); WayCmp { valid: l.valid(), tag: l.tag, hit: l.valid() && l.tag == tag } }).collect();
        Event::Lookup { core, cache, pa, set: set as u8, tag, ways }
    }

    /// The L2 line for `line_pa`, filling it from memory on a miss.
    fn l2_get(&mut self, core: usize, line_pa: u32) -> (usize, usize, Outcome) {
        let c = core as u8;
        let lk = self.lookup_event(c, CacheId::L2, line_pa);
        self.events.push(lk);
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
        let mut data = [0u32; LINE_WORDS];
        for (i, d) in data.iter_mut().enumerate() { *d = self.ram.get(line_pa + 4 * i as u32); }
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
