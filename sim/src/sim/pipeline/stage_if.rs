//! Pipeline stage 1 — Instruction Fetch (IF).
//!
//! Reads the instruction word from ROM at the current PC, computes
//! PC + 4, and produces the new IF/ID register contents (unless
//! flushed/stalled, in which case the caller substitutes a bubble).
//!
//! Always-active components: `pc`, `rom`, `pc_adder`, `if_id_reg`.

use super::state::{IfId, IfIdData, PipelineState, TrapKind};
use super::translate::{translate, AccessKind};

/// Compute what the IF/ID pipeline register *should* hold after this
/// cycle. Performs Sv32 translation if enabled. On a translation
/// fault, returns an IfId carrying `TrapKind::InstrPageFault` (instruction page
/// fault); the rest of the pipeline propagates it like any other
/// synchronous trap.
pub fn compute_if(state: &mut PipelineState) -> IfId {
    let pc = state.pc;
    let pc_plus4 = pc.wrapping_add(4);
    match translate(state, pc, AccessKind::Fetch) {
        Ok(phys) => {
            let instr = state.rom_read(phys);
            IfId::new(pc, instr, IfIdData { pc_plus4, ..IfIdData::default() })
        }
        Err(_cause) => {
            IfId::new(pc, 0, IfIdData { pc_plus4, trap_kind: TrapKind::InstrPageFault, trap_value: pc, flush_bubble: false })
        }
    }
}
