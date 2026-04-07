//! Mirrors `src/components/RomBlock.vue` (state-only).
//! Instruction memory; base 0x00000000.

#[derive(Debug, Default, Clone)]
pub struct Rom {
    words: Vec<u32>,
    base: u32,
}

impl Rom {
    pub fn new(size_words: usize, base: u32) -> Self {
        Self { words: vec![0; size_words], base }
    }

    pub fn base(&self) -> u32 { self.base }
    pub fn len_words(&self) -> usize { self.words.len() }

    pub fn read(&self, addr: u32) -> u32 {
        if addr < self.base { return 0; }
        let i = ((addr - self.base) >> 2) as usize;
        self.words.get(i).copied().unwrap_or(0)
    }

    pub fn load(&mut self, words: &[u32]) {
        for (i, w) in words.iter().enumerate() {
            if i < self.words.len() { self.words[i] = *w; }
        }
    }

    pub fn snapshot(&self) -> &[u32] { &self.words }

    /// Debugger poke by **word index** — patch a single instruction in place.
    pub fn poke_word(&mut self, word_idx: usize, val: u32) {
        if word_idx < self.words.len() { self.words[word_idx] = val; }
    }

    /// Debugger poke by **byte address**, alignment-forced.
    pub fn poke_addr(&mut self, addr: u32, val: u32) {
        if addr < self.base { return; }
        let i = ((addr - self.base) >> 2) as usize;
        if i < self.words.len() { self.words[i] = val; }
    }
}
