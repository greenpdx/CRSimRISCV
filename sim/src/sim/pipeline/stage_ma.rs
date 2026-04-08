//! Pipeline stage 4 — Memory Access (MA).
//!
//! For loads, reads RAM (or IO) at `alu_result` and applies the
//! load-width sign/zero extension. For stores, writes RAM (or IO).
//! Produces the new MA/WB register contents.
//!
//! Always-active components: `ma_wb_reg`, plus `ram` or `io` and
//! `loadstore` if memory access.

use super::state::{ExMa, MaWb, PipelineState};

/// Compute the MA stage outputs and apply any side effects (RAM/IO
/// writes). Returns the new MA/WB register contents.
pub fn compute_ma(state: &mut PipelineState, ex_ma: &ExMa) -> MaWb {
    if !ex_ma.valid {
        return MaWb::bubble();
    }

    let mut mem_data = 0u32;
    let addr = ex_ma.alu_result;
    let funct3 = ex_ma.funct3;

    if ex_ma.ctrl.mem_read {
        let raw = if PipelineState::is_io(addr) {
            state.io_data
        } else {
            state.ram_read(addr)
        };
        // Apply sign/zero extension based on funct3 (LB/LH/LW/LBU/LHU)
        let byte_off = (addr & 0x3) * 8;
        mem_data = match funct3 {
            0b000 => {  // LB
                let v = (raw >> byte_off) & 0xff;
                if v & 0x80 != 0 { v | 0xffff_ff00 } else { v }
            }
            0b001 => {  // LH
                let v = (raw >> byte_off) & 0xffff;
                if v & 0x8000 != 0 { v | 0xffff_0000 } else { v }
            }
            0b010 => raw,  // LW
            0b100 => (raw >> byte_off) & 0xff,    // LBU
            0b101 => (raw >> byte_off) & 0xffff,  // LHU
            _     => raw,
        };
    }

    if ex_ma.ctrl.mem_write {
        if PipelineState::is_io(addr) {
            state.io_data = ex_ma.rs2_data;
        } else {
            // Sub-word writes are not modeled in the visualizer; treat
            // SB/SH/SW as full word writes for v1.
            state.ram_write(addr, ex_ma.rs2_data);
        }
    }

    MaWb {
        valid: true,
        instr: ex_ma.instr,
        alu_result: ex_ma.alu_result,
        mem_data,
        pc_plus4: ex_ma.pc_plus4,
        rd_num: ex_ma.rd_num,
        ctrl: ex_ma.ctrl,
    }
}
