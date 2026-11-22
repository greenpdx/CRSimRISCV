//! Fully associative Sv32 TLB, as CVA6's ITLB / DTLB. Each core has two:
//! an ITLB for fetches and a DTLB for loads / stores.
//!
//! 4 entries, all compared at once (CVA6 Fig 6): an entry hits when it
//! is valid, its VPN matches (VPN[1] only for a 4 MiB superpage) and its
//! ASID matches the current one or it is global (G). Replacement: the
//! first invalid entry, else a 3-bit tree pseudo-LRU (CVA6 Figs 11–13).
//! SFENCE.VMA flushes by virtual address and / or ASID (CVA6 Figs 7–10).

pub const TLB_ENTRIES: usize = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TlbEntry {
    pub valid: bool,
    /// Virtual page number (20 bits = vaddr[31:12]). For superpages,
    /// only the high 10 bits (vpn[1]) are meaningful — the low 10
    /// bits are zeroed at install time.
    pub vpn: u32,
    /// Physical page number (20 bits).
    pub ppn: u32,
    /// The leaf PTE (V/R/W/X/U/G/A/D) so the permission check can run on a hit.
    pub pte: u32,
    /// True if this entry covers a 4 MiB superpage.
    pub is_super: bool,
    /// Address-space ID from satp when the entry was filled.
    pub asid: u16,
    /// Global mapping (G set in the leaf or in the pointer PTE above it):
    /// matches every ASID and survives an ASID flush.
    pub global: bool,
}

/// One entry's part in a lookup (the comparators of CVA6 Fig 6).
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntryCmp {
    pub valid: bool,
    pub vpn_match: bool,
    pub asid_match: bool,
    pub global: bool,
    /// valid ∧ vpn_match ∧ (asid_match ∨ global)
    pub hit: bool,
}

/// One entry's part in an SFENCE.VMA (the gates of CVA6 Fig 7).
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntryFlush {
    pub valid: bool,
    /// rs1 = x0 → true; else the entry maps rs1's page
    pub va_match: bool,
    /// rs2 = x0 → true; else entry.asid == rs2
    pub asid_match: bool,
    pub global: bool,
    /// valid ∧ va_match ∧ asid_match ∧ ¬(rs2 ≠ x0 ∧ global)
    pub inval: bool,
}

/// Which entry a fill replaced and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Victim {
    pub idx: u8,
    /// An invalid entry was free (no replacement needed).
    pub was_invalid: bool,
    pub plru_before: u8,
    pub plru_after: u8,
}

#[derive(Debug, Clone, Default)]
pub struct Tlb {
    pub entries: [TlbEntry; TLB_ENTRIES],
    /// Tree PLRU: bit 0 = root (0 → victim on the left, entries 0/1;
    /// 1 → right, entries 2/3), bit 1 = left pair (0 → 0, 1 → 1),
    /// bit 2 = right pair (0 → 2, 1 → 3).
    pub plru: u8,
    pub last_hit: bool,
    pub last_index: u8,
}

/// Does entry `e` map virtual page `vpn` (level match: VPN[1] only for a superpage)?
fn maps(e: &TlbEntry, vpn: u32) -> bool {
    if e.is_super { (e.vpn >> 10) == (vpn >> 10) } else { e.vpn == vpn }
}

impl Tlb {
    pub fn new() -> Self { Self::default() }

    /// Invalidate every entry (reset).
    pub fn flush(&mut self) {
        for e in self.entries.iter_mut() { e.valid = false; }
    }

    /// Point the PLRU tree away from entry `idx` (it becomes most recently used).
    pub fn touch(&mut self, idx: usize) {
        let mut p = self.plru;
        if idx < 2 {
            p |= 1;                                              // root → right
            p = if idx == 0 { p | 2 } else { p & !2 };           // left pair → the other one
        } else {
            p &= !1;                                             // root → left
            p = if idx == 2 { p | 4 } else { p & !4 };
        }
        self.plru = p;
    }

    /// The entry the PLRU tree points at.
    pub fn plru_victim(&self) -> usize {
        if self.plru & 1 == 0 { (self.plru >> 1) as usize & 1 } else { 2 + ((self.plru >> 2) as usize & 1) }
    }

