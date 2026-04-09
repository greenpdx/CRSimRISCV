//! Write-through D-cache store buffer.
//!
//! 4-deep FIFO of pending stores. The bus drains one entry per
//! bus-idle cycle (1-cycle latency per drain). When the buffer is
//! full, the issuing thread stalls until at least one entry has
//! been drained.
//!
//! Read-after-write forwarding: a load that hits any pending
//! store entry returns the stored value (most recent matching
//! entry wins) instead of consulting the cache. This guarantees
//! `sw addr; lw addr` reads back the just-written value even
//! before the store has been drained to external memory.

pub const STORE_BUFFER_DEPTH: usize = 4;

#[derive(Debug, Clone, Copy, Default)]
pub struct StoreEntry {
    pub addr: u32,    // word-aligned
    pub value: u32,
    pub thread: u8,   // who issued it (for stats / visualization)
}

#[derive(Debug, Clone)]
pub struct StoreBuffer {
    pub entries: [StoreEntry; STORE_BUFFER_DEPTH],
    pub count: usize,    // 0..=STORE_BUFFER_DEPTH
}

impl StoreBuffer {
    pub fn new() -> Self {
        Self { entries: [StoreEntry::default(); STORE_BUFFER_DEPTH], count: 0 }
    }
    pub fn reset(&mut self) {
        self.count = 0;
        self.entries = [StoreEntry::default(); STORE_BUFFER_DEPTH];
    }
    pub fn is_empty(&self) -> bool { self.count == 0 }
    pub fn is_full(&self) -> bool { self.count == STORE_BUFFER_DEPTH }
    pub fn len(&self) -> usize { self.count }

    /// Push a new store entry. Returns false if the buffer was
    /// full (caller must stall).
    pub fn push(&mut self, addr: u32, value: u32, thread: u8) -> bool {
        if self.is_full() { return false; }
        self.entries[self.count] = StoreEntry { addr: addr & !3, value, thread };
        self.count += 1;
        true
    }

    /// Pop the oldest entry (FIFO order).
    pub fn pop_front(&mut self) -> Option<StoreEntry> {
        if self.count == 0 { return None; }
        let head = self.entries[0];
        for i in 1..self.count {
            self.entries[i - 1] = self.entries[i];
        }
        self.count -= 1;
        self.entries[self.count] = StoreEntry::default();
        Some(head)
    }

    /// Read-after-write forwarding: scan the buffer for the most
    /// recent entry matching `addr` (word-aligned). Returns
    /// `Some(value)` if any entry matches.
    pub fn forward(&self, addr: u32) -> Option<u32> {
        let key = addr & !3;
        for i in (0..self.count).rev() {
            if self.entries[i].addr == key {
                return Some(self.entries[i].value);
            }
        }
        None
    }
}

impl Default for StoreBuffer { fn default() -> Self { Self::new() } }
