//! Thread scheduler for the barrel processor.

use super::state::{BarrelState, N_THREADS, SchedMode};

pub fn pick_next(state: &BarrelState) -> Option<u8> {
    match state.sched_mode {
        SchedMode::RoundRobin   => round_robin(state),
        SchedMode::Priority     => priority_strict(state),
        SchedMode::AgedPriority => aged_priority(state),
        SchedMode::TwoClass     => two_class(state),
    }
}

fn is_runnable(state: &BarrelState, slot: usize) -> bool {
    let t = &state.threads[slot];
    t.active && t.waiting < 0 && !t.miss_pending && !t.yield_pulse
}

fn round_robin(state: &BarrelState) -> Option<u8> {
    let start = state.current_thread as usize;
    for offset in 1..=N_THREADS {
        let cand = (start + offset) % N_THREADS;
        if is_runnable(state, cand) {
            return Some(cand as u8);
        }
    }
    None
}

/// Strict priority + RR within the highest runnable priority.
fn priority_strict(state: &BarrelState) -> Option<u8> {
    // Find the highest priority among runnable threads.
    let mut max_pri: i16 = -1;
    for s in 0..N_THREADS {
        if is_runnable(state, s) {
            let p = state.threads[s].priority as i16;
            if p > max_pri { max_pri = p; }
        }
    }
    if max_pri < 0 { return None; }
    // Round-robin within that priority class.
    let start = state.current_thread as usize;
    for offset in 1..=N_THREADS {
        let cand = (start + offset) % N_THREADS;
        if is_runnable(state, cand) && state.threads[cand].priority as i16 == max_pri {
            return Some(cand as u8);
        }
    }
    None
}

/// Effective priority = `priority + min(age, 8)`. Highest wins;
/// RR tiebreak. Threads that have been starved out by a high-pri
/// thread eventually rise above it as their age accumulates.
fn aged_priority(state: &BarrelState) -> Option<u8> {
    let start = state.current_thread as usize;
    let mut best: Option<u8> = None;
    let mut best_eff: i32 = -1;
    for offset in 1..=N_THREADS {
        let cand = (start + offset) % N_THREADS;
        if !is_runnable(state, cand) { continue; }
        let t = &state.threads[cand];
        let eff = (t.priority as i32) + ((t.age as i32).min(8));
        if eff > best_eff {
            best_eff = eff;
            best = Some(cand as u8);
        }
    }
    best
}

/// Two-class: try the pinned class (slots 0/1/2) first; if any is
/// runnable, RR among them. Otherwise RR among the general class
/// (slots 3..7).
fn two_class(state: &BarrelState) -> Option<u8> {
    let start = state.current_thread as usize;
    // Pass 1: pinned class
    for offset in 1..=N_THREADS {
        let cand = (start + offset) % N_THREADS;
        if cand < 3 && is_runnable(state, cand) {
            return Some(cand as u8);
        }
    }
    // Pass 2: general class
    for offset in 1..=N_THREADS {
        let cand = (start + offset) % N_THREADS;
        if cand >= 3 && is_runnable(state, cand) {
            return Some(cand as u8);
        }
    }
    None
}
