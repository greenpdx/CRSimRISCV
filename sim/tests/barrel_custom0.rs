//! Phase B2 — custom-0 instructions for the barrel processor.
//! FORK / JOIN / DONE / YIELD / TID / FORKRET, opcode 0x0b.

use cpu_sim::sim::barrel::{BarrelState, N_THREADS};
use cpu_sim::isa::encode::{NOP, SPIN, addi};

// ── Helper: set up a barrel CPU with slot 0 = parent, slot 1 =
// inactive (FORK target), slots 2..7 = spinning at a NOP loop in
// ROM. Spinners exist purely to space the parent's instructions in
// the pipeline (single-thread RAW forwarding lands in phase B5).
//
// Spinners' PC is set to `spinner_pc` — the caller is responsible
// for placing a `jal x0, 0` there in ROM.
fn cpu_with_spinners(spinner_pc: u32) -> BarrelState {
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    // slot 1 stays idle (FORK target)
    for s in 2..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = spinner_pc;
        cpu.threads[s].priority = 1;
    }
    cpu
}

// ── R-type encoder for custom-0 ───────────────────────────────────
fn enc(funct7: u32, rs2: u32, rs1: u32, rd: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (0 << 12) | (rd << 7) | 0x0b
}
fn fork(rd: u32, rs1: u32, rs2: u32) -> u32 { enc(0x00, rs2, rs1, rd) }
fn join(rs1: u32)                    -> u32 { enc(0x01, 0, rs1, 0) }
fn done()                            -> u32 { enc(0x02, 0, 0, 0) }
fn yield_()                          -> u32 { enc(0x03, 0, 0, 0) }
fn tid(rd: u32)                      -> u32 { enc(0x04, 0, 0, rd) }
fn forkret(rd_src: u32)              -> u32 { enc(0x05, 0, 0, rd_src) }

// ── TID ───────────────────────────────────────────────────────────
#[test]
fn barrel_tid_writes_current_thread_id() {
    // Each thread runs `tid x5; jal x0, 0`. After running, every
    // thread's x5 must equal its own slot ID.
    let mut rom = vec![NOP; 32];
    for i in 0..N_THREADS {
        let base = i * 2;
        rom[base + 0] = tid(5);
        rom[base + 1] = SPIN;
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
        assert_eq!(cpu.read_thread_register(i as u8, 5), i as u32, "tid in slot {}", i);
    }
}

// ── FORK ──────────────────────────────────────────────────────────
#[test]
fn barrel_fork_allocates_lowest_free_slot() {
    // Slot 0 forks a child whose start PC = 0x40. Spinners on
    // slots 2..7 fill the pipeline so the parent's RAW deps work.
    //
    //   0x00: addi x7, x0, 0x40   ; child start
    //   0x04: addi x8, x0, 0      ; arg
    //   0x08: fork x5, x7, x8
    //   0x0c: jal  x0, 0          ; parent spin
    //   ...
    //   0x40: addi x6, x0, 99     ; child entry
    //   0x44: jal  x0, 0          ; child spin
    //   0x80: jal  x0, 0          ; spinner pc (slots 2..7)
    let mut rom = vec![NOP; 64];
    rom[0]  = addi(7, 0, 0x40);
    rom[1]  = addi(8, 0, 0);
    rom[2]  = fork(5, 7, 8);
    rom[3]  = SPIN;
    rom[16] = addi(6, 0, 99);
    rom[17] = SPIN;
    rom[32] = SPIN;
    let mut cpu = cpu_with_spinners(0x80);
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..120 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 5), 1, "parent x5 = child slot ID");
    assert_eq!(cpu.read_thread_register(1, 6), 99, "child x6 set by child");
    assert!(cpu.threads[1].active, "child still active in spin");
}

#[test]
fn barrel_fork_returns_minus1_when_full() {
    // Activate all 8 threads first, then have slot 0 issue FORK.
    // Should return -1 (= 0xFFFFFFFF) because no free slot.
    let mut rom = vec![NOP; 16];
    rom[0] = addi(7, 0, 0x40);    // dummy start pc
    rom[1] = addi(8, 0, 0);
    rom[2] = fork(5, 7, 8);
    rom[3] = SPIN;
    let mut cpu = BarrelState::new();
    cpu.activate_all();
    for i in 0..N_THREADS {
        cpu.set_thread_pc(i as u8, 0);
    }
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..40 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 5), 0xffff_ffff, "no free slot → -1");
}

// ── DONE / JOIN ───────────────────────────────────────────────────
#[test]
fn barrel_join_blocks_until_child_done() {
    let mut rom = vec![NOP; 64];
    rom[0]  = addi(7, 0, 0x40);
    rom[1]  = addi(8, 0, 0);
    rom[2]  = fork(5, 7, 8);
    rom[3]  = join(5);
    rom[4]  = addi(6, 0, 7);
    rom[5]  = SPIN;
    rom[16] = addi(6, 0, 99);
    rom[17] = done();
    rom[18] = SPIN;
    rom[32] = SPIN;
    let mut cpu = cpu_with_spinners(0x80);
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..300 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 6), 7, "parent x6");
    assert_eq!(cpu.read_thread_register(1, 6), 99, "child x6");
    assert!(!cpu.threads[1].active, "child should be inactive after DONE");
    assert_eq!(cpu.threads[0].waiting, -1, "parent unblocked");
}

