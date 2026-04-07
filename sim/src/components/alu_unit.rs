//! Mirrors `src/components/AluUnit.vue`.
//! Props: alu_control, operand_a, operand_b, active.
//! Computed: op_def = ALU_OP_BY_CODE[alu_control]; result = alu_compute(...).

use crate::data::alu_data::{alu_compute, alu_op_by_code, AluOpDef, AluResult};

#[derive(Debug, Clone, Copy, Default)]
pub struct AluUnit {
    pub alu_control: u8,
    pub operand_a: u32,
    pub operand_b: u32,
    pub active: bool,
}

impl AluUnit {
    pub fn op_def(&self) -> Option<&'static AluOpDef> { alu_op_by_code(self.alu_control) }
    pub fn result(&self) -> AluResult { alu_compute(self.alu_control, self.operand_a, self.operand_b) }
}
