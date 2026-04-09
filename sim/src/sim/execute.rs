//! Shared ALU execution and branch decision logic used by both the
//! 5-stage pipeline and the barrel processor's EX stage.

use crate::data::alu_control_data::compute_alu_control;
use crate::data::alu_data::{alu_compute, AluResult};
use crate::data::control_data::{branch_condition, BranchCondition};

/// Run the ALU: compute the control code from decoder signals, then
/// execute the operation. Returns the full AluResult (result + flags).
pub fn execute_alu(
    rs1_val: u32, alu_b: u32,
    alu_op: u8, funct3: u8, funct7b30: u8, opcode: u8,
) -> AluResult {
    let is_lui = opcode == 0b0110111;
    let is_rt  = opcode == 0b0110011;
    let is_si  = opcode == 0b0010011 && (funct3 == 0b101 || funct3 == 0b001);
    let f7b30  = if is_rt || is_si { funct7b30 } else { 0 };
    let alu_op_code = compute_alu_control(alu_op, funct3, f7b30, is_lui);
    alu_compute(alu_op_code, rs1_val, alu_b)
}

/// Evaluate a branch condition against ALU flags. Returns true if
/// the branch should be taken.
pub fn decide_branch(funct3: u8, flags: &crate::data::alu_data::AluFlags) -> bool {
    match branch_condition(funct3) {
        Some(BranchCondition::Zero)     =>  flags.zero,
        Some(BranchCondition::NotZero)  => !flags.zero,
        Some(BranchCondition::Neg)      =>  flags.negative,
        Some(BranchCondition::NotNeg)   => !flags.negative,
        Some(BranchCondition::Carry)    =>  flags.carry,
        Some(BranchCondition::NotCarry) => !flags.carry,
        None => false,
    }
}
