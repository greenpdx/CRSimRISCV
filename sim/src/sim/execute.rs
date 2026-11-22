//! ALU control, ALU and branch decision of the single-cycle core, the 5-stage
//! pipeline and the barrel processor (decoder-table cores; flag-based branches).

use crate::data::alu_control_data::compute_alu_control;
use crate::data::alu_data::{alu_compute, AluFlags, AluResult};
use crate::isa::branch::{taken_by_flags, Cond};

/// ALU control code from the decoder's ALUOp and the instruction: funct7[5]
/// (instr[30]) only counts for R-type and the shift immediates.
pub fn alu_control_code(alu_op: u8, funct3: u8, funct7b30: u8, opcode: u8) -> u8 {
    let is_lui = opcode == 0b0110111;
    let is_rt  = opcode == 0b0110011;
    let is_si  = opcode == 0b0010011 && (funct3 == 0b101 || funct3 == 0b001);
    let f7b30  = if is_rt || is_si { funct7b30 } else { 0 };
    compute_alu_control(alu_op, funct3, f7b30, is_lui)
}

/// Run the ALU: compute the control code from decoder signals, then
/// execute the operation. Returns the full AluResult (result + flags).
///
/// ALU input A is rs1, except for AUIPC where it is the instruction's PC.
pub fn execute_alu(
    rs1_val: u32, alu_b: u32, pc: u32,
    alu_op: u8, funct3: u8, funct7b30: u8, opcode: u8,
) -> AluResult {
    let alu_a = if crate::sim::decode::is_auipc(opcode) { pc } else { rs1_val };
    alu_compute(alu_control_code(alu_op, funct3, funct7b30, opcode), alu_a, alu_b)
}

/// Branch decision from the flags of `rs1 - rs2` (branches force ALU SUB).
/// funct3 010 / 011 are not branches: not taken.
pub fn decide_branch(funct3: u8, flags: &AluFlags) -> bool {
    match Cond::from_funct3(funct3) {
        Some(c) => taken_by_flags(c, flags.of_sub()),
        None => false,
    }
}
