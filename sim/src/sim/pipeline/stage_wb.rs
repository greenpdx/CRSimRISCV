//! Pipeline stage 5 — Write Back (WB).
//!
//! Selects the write-back source via `wb_sel` and writes the result
//! into the register file (if `reg_write` is set and `rd_num != 0`).
//!
//! Always-active components: `mux_wb`, `regfile` (write port).

use super::state::{MaWb, PipelineState};

/// Apply the WB stage to the regfile. Reads from MA/WB and modifies
/// `state.registers` directly.
pub fn compute_wb(state: &mut PipelineState, ma_wb: &MaWb) {
    if !ma_wb.valid || !ma_wb.ctrl.reg_write || ma_wb.rd_num == 0 {
        return;
    }
    let value = match ma_wb.ctrl.wb_sel {
        0 => ma_wb.alu_result,
        1 => ma_wb.mem_data,
        2 => ma_wb.pc_plus4,
        3 => ma_wb.mem_data,  // CSR old value (Zicsr)
        _ => 0,
    };
    state.write_register(ma_wb.rd_num, value);
    if ma_wb.rd_num != 0 {
        state.reg_write_mask |= 1 << ma_wb.rd_num;
    }
}
