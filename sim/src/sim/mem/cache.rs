//! A set-associative cache array: `sets × ways` lines of 4 words, an LRU
//! age per line, and a line state `S` — a valid bit for the barrel's L1s,
//! MESI for the memory panel's hierarchy. Placement, lookup and replacement
//! only; who fills it, when, and what the states mean belongs to the owner.

/// Bytes per cache line.
pub const LINE_BYTES: u32 = 16;
/// Words per cache line.
pub const LINE_WORDS: usize = 4;

/// What a line's state must tell the array.
pub trait LineState: Copy + Default + PartialEq + std::fmt::Debug {
    fn valid(&self) -> bool;
}

/// Plain valid bit.
impl LineState for bool {
    fn valid(&self) -> bool { *self }
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]

#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, Default)]
pub struct Line<S> {
    pub state: S,
    pub tag: u32,
    /// Age since last use within its set (0 = most recent).
    pub lru: u8,
    pub data: [u32; LINE_WORDS],
}

impl<S: LineState> Line<S> {
    pub fn valid(&self) -> bool { self.state.valid() }
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]

#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone)]
pub struct Cache<S> {
    pub sets: usize,
    pub ways: usize,
    /// `sets * ways` lines, set-major (`set * ways + way`).
    pub lines: Vec<Line<S>>,
}

impl<S: LineState> Cache<S> {
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

    /// Word index of `pa` within its line.
    pub fn word_offset(pa: u32) -> usize { ((pa >> 2) as usize) % LINE_WORDS }

    pub fn line(&self, set: usize, way: usize) -> &Line<S> { &self.lines[set * self.ways + way] }
    pub fn line_mut(&mut self, set: usize, way: usize) -> &mut Line<S> { &mut self.lines[set * self.ways + way] }

    /// The valid line holding `pa`, if any: (set, way).
    pub fn find(&self, pa: u32) -> Option<(usize, usize)> {
        let (set, tag) = self.split(pa);
        (0..self.ways).find(|&w| { let l = self.line(set, w); l.valid() && l.tag == tag }).map(|w| (set, w))
    }

    /// The word at `pa` if its line is present (no LRU update).
    pub fn peek(&self, pa: u32) -> Option<u32> {
        self.find(pa).map(|(s, w)| self.line(s, w).data[Self::word_offset(pa)])
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

    /// Put the line holding `pa` into the victim way of its set: (set, way).
    pub fn fill(&mut self, pa: u32, data: [u32; LINE_WORDS], state: S) -> (usize, usize) {
        let (set, tag) = self.split(pa);
        let way = self.victim(set);
        *self.line_mut(set, way) = Line { state, tag, lru: 0, data };
        self.touch(set, way);
        (set, way)
    }

    /// Invalidate every line.
    pub fn clear(&mut self) { self.lines.fill(Line::default()); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_mapped_matches_the_bit_fields() {
        let c: Cache<bool> = Cache::new(256, 1);
        for a in [0u32, 0x1234_5678, 0xffff_fff0, 0x1000_0010] {
            assert_eq!(c.split(a), (((a >> 4) & 0xff) as usize, a >> 12));
        }
    }

    #[test]
    fn fill_find_and_lru_replacement() {
        let mut c: Cache<bool> = Cache::new(2, 2);
        let set0 = |i: u32| i * 2 * LINE_BYTES;        // lines that all map to set 0
        c.fill(set0(0), [1, 2, 3, 4], true);
        c.fill(set0(1), [5, 6, 7, 8], true);
        assert_eq!(c.peek(set0(0) + 4), Some(2));
        let (s, w) = c.find(set0(0)).unwrap();
        c.touch(s, w);                                  // line 0 recent → line 1 is the victim
        c.fill(set0(2), [9; 4], true);
        assert!(c.find(set0(1)).is_none());
        assert_eq!(c.peek(set0(0)), Some(1));
        assert_eq!(c.peek(set0(2)), Some(9));
    }
}
