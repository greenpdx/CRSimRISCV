//! Stage 4 — Execute.
//!
//! Runs the ALU and resolves branches/jumps. No forwarding here:
//! when ≥ 2 threads are active, instructions from the same thread
//! are always 8 cycles apart, so a producer's WB has already
//! completed before its dependent's DI runs. The single-thread
//! degenerate case is broken in B1 and gets forwarding/stalls in
//! a later phase.

use crate::sim::execute::{execute_alu, decide_branch};
use crate::sim::forwarding::ForwardSrc;
use super::state::{CustomOp, DiEx, ExMa, MaWb};

/// Pick the right forwarding source for one rs operand. Only fires
/// when the in-flight producer is from the SAME thread as the
/// consumer (cross-thread forwarding is impossible — separate
/// register files). In multi-thread barrel mode this naturally
/// returns Regfile because adjacent stages always hold different
/// threads. Single-thread mode is where it does real work.
fn forward_for(rs_num: u8, thread: u8, ex_ma: &ExMa, ma_wb: &MaWb) -> ForwardSrc {
    if rs_num == 0 { return ForwardSrc::Regfile; }
    if ex_ma.valid && ex_ma.thread == thread && ex_ma.ctrl.reg_write
       && ex_ma.rd_num == rs_num && ex_ma.rd_num != 0 {
        return ForwardSrc::ExMa;
    }
    if ma_wb.valid && ma_wb.thread == thread && ma_wb.ctrl.reg_write
       && ma_wb.rd_num == rs_num && ma_wb.rd_num != 0 {
        return ForwardSrc::MaWb;
    }
    ForwardSrc::Regfile
}

fn pick_value(src: ForwardSrc, fallback: u32, ex_ma: &ExMa, ma_wb: &MaWb) -> u32 {
    match src {
        ForwardSrc::Regfile => fallback,
        ForwardSrc::ExMa    => ex_ma.alu_result,
        ForwardSrc::MaWb    => match ma_wb.ctrl.wb_sel {
            0 => ma_wb.alu_result,
            1 => ma_wb.mem_data,
            2 => ma_wb.pc_plus4,
            _ => 0,
        },
    }
}

pub fn compute_ex(di_ex: &DiEx, ex_ma_in: &ExMa, ma_wb_in: &MaWb) -> ExMa {
    if !di_ex.valid {
        return ExMa::bubble();
    }

    // Forwarding from in-flight stages (same-thread only).
    // Applied BEFORE the custom-0 check so FORK/JOIN/etc. see
    // forwarded register values in single-thread boot mode.
    let fa = forward_for(di_ex.rs1_num, di_ex.thread, ex_ma_in, ma_wb_in);
    let fb = forward_for(di_ex.rs2_num, di_ex.thread, ex_ma_in, ma_wb_in);
    let rs1_val = pick_value(fa, di_ex.rs1_data, ex_ma_in, ma_wb_in);
    let rs2_val = pick_value(fb, di_ex.rs2_data, ex_ma_in, ma_wb_in);

    // Custom-0 instructions skip the ALU entirely. The actual
    // semantics run in stage_ma where we have &mut state. Here we
    // propagate the FORWARDED rs1/rs2 values.
    // Propagate trap (or custom-0) without ALU work.
    if di_ex.trap_kind != 0 || di_ex.ctrl.custom_op != CustomOp::None || di_ex.csr_op != 0 {
        let csr_write_val = if di_ex.csr_imm_form { di_ex.csr_uimm as u32 } else { rs1_val };
        let csr_src_is_zero = if di_ex.csr_imm_form { di_ex.csr_uimm == 0 } else { di_ex.rs1_num == 0 };
        return ExMa {
            valid: true,
            thread: di_ex.thread,
            instr: di_ex.instr,
            alu_result: 0,
            rs1_data: rs1_val,
            rs2_data: rs2_val,
            pc_plus4: di_ex.pc_plus4,
            rd_num: di_ex.rd_num,
            funct3: di_ex.funct3,
            branch_taken: false,
            branch_target: 0,
            ctrl: di_ex.ctrl,
            trap_kind: di_ex.trap_kind,
            trap_pc: di_ex.pc,
            csr_op: di_ex.csr_op,
            csr_addr: di_ex.csr_addr,
            csr_write_val,
            csr_src_is_zero,
        };
    }

    let alu_b = if di_ex.ctrl.alu_src { di_ex.imm } else { rs2_val };
    let r = execute_alu(rs1_val, alu_b, di_ex.pc, di_ex.ctrl.alu_op, di_ex.funct3, di_ex.funct7b30, di_ex.opcode);

    let mut branch_taken = false;
    let mut branch_target = di_ex.pc.wrapping_add(di_ex.imm);
    if di_ex.ctrl.branch {
        branch_taken = decide_branch(di_ex.funct3, &r.flags);
    }
    if di_ex.ctrl.jump {
        branch_taken = true;
        if di_ex.opcode == 0b1100111 {
            branch_target = r.result & 0xffff_fffe; // JALR
        }
    }

    ExMa {
        valid: true,
        thread: di_ex.thread,
        instr: di_ex.instr,
        alu_result: r.result,
        rs1_data: rs1_val,
        // Stores read rs2 as the value to write. Forward it too.
        rs2_data: rs2_val,
        pc_plus4: di_ex.pc_plus4,
        rd_num: di_ex.rd_num,
        funct3: di_ex.funct3,
        branch_taken,
        branch_target,
        ctrl: di_ex.ctrl,
        trap_kind: 0,
        trap_pc: 0,
        csr_op: 0, csr_addr: 0, csr_write_val: 0, csr_src_is_zero: false,
    }
}
