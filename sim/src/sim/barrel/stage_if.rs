//! Stage 2 — Instruction Fetch.
//!
//! Phase B3: route between three fetch sources:
//!
//!   1. Local I-SRAM (slots 0/1/2 only, single-cycle, never misses)
//!   2. External instruction memory (any slot, B4 will route this
//!      through an I-L1 cache)
//!   3. **Fault** if a slot tries to fetch from another slot's
//!      I-SRAM range, or any slot ≥ 3 fetches from any I-SRAM
//!      range. The faulting thread is deactivated and a flag is
//!      set on the thread for later inspection.

use super::cache::{Cache, PendingMiss};
use super::state::{BarrelState, IfDi, ISRAM_BASE, ISRAM_BYTES_PER_SLOT, IfSrc, LtIf};

pub fn compute_if(state: &mut BarrelState, lt_if: &LtIf) -> IfDi {
    if !lt_if.valid {
        return IfDi::bubble();
    }
    let pc = lt_if.pc;
    let thread = lt_if.thread;

    // ── Local I-SRAM (Phase B3) ──────────────────────────────
    if let Some(target_slot) = BarrelState::isram_slot_of(pc) {
        if target_slot == thread && (thread as usize) < 3 {
            let off = pc - ISRAM_BASE - (target_slot as u32) * ISRAM_BYTES_PER_SLOT;
            let instr = state.isram_read(target_slot, off);
            return IfDi {
                valid: true, thread, pc, instr,
                src: IfSrc::LocalIsram,
            };
        } else {
            let t = &mut state.threads[thread as usize];
            t.instr_access_fault = true;
            t.active = false;
            return IfDi::bubble();
        }
    }

    // ── I-L1 cache for external instruction memory (Phase B4) ──
    if let Some(instr) = state.icache.lookup(pc) {
        return IfDi {
            valid: true, thread, pc, instr,
            src: IfSrc::ICacheHit,
        };
    }

    // I-cache miss. Either start a new fill (cache idle) or join
    // an existing fill (cache busy with a pending miss).
    let line_addr = Cache::line_addr(pc);
    if state.icache.pending.is_none() {
        // Start a new fill. The bus arbiter will service it.
        state.icache.pending = Some(PendingMiss { line_addr, thread });
    }
    // Either way, this thread is now waiting on the I-cache.
    state.icache.waiters[thread as usize] = true;
    state.threads[thread as usize].miss_pending = true;

    IfDi {
        valid: false,                // bubble for this cycle
        thread,
        pc,
        instr: 0,
        src: IfSrc::ICacheMiss,
    }
}
