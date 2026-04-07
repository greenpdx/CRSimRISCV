//! Mirrors `src/components/PcAdder.vue`.
//! Props: pc, offset, active. Computed: result = pc + offset.

#[derive(Debug, Clone, Copy, Default)]
pub struct PcAdder {
    pub pc: u32,
    pub offset: u32,
    pub active: bool,
}

impl PcAdder {
    pub fn result(&self) -> u32 { self.pc.wrapping_add(self.offset) }
}
