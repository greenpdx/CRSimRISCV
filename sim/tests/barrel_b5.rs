//! Phase B5 — scheduler modes + single-thread forwarding fallback.

use cpu_sim::sim::barrel::{BarrelState, N_THREADS, SchedMode};
use cpu_sim::isa::encode::{NOP, SPIN, addi, lui, lw};

// ── Scheduler mode tests ─────────────────────────────────────────

#[test]
fn sched_priority_strict_picks_highest_first() {
    // Slot 0 priority 3, slots 1..7 priority 1. After many cycles
    // slot 0 should account for almost all dispatches.
    let mut cpu = BarrelState::new();
    cpu.sched_mode = SchedMode::Priority;
    for s in 0..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0;
        cpu.threads[s].priority = if s == 0 { 3 } else { 1 };
    }
    let mut rom = vec![NOP; 16];
    rom[0] = SPIN;
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..200 { cpu.step(); }
    // Slot 0 should dominate the schedule history.
    let n_zero = cpu.sched_history.iter().filter(|&&t| t == 0).count();
    let n_other = cpu.sched_history.iter().filter(|&&t| t != 0 && t != 0xff).count();
    assert!(n_zero > n_other, "slot 0 should dominate (got {} vs {})", n_zero, n_other);
}

#[test]
fn sched_aged_priority_eventually_runs_low_priority() {
    // Slot 0 priority 3, slot 5 priority 0, both runnable. Aged
    // priority should give slot 5 at least some dispatches.
    let mut cpu = BarrelState::new();
    cpu.sched_mode = SchedMode::AgedPriority;
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 3;
    cpu.threads[5].active = true;
    cpu.threads[5].pc = 0;
    cpu.threads[5].priority = 0;
    let nop_rom = vec![SPIN; 16];
    cpu.load_program(&nop_rom);
    cpu.prewarm_icache();
    for _ in 0..200 { cpu.step(); }
    let n_five = cpu.sched_history.iter().filter(|&&t| t == 5).count();
    assert!(n_five > 0, "slot 5 should have run at least once with AgedPriority");
}

#[test]
fn sched_two_class_pinned_runs_when_active() {
    // Slots 0/1/2 (pinned) and 3..7 all active. With TwoClass,
    // pinned slots should dominate. Then deactivate slots 0/1/2
    // and verify general slots run.
    let mut cpu = BarrelState::new();
    cpu.sched_mode = SchedMode::TwoClass;
    for s in 0..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0;
        cpu.threads[s].priority = 1;
    }
    let nop_rom = vec![SPIN; 16];
    cpu.load_program(&nop_rom);
    cpu.prewarm_icache();
    for _ in 0..100 { cpu.step(); }
    // History should be all from {0,1,2}.
    for &t in cpu.sched_history.iter() {
        if t != 0xff {
            assert!(t < 3, "TwoClass should only run pinned slots when any is active, got {}", t);
        }
    }
    // Now deactivate the pinned class.
    for s in 0..3 { cpu.threads[s].active = false; }
    // Reset history
    cpu.sched_history = [0xff; 32];
    cpu.sched_history_idx = 0;
    for _ in 0..100 { cpu.step(); }
    // History should now be from {3..7}.
    for &t in cpu.sched_history.iter() {
        if t != 0xff {
            assert!(t >= 3, "TwoClass with no pinned should run general, got {}", t);
        }
    }
}

#[test]
fn sched_round_robin_still_works_as_default() {
    let cpu = BarrelState::new();
    assert_eq!(cpu.sched_mode, SchedMode::RoundRobin);
}

// ── Single-thread forwarding tests ─────────────────────────────

#[test]
fn forward_back_to_back_addi_no_stall_single_thread() {
    // Only thread 0 active. Two dependent addis. Should produce
    // the right result with no spinners — proves EX/MA → EX
    // forwarding works.
    let mut rom = vec![NOP; 16];
    rom[0] = addi(5, 0, 7);       // x5 = 7
    rom[1] = addi(6, 5, 1);       // x6 = x5 + 1 = 8 (depends on x5)
    rom[2] = SPIN;
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..30 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 5), 7);
    assert_eq!(cpu.read_thread_register(0, 6), 8, "forwarding EX/MA → EX must work");
}

#[test]
fn forward_three_dependent_alu_ops() {
    // Chain of 3 dependent ALU ops, single thread. Forwarding
    // should make all 3 retire correctly with no stalls.
    let mut rom = vec![NOP; 16];
    rom[0] = addi(5, 0, 1);       // x5 = 1
    rom[1] = addi(6, 5, 2);       // x6 = x5 + 2 = 3   (depends on x5)
    rom[2] = addi(7, 6, 4);       // x7 = x6 + 4 = 7   (depends on x6)
    rom[3] = SPIN;
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..30 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 5), 1);
    assert_eq!(cpu.read_thread_register(0, 6), 3);
    assert_eq!(cpu.read_thread_register(0, 7), 7, "third dependent ALU op must work");
}

#[test]
fn forward_load_use_stall_then_forward() {
    // Single-thread load-use hazard. The lw value isn't ready
    // until end of MA, so the consumer must stall one cycle and
    // then receive the value via MA/WB → EX forwarding.
    //
    // Note: B5 single-thread only handles load-use with a HOT
    // d-cache. A cold miss in single-thread mode would compound
    // the MA pc-rollback with the stall pc-rollback and lose the
    // load. We work around by pre-installing the cache line.
    let mut rom = vec![NOP; 16];
    rom[0] = lui(6, 0x10000);     // x6 = 0x10000000 (RAM base)
    rom[1] = lw(7, 6, 0);         // x7 = MEM[x6]
    rom[2] = addi(8, 7, 1);       // x8 = x7 + 1   (load-use!)
    rom[3] = SPIN;
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    cpu.dmem_write(0x10000000, 41);
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    // Pre-install the d-cache line so the lw hits.
    cpu.dcache.install(0x10000000, [41, 0, 0, 0]);
    for _ in 0..50 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 6), 0x10000000);
    assert_eq!(cpu.read_thread_register(0, 7), 41);
    assert_eq!(cpu.read_thread_register(0, 8), 42, "load-use stall + forwarding must work");
}
