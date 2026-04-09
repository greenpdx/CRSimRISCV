//! Phase B1 smoke tests for the 8-thread barrel processor.
//!
//! Verifies the bare-bones round-robin pipeline: 6 stages, 8
//! threads, no caches, no custom-0, no scheduler tricks. Just
//! "dispatch one thread per cycle in round-robin order, run its
//! instruction, write back to its slot's regfile."

use cpu_sim::sim::barrel::{BarrelState, EXT_DMEM_BASE, N_THREADS};

/// Convenience: build a barrel CPU with all 8 threads active and
/// each thread's PC pre-set to its own start address.
fn make_8_thread_cpu(starts: &[u32; 8]) -> BarrelState {
    let mut cpu = BarrelState::new();
    cpu.activate_all();
    for i in 0..N_THREADS {
        cpu.set_thread_pc(i as u8, starts[i]);
    }
    cpu
}

#[test]
fn barrel_round_robin_dispatches_each_thread_in_order() {
    // Each thread runs `addi x5, x0, N+1` followed by `jal x0, 0`
    // (spin) so x5 is written once and never overwritten.
    //
    // Layout (8 bytes per thread):
    //   0x00:  addi x5, x0, 1   (thread 0)
    //   0x04:  jal  x0, 0
    //   0x08:  addi x5, x0, 2   (thread 1)
    //   0x0c:  jal  x0, 0
    //   ...
    let nop = 0x00000013u32;
    let jal_spin = 0x0000006fu32;
    let mut rom = vec![nop; 32];
    for i in 0..N_THREADS {
        let base = i * 2;
        let k = i as u32 + 1;
        rom[base + 0] = (k << 20) | (5 << 7) | 0x13; // addi x5, x0, k
        rom[base + 1] = jal_spin;
    }
    let mut cpu = BarrelState::new();
    cpu.activate_all();
    for i in 0..N_THREADS {
        cpu.set_thread_pc(i as u8, (i as u32) * 8);
    }
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..40 { cpu.step(); }

    for i in 0..N_THREADS {
        assert_eq!(
            cpu.read_thread_register(i as u8, 5),
            i as u32 + 1,
            "thread {} expected x5 = {}", i, i + 1
        );
    }
}

#[test]
fn barrel_dispatch_history_is_round_robin() {
    // All 8 threads run NOPs from the same offset. Verify the
    // scheduler dispatch history is 0,1,2,3,4,5,6,7,0,1,...
    let mut cpu = make_8_thread_cpu(&[0; 8]);
    // Pre-load some NOPs
    let nop = 0x00000013u32;
    cpu.load_program(&vec![nop; 16]);
    cpu.prewarm_icache();

    // Step exactly 16 cycles and check the history.
    for _ in 0..16 { cpu.step(); }

    // sched_history is a ring buffer. After 16 cycles starting at
    // current_thread = 7 (default), the LT picks 0 first, then 1,
    // 2, ... 7, 0, 1, ... 7. Verify each successive entry is +1.
    let n = N_THREADS as u8;
    let start = (cpu.sched_history_idx as usize + cpu.sched_history.len() - 16) % cpu.sched_history.len();
    for k in 0..16 {
        let idx = (start + k) % cpu.sched_history.len();
        let expected = (k as u8) % n;
        assert_eq!(
            cpu.sched_history[idx], expected,
            "history[{}] expected T{}, got T{}", k, expected, cpu.sched_history[idx]
        );
    }
}

#[test]
fn barrel_inactive_slot_is_skipped() {
    // Activate all threads except slot 5. Verify slot 5 never gets
    // dispatched even after many cycles.
    let mut cpu = make_8_thread_cpu(&[0; 8]);
    cpu.threads[5].active = false;
    let nop = 0x00000013u32;
    cpu.load_program(&vec![nop; 16]);
    cpu.prewarm_icache();

    for _ in 0..50 { cpu.step(); }

    for &tid in cpu.sched_history.iter() {
        assert_ne!(tid, 5, "inactive slot 5 should never be dispatched");
    }
}

