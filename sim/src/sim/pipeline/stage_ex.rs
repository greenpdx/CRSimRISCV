//! Pipeline stage 3 — Execute (EX).
//!
//! Runs the **forwarding unit** to pick rs1/rs2 sources, then computes
//! the ALU result, the branch target, and (for branches) decides
//! whether the branch is taken. Produces the new EX/MA register
//! contents.
//!
//! Always-active components: `forward_unit`, `mux_fwd_a`, `mux_fwd_b`,
//! `alu_ctrl`, `mux_alusrc`, `alu`, `branch_adder`, `ex_ma_reg`.

use crate::sim::execute::{execute_alu, decide_branch};

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
                3 => ma_wb.mem_data,
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
        let mut bubble = ExMa::bubble();
        bubble.stall_reg = id_ex.stall_reg;
        bubble.flush_bubble = id_ex.flush_bubble;
        bubble.early_redirected = id_ex.id_redirected;
        return ExResult {
            ex_ma: bubble,
            forward_a: ForwardSrc::Regfile,
            forward_b: ForwardSrc::Regfile,
        };
    }

    let fa = forward_a(id_ex, ex_ma_in, ma_wb_in);
    let fb = forward_b(id_ex, ex_ma_in, ma_wb_in);
    let rs1_val = pick_value(fa, id_ex.rs1_data, ex_ma_in, ma_wb_in);
    let rs2_val = pick_value(fb, id_ex.rs2_data, ex_ma_in, ma_wb_in);

    let alu_b = if id_ex.ctrl.alu_src { id_ex.imm } else { rs2_val };
    let r = execute_alu(rs1_val, alu_b, id_ex.pc, id_ex.ctrl.alu_op, id_ex.funct3, id_ex.funct7b30, id_ex.opcode);

    let mut branch_taken = false;
    let mut branch_target = id_ex.pc.wrapping_add(id_ex.imm);
    if id_ex.ctrl.branch {
        branch_taken = decide_branch(id_ex.funct3, &r.flags);
    }
    if id_ex.ctrl.jump {
        branch_taken = true;
        if id_ex.opcode == 0b1100111 {
            // JALR: target = (rs1 + imm) & ~1
            branch_target = r.result & 0xffff_fffe;
        }
        // For JAL, branch_target = PC + imm (already set above).
    }

    // CSR write source: zero-extended uimm for the immediate-form ops,
    // otherwise the (forwarded) rs1 value.
    let csr_write_val = if id_ex.csr_imm_form { id_ex.csr_uimm as u32 } else { rs1_val };
    let csr_src_is_zero = if id_ex.csr_imm_form { id_ex.csr_uimm == 0 } else { id_ex.rs1_num == 0 };

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
        csr_addr: id_ex.csr_addr,
        csr_write_val,
        csr_src_is_zero,
        trap_kind: id_ex.trap_kind,
        trap_pc: id_ex.pc,
        stall_reg: 0,
        flush_bubble: false,
        early_redirected: id_ex.id_redirected,
    };

    ExResult { ex_ma, forward_a: fa, forward_b: fb }
}
