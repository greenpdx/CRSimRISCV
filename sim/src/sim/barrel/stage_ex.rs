//! Stage 4 — Execute.
//!
//! Runs the ALU and resolves branches/jumps, with the shared
//! forwarding unit (`sim::forwarding`) restricted to the same thread.

use crate::sim::execute::{execute_alu, decide_branch};
use crate::sim::forwarding::{forward, forwarded};
use super::state::{CsrOp, CustomOp, DiEx, ExMa, ExMaData, MaWb};

pub fn compute_ex(di_ex: &DiEx, ex_ma_in: &ExMa, ma_wb_in: &MaWb) -> ExMa {
    if !di_ex.valid {
        return ExMa::bubble();
    }

    // Forwarding from in-flight stages — same thread only (separate
    // register files): in multi-thread mode adjacent stages hold other
    // threads, so this returns the regfile; single-thread mode is where it
    // does real work. Applied BEFORE the custom-0 check so FORK/JOIN/etc.
    // see forwarded register values in single-thread boot mode.
    let fa = forward(di_ex.rs1_num, di_ex.thread, ex_ma_in, ma_wb_in);
    let fb = forward(di_ex.rs2_num, di_ex.thread, ex_ma_in, ma_wb_in);
    let rs1_val = forwarded(fa, di_ex.rs1_data, ex_ma_in, ma_wb_in);
    let rs2_val = forwarded(fb, di_ex.rs2_data, ex_ma_in, ma_wb_in);

    // Custom-0 instructions skip the ALU entirely. The actual
    // semantics run in stage_ma where we have &mut state. Here we
    // propagate the FORWARDED rs1/rs2 values.
    // Propagate trap (or custom-0) without ALU work.
    if di_ex.trap_kind.is_some() || di_ex.ctrl.custom_op != CustomOp::None || di_ex.csr_op != CsrOp::None {
        let csr_write_val = if di_ex.csr_imm_form { di_ex.csr_uimm as u32 } else { rs1_val };
        let csr_src_is_zero = if di_ex.csr_imm_form { di_ex.csr_uimm == 0 } else { di_ex.rs1_num == 0 };
        return ExMa::new(di_ex.pc, di_ex.instr, ExMaData {
            thread: di_ex.thread,
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
            csr_op: di_ex.csr_op,
            csr_addr: di_ex.csr_addr,
            csr_write_val,
            csr_src_is_zero,
        });
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

    ExMa::new(di_ex.pc, di_ex.instr, ExMaData {
        thread: di_ex.thread,
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
        ..ExMaData::default()   // no trap, no CSR op
    })
}