#[test]
fn barrel_eight_threads_each_compute_independently() {
    // Each thread runs a tiny program in its own region of ROM:
    //   addi t0, x0, K
    //   addi t1, t0, K
    // After enough cycles, t0 = K, t1 = 2*K for each thread.
    let mut rom = vec![0u32; 64];
    for i in 0..8 {
        let base = i * 4;
        let k = i as u32 + 1;
        // addi x5, x0, k          ; rd=5
        rom[base + 0] = (k << 20) | (5 << 7) | 0x13;
        // addi x6, x5, k          ; rd=6, rs1=5
        rom[base + 1] = (k << 20) | (5 << 15) | (6 << 7) | 0x13;
    }
    let mut cpu = BarrelState::new();
    cpu.activate_all();
    for i in 0..N_THREADS {
        cpu.set_thread_pc(i as u8, (i * 16) as u32);
    }
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    // Each thread executes 2 instructions = 16 dispatches total.
    // Add 6 cycles to drain the pipeline.
    for _ in 0..30 { cpu.step(); }

    for i in 0..N_THREADS {
        let k = i as u32 + 1;
        assert_eq!(cpu.read_thread_register(i as u8, 5), k, "thread {} t0", i);
        assert_eq!(cpu.read_thread_register(i as u8, 6), 2 * k, "thread {} t1", i);
    }
}

#[test]
fn barrel_8_threads_back_to_back_no_forwarding_needed() {
    // The big claim: with 8 threads and a 6-stage pipeline, two
    // back-to-back dependent instructions in the SAME thread
    // require no forwarding because they're 8 cycles apart in the
    // pipeline by the time the dependent reaches DI.
    //
    // Each thread runs:
    //   addi x5, x0, 7
    //   addi x6, x5, 1     <- depends on x5 from previous instr
    // Verify x6 = 8 in every thread without any explicit
    // forwarding logic.
    let mut rom = vec![0u32; 32];
    for i in 0..8 {
        let base = i * 4;
        // addi x5, x0, 7
        rom[base + 0] = (7 << 20) | (5 << 7) | 0x13;
        // addi x6, x5, 1
        rom[base + 1] = (1 << 20) | (5 << 15) | (6 << 7) | 0x13;
    }
    let mut cpu = BarrelState::new();
    cpu.activate_all();
    for i in 0..N_THREADS {
        cpu.set_thread_pc(i as u8, (i * 16) as u32);
    }
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..40 { cpu.step(); }

    for i in 0..N_THREADS {
        assert_eq!(cpu.read_thread_register(i as u8, 5), 7, "thread {} t0", i);
        assert_eq!(cpu.read_thread_register(i as u8, 6), 8, "thread {} t1 (no fwd needed)", i);
    }
}

#[test]
fn barrel_thread_branch_redirects_only_its_own_pc() {
    // Thread 0 takes a branch back to itself. Other threads keep
    // running their NOPs. Verify the branch only affects thread 0's
    // pc, no flush bubbles in the pipeline.
    //
    //   addr 0x00: addi t0, x0, 1     (thread 0)
    //   addr 0x04: beq  t0, t0, 0     (always taken, jumps back to 0x04)
    let mut rom = vec![0x00000013u32; 16]; // NOPs everywhere
    rom[0] = (1 << 20) | (5 << 7) | 0x13;       // addi x5, x0, 1
    rom[1] = (5 << 20) | (5 << 15) | (0 << 12) | (0 << 7) | 0x63;
    // beq x5, x5, 0  → branches to itself (offset 0). bits[7]=0, bits[31]=0,
    // bits[11:8]=0, bits[30:25]=0 → imm = 0. Spin loop on itself.

    let mut cpu = BarrelState::new();
    cpu.activate_all();
    cpu.set_thread_pc(0, 0);
    for i in 1..N_THREADS {
        cpu.set_thread_pc(i as u8, 0x20);
    }
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..40 { cpu.step(); }

    // Thread 0 should have x5 = 1 (the addi ran).
    assert_eq!(cpu.read_thread_register(0, 5), 1);
    // Thread 0's PC should be parked at the beq self-loop (0x04 or 0x08).
    let p0 = cpu.threads[0].pc;
    assert!(
        p0 == 0x04 || p0 == 0x08,
        "thread 0 pc expected to be parked near beq self-loop, got 0x{:x}", p0
    );
    // Other threads kept running at their own pc.
    for i in 1..N_THREADS {
        assert!(
            cpu.threads[i].pc >= 0x20,
            "thread {} pc should have advanced from 0x20, got 0x{:x}", i, cpu.threads[i].pc
        );
    }
}
