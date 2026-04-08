//! Tiny direct-mapped Sv32 TLB (Phase 4.4).
//!
//! 8 entries indexed by `vpn[2:0]`. Stores both 4 KiB and 4 MiB
//! superpage translations. Flushed by `SFENCE.VMA` and on writes to
//! `satp`. No ASID support — flush is unconditional and global.

const TLB_ENTRIES: usize = 8;
const TLB_INDEX_MASK: u32 = (TLB_ENTRIES as u32) - 1;

#[derive(Debug, Clone, Copy, Default)]
pub struct TlbEntry {
    pub valid: bool,
    /// Virtual page number (20 bits = vaddr[31:12]). For superpages,
    /// only the high 10 bits (vpn[1]) are meaningful — the low 10
    /// bits are zeroed at install time so equality compares match.
    pub vpn: u32,
    /// Physical page number (20 bits).
    pub ppn: u32,
    /// PTE flag bits (V/R/W/X/U/A/D); the walker stores the leaf PTE
    /// value here so the perm checker can re-evaluate on hit.
    pub pte: u32,
    /// True if this entry covers a 4 MiB superpage.
    pub is_super: bool,
}

#[derive(Debug, Clone)]
pub struct Tlb {
    pub entries: [TlbEntry; TLB_ENTRIES],
    pub last_hit: bool,
    pub last_index: u8,
}

impl Default for Tlb {
    fn default() -> Self {
        Self {
            entries: [TlbEntry::default(); TLB_ENTRIES],
            last_hit: false,
            last_index: 0,
        }
    }
}

impl Tlb {
    pub fn new() -> Self { Self::default() }

    pub fn flush(&mut self) {
        for e in self.entries.iter_mut() { e.valid = false; }
    }

    /// Look up `vpn`. Matches a regular entry if `vpn` matches exactly,
    /// or a superpage entry if `vpn >> 10 == entry.vpn >> 10`.
    pub fn lookup(&mut self, vpn: u32) -> Option<TlbEntry> {
        let idx_normal = (vpn & TLB_INDEX_MASK) as usize;
        let idx_super  = ((vpn >> 10) & TLB_INDEX_MASK) as usize;
        // Try normal-page slot first.
        let e = self.entries[idx_normal];
        if e.valid && !e.is_super && e.vpn == vpn {
            self.last_hit = true;
            self.last_index = idx_normal as u8;
            return Some(e);
        }
        // Try superpage slot.
        let e = self.entries[idx_super];
        if e.valid && e.is_super && (e.vpn >> 10) == (vpn >> 10) {
            self.last_hit = true;
            self.last_index = idx_super as u8;
            return Some(e);
        }
        self.last_hit = false;
        None
    }

    pub fn install(&mut self, vpn: u32, ppn: u32, pte: u32, is_super: bool) {
        let key = if is_super { vpn & !0x3ff } else { vpn };
        let idx = if is_super {
            ((vpn >> 10) & TLB_INDEX_MASK) as usize
        } else {
            (vpn & TLB_INDEX_MASK) as usize
        };
        self.entries[idx] = TlbEntry {
            valid: true, vpn: key, ppn, pte, is_super,
        };
    }
}