    /// Compare `vpn` / `asid` against every entry. Returns the hit entry
    /// (and touches it in the PLRU tree) and each entry's comparator outputs.
    pub fn lookup(&mut self, vpn: u32, asid: u16) -> (Option<usize>, [EntryCmp; TLB_ENTRIES]) {
        let mut cmp = [EntryCmp::default(); TLB_ENTRIES];
        for (c, e) in cmp.iter_mut().zip(self.entries.iter()) {
            c.valid = e.valid;
            c.vpn_match = maps(e, vpn);
            c.asid_match = e.asid == asid;
            c.global = e.global;
            c.hit = e.valid && c.vpn_match && (c.asid_match || c.global);
        }
        let hit = cmp.iter().position(|c| c.hit);
        self.last_hit = hit.is_some();
        if let Some(i) = hit {
            self.last_index = i as u8;
            self.touch(i);
        }
        (hit, cmp)
    }

    /// Fill a new entry: the first invalid one, else the PLRU victim.
    pub fn install(&mut self, vpn: u32, ppn: u32, pte: u32, is_super: bool, asid: u16, global: bool) -> Victim {
        let plru_before = self.plru;
        let free = self.entries.iter().position(|e| !e.valid);
        let idx = free.unwrap_or_else(|| self.plru_victim());
        let key = if is_super { vpn & !0x3ff } else { vpn };
        self.entries[idx] = TlbEntry { valid: true, vpn: key, ppn, pte, is_super, asid, global };
        self.touch(idx);
        Victim { idx: idx as u8, was_invalid: free.is_some(), plru_before, plru_after: self.plru }
    }

    /// SFENCE.VMA rs1, rs2: `va` = rs1's address (None for x0), `asid` =
    /// rs2's ASID (None for x0). Global entries survive an ASID flush.
    pub fn sfence(&mut self, va: Option<u32>, asid: Option<u16>) -> [EntryFlush; TLB_ENTRIES] {
        let mut out = [EntryFlush::default(); TLB_ENTRIES];
        for (f, e) in out.iter_mut().zip(self.entries.iter_mut()) {
            f.valid = e.valid;
            f.va_match = va.is_none_or(|v| maps(e, v >> 12));
            f.asid_match = asid.is_none_or(|a| e.asid == a);
            f.global = e.global;
            f.inval = e.valid && f.va_match && f.asid_match && !(asid.is_some() && e.global);
            if f.inval { e.valid = false; }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_free_entries_then_replaces_by_plru() {
        let mut t = Tlb::new();
        for i in 0..4 { let v = t.install(0x100 + i, i, 1, false, 0, false); assert_eq!((v.idx, v.was_invalid), (i as u8, true)); }
        // 0,1,2,3 touched in order → least recent is 0
        let v = t.install(0x200, 9, 1, false, 0, false);
        assert_eq!((v.idx, v.was_invalid), (0, false));
        // touch 1 (hit), then the victim is in the right pair: 2
        assert_eq!(t.lookup(0x101, 0).0, Some(1));
        assert_eq!(t.install(0x201, 9, 1, false, 0, false).idx, 2);
    }

    #[test]
    fn asid_and_global_compare() {
        let mut t = Tlb::new();
        t.install(0x10, 1, 1, false, 5, false);
        t.install(0x20, 2, 1, false, 5, true);
        assert_eq!(t.lookup(0x10, 5).0, Some(0));
        assert_eq!(t.lookup(0x10, 6).0, None, "other ASID misses");
        assert_eq!(t.lookup(0x20, 6).0, Some(1), "global matches any ASID");
        assert_eq!(t.lookup(0x10, 6).1[0], EntryCmp { valid: true, vpn_match: true, asid_match: false, global: false, hit: false });
    }

    #[test]
    fn sfence_four_cases() {
        let fill = || {
            let mut t = Tlb::new();
            t.install(0x10, 1, 1, false, 1, false);   // ASID 1
            t.install(0x11, 2, 1, false, 2, false);   // ASID 2
            t.install(0x10, 3, 1, false, 2, false);   // same page, ASID 2
            t.install(0x30, 4, 1, false, 1, true);    // global
            t
        };
        let valid = |t: &Tlb| t.entries.iter().map(|e| e.valid).collect::<Vec<_>>();
        let mut t = fill(); t.sfence(None, None);              assert_eq!(valid(&t), [false; 4], "x0, x0: all");
        let mut t = fill(); t.sfence(Some(0x10_000), None);    assert_eq!(valid(&t), [false, true, false, true], "vaddr, x0: that page, any ASID");
        let mut t = fill(); t.sfence(None, Some(2));           assert_eq!(valid(&t), [true, false, false, true], "x0, asid: that ASID, not global");
        let mut t = fill(); t.sfence(Some(0x10_000), Some(2)); assert_eq!(valid(&t), [true, true, false, true], "vaddr, asid");
        let mut t = fill(); t.sfence(Some(0x30_000), Some(1)); assert_eq!(valid(&t), [true; 4], "global survives an ASID flush");
    }
}
