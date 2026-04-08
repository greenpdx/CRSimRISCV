//! Pipeline stage 1 — Instruction Fetch (IF).
//!
//! Reads the instruction word from ROM at the current PC, computes
//! PC + 4, and produces the new IF/ID register contents (unless
//! flushed/stalled, in which case the caller substitutes a bubble).
//!
//! Always-active components: `pc`, `rom`, `pc_adder`, `if_id_reg`.

use super::state::{IfId, PipelineState};

/// Compute what the IF/ID pipeline register *should* hold after this
/// cycle. The caller (`PipelineState::step`) decides whether to
/// actually latch this value, replace it with a bubble, or freeze the
/// existing IF/ID register (on a stall).
pub fn compute_if(state: &PipelineState) -> IfId {
    let pc = state.pc;
    let instr = state.rom_read(pc);
    let pc_plus4 = pc.wrapping_add(4);
    IfId { valid: true, pc, instr, pc_plus4 }
}
