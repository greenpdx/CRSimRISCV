//! Stage 1 — Load Thread.
//!
//! Asks the scheduler which thread to dispatch this cycle, then
//! latches its `pc` into the LT/IF pipeline register.

use super::scheduler::pick_next;
use super::state::{BarrelState, LtIf, LtIfData};

pub fn compute_lt(state: &BarrelState) -> (LtIf, Option<u8>) {
    match pick_next(state) {
        Some(slot) => {
            let t = &state.threads[slot as usize];
            (LtIf::new(t.pc, 0, LtIfData { thread: slot }), Some(slot))
        }
        None => (LtIf::bubble(), None),
    }
}
