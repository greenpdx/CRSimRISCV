//! Mirrors `src/components/RamBlock.vue` (state-only).
//! Word-addressed; base 0x00010000 in cpuState.ts.

#[derive(Debug, Default, Clone)]
pub struct Ram {
    words: Vec<u32>,
    base: u32,
}

impl Ram {
    pub fn new(size_words: usize, base: u32) -> Self {
        Self { words: vec![0; size_words], base }
    }

    pub fn base(&self) -> u32 { self.base }
    pub fn len_words(&self) -> usize { self.words.len() }

    fn idx(&self, addr: u32) -> Option<usize> {
        let aligned = addr & !3;
        if aligned < self.base { return None; }
        let i = ((aligned - self.base) >> 2) as usize;
        if i < self.words.len() { Some(i) } else { None }
    }

    pub fn read(&self, addr: u32) -> u32 {
        self.idx(addr).map(|i| self.words[i]).unwrap_or(0)
    }

    pub fn write(&mut self, addr: u32, value: u32) {
        if let Some(i) = self.idx(addr) { self.words[i] = value; }
    }

    pub fn snapshot(&self) -> &[u32] { &self.words }

    /// Debugger poke by **word index** (not byte address). Bounds-checked, silent
    /// on out-of-range. Use this to edit RAM mid-run from a UI/debugger.
    pub fn poke_word(&mut self, word_idx: usize, val: u32) {
        if word_idx < self.words.len() { self.words[word_idx] = val; }
    }

    /// Debugger poke by **byte address**, alignment-forced.
    pub fn poke_addr(&mut self, addr: u32, val: u32) {
        if let Some(i) = self.idx(addr) { self.words[i] = val; }
    }
}
