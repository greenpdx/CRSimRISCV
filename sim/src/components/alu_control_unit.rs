//! Mirrors `src/components/AluControlUnit.vue`.
//! Props: alu_op (0|1|2), funct3, funct7b30, is_lui, active.
//! Computed: alu_control_out = compute_alu_control(...).

use crate::data::alu_control_data::compute_alu_control;

#[derive(Debug, Clone, Copy, Default)]
pub struct AluControlUnit {
    pub alu_op: u8,
    pub funct3: u8,
    pub funct7b30: u8,
    pub is_lui: bool,
    pub active: bool,
}

impl AluControlUnit {
    /// Computed: 4-bit aluControl output.
    pub fn alu_control_out(&self) -> u8 {
        compute_alu_control(self.alu_op, self.funct3, self.funct7b30, self.is_lui)
    }
}
