//! Stage 1 — Load Thread.
//!
//! Asks the scheduler which thread to dispatch this cycle, then
//! latches its `pc` into the LT/IF pipeline register.

use super::scheduler::pick_next;
use super::state::{BarrelState, LtIf};

pub fn compute_lt(state: &BarrelState) -> (LtIf, Option<u8>) {
    match pick_next(state) {
        Some(slot) => {
            let t = &state.threads[slot as usize];
            (LtIf { valid: true, thread: slot, pc: t.pc }, Some(slot))
        }
        None => (LtIf::bubble(), None),
    }
}
