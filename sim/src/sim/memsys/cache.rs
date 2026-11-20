//! One set-associative cache: lines with a MESI state, LRU ages, 4 words.

/// Bytes per cache line.
pub const LINE_BYTES: u32 = 16;
/// Words per cache line.
pub const LINE_WORDS: usize = 4;

/// Line state. L1D uses full MESI. L1I lines are only I / S. L2 uses
/// I (invalid), E (clean) and M (dirty, newer than memory).
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mesi { #[default] I, S, E, M }

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, Default)]
pub struct Line {
    pub state: Mesi,
    pub tag: u32,
    /// Age since last use within its set (0 = most recent).
    pub lru: u8,
    pub data: [u32; LINE_WORDS],
}

impl Line {
    pub fn valid(&self) -> bool { self.state != Mesi::I }
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Cache {
    pub sets: usize,
    pub ways: usize,
    /// `sets * ways` lines, set-major (`set * ways + way`).
    pub lines: Vec<Line>,
}

impl Cache {
    pub fn new(sets: usize, ways: usize) -> Self {
        Self { sets, ways, lines: vec![Line::default(); sets * ways] }
    }

    /// (set, tag) of a physical address.
    pub fn split(&self, pa: u32) -> (usize, u32) {
        let la = pa / LINE_BYTES;
        ((la as usize) % self.sets, la / self.sets as u32)
    }

    /// Line base address of (set, tag).
    pub fn line_addr(&self, set: usize, tag: u32) -> u32 {
        (tag * self.sets as u32 + set as u32) * LINE_BYTES
    }

    pub fn line(&self, set: usize, way: usize) -> &Line { &self.lines[set * self.ways + way] }
    pub fn line_mut(&mut self, set: usize, way: usize) -> &mut Line { &mut self.lines[set * self.ways + way] }

    /// The valid line holding `pa`, if any: (set, way).
    pub fn find(&self, pa: u32) -> Option<(usize, usize)> {
        let (set, tag) = self.split(pa);
        (0..self.ways).find(|&w| { let l = self.line(set, w); l.valid() && l.tag == tag }).map(|w| (set, w))
    }

    /// Mark (set, way) most recently used.
    pub fn touch(&mut self, set: usize, way: usize) {
        for w in 0..self.ways {
            let l = self.line_mut(set, w);
            l.lru = if w == way { 0 } else { l.lru.saturating_add(1) };
        }
    }

    /// Way to replace in `set`: an invalid one, else the least recently used.
    pub fn victim(&self, set: usize) -> usize {
        if let Some(w) = (0..self.ways).find(|&w| !self.line(set, w).valid()) { return w; }
        (0..self.ways).max_by_key(|&w| self.line(set, w).lru).unwrap_or(0)
    }
}
