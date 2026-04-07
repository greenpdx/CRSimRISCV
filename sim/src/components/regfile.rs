//! Mirrors `src/components/RegisterFile.vue` (state-only, no toggle UI).
//! 32 RV32I registers; x0 is hard-wired zero.

#[derive(Debug, Clone, Copy)]
pub struct Regfile { regs: [u32; 32] }

impl Default for Regfile {
    fn default() -> Self { Self { regs: [0; 32] } }
}

impl Regfile {
    pub fn new() -> Self { Self::default() }

    pub fn read(&self, idx: u8) -> u32 { self.regs[idx as usize & 0x1f] }

    /// Write enforces x0 = 0 at the boundary; callers cannot bypass it.
    pub fn write(&mut self, idx: u8, val: u32) {
        let i = idx as usize & 0x1f;
        if i != 0 { self.regs[i] = val; }
    }

    /// Read-only snapshot for display purposes.
    pub fn snapshot(&self) -> [u32; 32] { self.regs }

    /// Debugger poke: force a register value at runtime, bypassing the
    /// normal write path. x0 is still pinned to 0 so the invariant holds.
    pub fn poke(&mut self, idx: u8, val: u32) {
        let i = idx as usize & 0x1f;
        if i != 0 { self.regs[i] = val; }
    }
}