#[test]
fn barrel_join_on_finished_thread_no_block() {
    // Child immediately DONEs. Parent forks then loops a delay
    // (using NOPs as the delay) and finally JOINs. The JOIN
    // should be a no-op because the child is already inactive.
    let mut rom = vec![NOP; 64];
    rom[0]  = addi(7, 0, 0x40);
    rom[1]  = addi(8, 0, 0);
    rom[2]  = fork(5, 7, 8);
    // delay loop: many nops so the child has time to retire
    rom[3]  = NOP;
    rom[4]  = NOP;
    rom[5]  = NOP;
    rom[6]  = NOP;
    rom[7]  = NOP;
    rom[8]  = NOP;
    rom[9]  = NOP;
    rom[10] = NOP;
    rom[11] = join(5);
    rom[12] = addi(6, 0, 42);
    rom[13] = SPIN;
    rom[16] = done();
    rom[17] = SPIN;
    rom[32] = SPIN;
    let mut cpu = cpu_with_spinners(0x80);
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..300 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 6), 42, "parent passed JOIN no-block");
    assert!(!cpu.threads[1].active, "child done");
}

// ── YIELD ─────────────────────────────────────────────────────────
#[test]
fn barrel_yield_skips_one_round() {
    // Two threads, both in tight spins that increment x5. Thread 0
    // also issues YIELD on every iteration. Over many cycles,
    // thread 1 should retire more instructions than thread 0
    // because thread 0 voluntarily skips dispatch slots.
    //
    // Slot 0:  addi x5, x5, 1
    //          yield
    //          jal  x0, -8        ; loop back to addi
    // Slot 1:  addi x5, x5, 1
    //          jal  x0, -4        ; loop back
    let jal_off = |off: i32| -> u32 {
        let imm = off as u32 & 0x1f_ffff;
        let imm20    = (imm >> 20) & 1;
        let imm10_1  = (imm >> 1) & 0x3ff;
        let imm11    = (imm >> 11) & 1;
        let imm19_12 = (imm >> 12) & 0xff;
        (imm20 << 31) | (imm10_1 << 21) | (imm11 << 20) | (imm19_12 << 12) | (0 << 7) | 0x6f
    };
    let mut rom = vec![NOP; 32];
    rom[0] = addi(5, 5, 1);
    rom[1] = yield_();
    rom[2] = jal_off(-8);
    rom[8] = addi(5, 5, 1);
    rom[9] = jal_off(-4);
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    cpu.threads[1].active = true;
    cpu.threads[1].pc = 0x20;
    cpu.threads[1].priority = 1;
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..200 { cpu.step(); }
    let t0_count = cpu.read_thread_register(0, 5);
    let t1_count = cpu.read_thread_register(1, 5);
    assert!(t1_count > t0_count,
            "yielding thread 0 should run less than thread 1 (got t0={} t1={})", t0_count, t1_count);
}

// ── FORKRET ───────────────────────────────────────────────────────
#[test]
fn barrel_forkret_passes_return_value_to_parent_a0() {
    let mut rom = vec![NOP; 64];
    rom[0]  = addi(7, 0, 0x40);
    rom[1]  = addi(8, 0, 0);
    rom[2]  = fork(5, 7, 8);
    rom[3]  = join(5);
    rom[4]  = SPIN;
    rom[16] = addi(10, 0, 0x55);
    // pad so child's addi has time to retire before forkret reads its rd
    rom[17] = NOP;
    rom[18] = NOP;
    rom[19] = NOP;
    rom[20] = forkret(10);
    rom[21] = SPIN;
    rom[32] = SPIN;
    let mut cpu = cpu_with_spinners(0x80);
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..400 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 10), 0x55,
               "parent's a0 = child's FORKRET return value");
    assert!(!cpu.threads[1].active, "child done");
}

// ── The assembler's thread mnemonics run on the core ──────────────
/// Assemble one block of source into words.
fn asm(src: &str) -> Vec<u32> {
    let r = rv_asm::assemble(src);
    assert!(r.success, "{:?}", r.errors.iter().map(|e| &e.message).collect::<Vec<_>>());
    r.instructions.iter().map(|i| i.value).collect()
}

#[test]
fn assembled_thread_instructions_run_on_the_barrel() {
    // Parent forks a child, joins it and reads its own id; the child reads
    // its id, yields once and returns a value with forkret.
    let parent = asm("addi x7, x0, 0x40\naddi x8, x0, 0\nfork x5, x7, x8\njoin x5\ntid x9\naddi x10, x0, 77\njal x0, 0");
    let child = asm("tid x6\nyield\naddi x11, x0, 5\nforkret x11");
    let mut rom = vec![NOP; 64];
    rom[..parent.len()].copy_from_slice(&parent);
    rom[16..16 + child.len()].copy_from_slice(&child);
    rom[32] = SPIN;
    let mut cpu = cpu_with_spinners(0x80);
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..400 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 5), 1, "fork returned the child's slot");
    assert_eq!(cpu.read_thread_register(1, 6), 1, "the child's tid");
    assert_eq!(cpu.read_thread_register(0, 9), 0, "the parent's tid");
    assert_eq!(cpu.read_thread_register(0, 10), 77, "the parent ran on after the join");
    assert!(!cpu.threads[1].active, "the child ended with forkret");
}
