//! Mirrors `src/components/PcRegister.vue`.
//! Props: pc, next_pc, active.

#[derive(Debug, Clone, Copy, Default)]
pub struct PcRegister {
    pub pc: u32,
    pub next_pc: u32,
    pub active: bool,
}

impl PcRegister {
    /// Mirrors `fmtHex` in PcRegister.vue.
    pub fn fmt_hex(v: u32) -> String { format!("0x{:08x}", v) }

    /// Latch next_pc into pc (clock edge).
    pub fn tick(&mut self) { self.pc = self.next_pc; }
}
