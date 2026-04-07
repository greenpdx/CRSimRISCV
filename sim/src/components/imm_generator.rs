//! Mirrors `src/components/ImmGenerator.vue`.
//! Props: instruction, imm_sel, active. Computed: immediate = extract_immediate(...).

use crate::data::imm_data::{extract_immediate, ImmSel};

#[derive(Debug, Clone, Copy, Default)]
pub struct ImmGenerator {
    pub instruction: u32,
    pub imm_sel: u8,
    pub active: bool,
}

impl ImmGenerator {
    pub fn immediate(&self) -> i32 {
        extract_immediate(self.instruction, ImmSel::from_u8(self.imm_sel))
    }
}
