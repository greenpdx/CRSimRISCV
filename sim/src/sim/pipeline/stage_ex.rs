//! Pipeline stage 3 — Execute (EX).
//!
//! Runs the **forwarding unit** to pick rs1/rs2 sources, then computes
//! the ALU result, the branch target, and (for branches) decides
//! whether the branch is taken. Produces the new EX/MA register
//! contents.
//!
//! Always-active components: `forward_unit`, `mux_fwd_a`, `mux_fwd_b`,
//! `alu_ctrl`, `mux_alusrc`, `alu`, `branch_adder`, `ex_ma_reg`.

use crate::data::alu_control_data::compute_alu_control;
use crate::data::alu_data::alu_compute;
use crate::data::control_data::{branch_condition, BranchCondition};

use super::state::{ExMa, ForwardSrc, IdEx, MaWb};

/// Per-cycle EX outputs: the new EX/MA register, plus the forwarding
/// decisions for visualization.
pub struct ExResult {
    pub ex_ma: ExMa,
    pub forward_a: ForwardSrc,
    pub forward_b: ForwardSrc,
}

/// Forward selector for ALU input A. Picks the most-recent in-flight
/// write to `id_ex.rs1_num`, falling back to the regfile read if no
/// in-flight write matches.
pub fn forward_a(id_ex: &IdEx, ex_ma: &ExMa, ma_wb: &MaWb) -> ForwardSrc {
    if id_ex.rs1_num == 0 { return ForwardSrc::Regfile; }
    if ex_ma.valid && ex_ma.ctrl.reg_write && ex_ma.rd_num != 0
        && ex_ma.rd_num == id_ex.rs1_num
    {
        return ForwardSrc::ExMa;
    }
    if ma_wb.valid && ma_wb.ctrl.reg_write && ma_wb.rd_num != 0
        && ma_wb.rd_num == id_ex.rs1_num
    {
        return ForwardSrc::MaWb;
    }
    ForwardSrc::Regfile
}

/// Same for ALU input B.
pub fn forward_b(id_ex: &IdEx, ex_ma: &ExMa, ma_wb: &MaWb) -> ForwardSrc {
    if id_ex.rs2_num == 0 { return ForwardSrc::Regfile; }
    if ex_ma.valid && ex_ma.ctrl.reg_write && ex_ma.rd_num != 0
        && ex_ma.rd_num == id_ex.rs2_num
    {
        return ForwardSrc::ExMa;
    }
    if ma_wb.valid && ma_wb.ctrl.reg_write && ma_wb.rd_num != 0
        && ma_wb.rd_num == id_ex.rs2_num
    {
        return ForwardSrc::MaWb;
    }
    ForwardSrc::Regfile
}

/// Look up the value to forward from a chosen source.
fn pick_value(src: ForwardSrc, fallback: u32, ex_ma: &ExMa, ma_wb: &MaWb) -> u32 {
    match src {
        ForwardSrc::Regfile => fallback,
        ForwardSrc::ExMa => ex_ma.alu_result,
        ForwardSrc::MaWb => {
            // What MA/WB *would* write back to the regfile next cycle.
            match ma_wb.ctrl.wb_sel {
                0 => ma_wb.alu_result,
                1 => ma_wb.mem_data,
                2 => ma_wb.pc_plus4,
                _ => 0,
            }
        }
    }
}

/// Compute the EX stage outputs for the instruction currently in
/// ID/EX, using the current EX/MA and MA/WB registers as forwarding
/// sources.
pub fn compute_ex(id_ex: &IdEx, ex_ma_in: &ExMa, ma_wb_in: &MaWb) -> ExResult {
    if !id_ex.valid {
        return ExResult {
            ex_ma: ExMa::bubble(),
            forward_a: ForwardSrc::Regfile,
            forward_b: ForwardSrc::Regfile,
        };
    }

    let fa = forward_a(id_ex, ex_ma_in, ma_wb_in);
    let fb = forward_b(id_ex, ex_ma_in, ma_wb_in);
    let rs1_val = pick_value(fa, id_ex.rs1_data, ex_ma_in, ma_wb_in);
    let rs2_val = pick_value(fb, id_ex.rs2_data, ex_ma_in, ma_wb_in);

    let alu_b = if id_ex.ctrl.alu_src { id_ex.imm } else { rs2_val };

    let is_lui = id_ex.opcode == 0b0110111;
    let is_rt  = id_ex.opcode == 0b0110011;
    let is_si  = id_ex.opcode == 0b0010011 && (id_ex.funct3 == 0b101 || id_ex.funct3 == 0b001);
    let f7b30  = if is_rt || is_si { id_ex.funct7b30 } else { 0 };
    let alu_op_code = compute_alu_control(id_ex.ctrl.alu_op, id_ex.funct3, f7b30, is_lui);

    let r = alu_compute(alu_op_code, rs1_val, alu_b);

    // Branch decision
    let mut branch_taken = false;
    let mut branch_target = id_ex.pc.wrapping_add(id_ex.imm);
    if id_ex.ctrl.branch {
        branch_taken = match branch_condition(id_ex.funct3) {
            Some(BranchCondition::Zero)     =>  r.flags.zero,
            Some(BranchCondition::NotZero)  => !r.flags.zero,
            Some(BranchCondition::Neg)      =>  r.flags.negative,
            Some(BranchCondition::NotNeg)   => !r.flags.negative,
            Some(BranchCondition::Carry)    =>  r.flags.carry,
            Some(BranchCondition::NotCarry) => !r.flags.carry,
            None => false,
        };
    }
    if id_ex.ctrl.jump {
        branch_taken = true;
        if id_ex.opcode == 0b1100111 {
            // JALR: target = (rs1 + imm) & ~1
            branch_target = r.result & 0xffff_fffe;
        }
        // For JAL, branch_target = PC + imm (already set above).
    }

    let ex_ma = ExMa {
        valid: true,
        instr: id_ex.instr,
        alu_result: r.result,
        rs2_data: rs2_val,    // for stores: forwarded value
        pc_plus4: id_ex.pc_plus4,
        rd_num: id_ex.rd_num,
        funct3: id_ex.funct3,
        branch_taken,
        branch_target,
        ctrl: id_ex.ctrl,
    };

    ExResult { ex_ma, forward_a: fa, forward_b: fb }
}
