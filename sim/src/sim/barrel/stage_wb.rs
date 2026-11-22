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
    let value = ma_wb.wb_value();
    let slot = ma_wb.thread as usize;
    let rd = ma_wb.rd_num as usize;
    if rd != 0 && rd < 32 && slot < state.threads.len() {
        state.threads[slot].regs[rd] = value;
    }
}
