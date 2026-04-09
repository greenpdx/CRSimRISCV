//! Stage 6 — Write Back.
//!
//! Writes the selected value into the per-thread regfile. The
//! `thread` tag carried by the MaWb register tells WB which slot's
//! regfile to write.

use super::state::{BarrelState, MaWb};

pub fn compute_wb(state: &mut BarrelState, ma_wb: &MaWb) {
    if !ma_wb.valid || !ma_wb.ctrl.reg_write || ma_wb.rd_num == 0 {
        return;
    }
    let value = match ma_wb.ctrl.wb_sel {
        0 => ma_wb.alu_result,
        1 => ma_wb.mem_data,
        2 => ma_wb.pc_plus4,
        _ => 0,
    };
    let slot = ma_wb.thread as usize;
    let rd = ma_wb.rd_num as usize;
    if rd != 0 && rd < 32 && slot < state.threads.len() {
        state.threads[slot].regs[rd] = value;
    }
}
