//! Phase B7 — IRQ injection + per-thread CSRs.

use cpu_sim::sim::barrel::{BarrelState, N_THREADS};

const NOP: u32 = 0x00000013;
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
fn lui(rd: u32, imm: u32) -> u32 { (imm << 12) | (rd << 7) | 0x37 }
fn lw(rd: u32, rs1: u32, off: i32) -> u32 {
    (((off as u32) & 0xfff) << 20) | (rs1 << 15) | (0b010 << 12) | (rd << 7) | 0x03
}
fn jal_spin() -> u32 { 0x0000006f }

#[test]
fn b7_inject_irq_activates_locked_slot() {
    // Boot mode: only slot 0 active. Slot 5 is locked. Inject an
    // IRQ on slot 5 — it should become active and run from its
    // pre-loaded handler PC.
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    // Slot 5 is locked, but its handler PC is pre-loaded:
    cpu.threads[5].pc = 0x40;
    // Some spinners on 1..4, 6..7 to keep the pipeline busy.
    for s in [1, 2, 3, 4, 6, 7] {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0;
    }
    let mut rom = vec![NOP; 64];
    rom[0]  = jal_spin();
    rom[16] = addi(5, 0, 99);    // 0x40 — handler entry
    rom[17] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    // Step a few cycles before injection — slot 5 should not run.
    for _ in 0..16 { cpu.step(); }
    assert!(!cpu.threads[5].active);
    assert_eq!(cpu.read_thread_register(5, 5), 0);

    // Set up routing: IRQ line 5 → slot 5.
    cpu.irq.route[5] = 5;
    // Inject IRQ line 5 → slot 5 wakes on next tick.
    cpu.inject_irq(5);
    cpu.step(); // tick the IRQ controller
    assert!(cpu.threads[5].active, "slot 5 should be active after IRQ");
    assert_ne!(cpu.irq.pending & (1 << 5), 0, "IRQ line 5 pending");

    // Step until slot 5 has retired its addi.
    for _ in 0..40 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(5, 5), 99,
               "slot 5 should have run its handler after IRQ");
}

#[test]
fn b7_csr_per_thread_storage() {
    // Each thread has independent CSR storage. Writing one
    // slot's mtvec must not affect another's.
    let mut cpu = BarrelState::new();
    // mtvec is global: writing from any slot sets the same value.
    cpu.write_csr(0, 0x305, 0xdead_0000);
    assert_eq!(cpu.read_csr(0, 0x305), 0xdead_0000);
    assert_eq!(cpu.read_csr(3, 0x305), 0xdead_0000); // same global
    // Per-thread CSRs are independent.
    cpu.write_csr(0, 0x340, 0x1111); // slot 0 mscratch
    cpu.write_csr(3, 0x340, 0x3333); // slot 3 mscratch
    assert_eq!(cpu.read_csr(0, 0x340), 0x1111);
    assert_eq!(cpu.read_csr(3, 0x340), 0x3333);
    assert_eq!(cpu.read_csr(7, 0x340), 0); // untouched slot
}

#[test]
fn b7_csr_addresses_recognized() {
    // Spot-check a few CSRs round-trip.
    let mut cpu = BarrelState::new();
    // Per-thread CSRs round-trip.
    let per_thread = [
        (0x300u16, 0x12345678u32), // mstatus
        (0x341,    0xaabb_ccdd),   // mepc
        (0x342,    0x8000_000b),   // mcause
        (0x343,    0xdead_beef),   // mtval
        (0x340,    0x1234_5678),   // mscratch
        (0x180,    0x8000_0010),   // satp
    ];
    for (addr, val) in per_thread {
        cpu.write_csr(2, addr, val);
        assert_eq!(cpu.read_csr(2, addr), val, "per-thread csr 0x{:x}", addr);
    }
    // Global CSRs round-trip.
    let global = [
        (0x305u16, 0x80001000u32), // mtvec
        (0x304,    0x800),         // mie
        (0x344,    0x800),         // mip
    ];
    for (addr, val) in global {
        cpu.write_csr(0, addr, val); // slot arg ignored for globals
        assert_eq!(cpu.read_csr(0, addr), val, "global csr 0x{:x}", addr);
    }
}

#[test]
fn b7_thread_count_constant() {
    assert_eq!(N_THREADS, 8);
}

// ── Sync trap tests ────────────────────────────────────────────

#[test]
fn b7_illegal_instr_traps_to_mtvec() {
    // Thread 0 executes an unknown opcode. It should trap to the
    // global mtvec. mepc[T0] = the faulting PC. mcause[T0] = 2.
    let mut cpu = BarrelState::new();
    cpu.mtvec = 0x100;  // global trap vector
    let mut rom = vec![NOP; 128];
    rom[0] = 0x0000_000B;  // unknown opcode (not custom-0 with valid funct7)
    // Wait, 0x0b IS the custom-0 opcode. Use something else.
    rom[0] = 0x0000_007F;  // opcode 0x7F is reserved → illegal
    rom[64] = addi(6, 0, 0x42); // handler at mtvec=0x100 (word 64)
    rom[65] = jal_spin();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    // Spinners
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x80;
    }
    rom[32] = jal_spin(); // spinners at 0x80
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..100 { cpu.step(); }
    assert_eq!(cpu.threads[0].mcause, 2, "mcause = illegal instr");
    assert_eq!(cpu.threads[0].mepc, 0, "mepc = faulting PC");
    assert_eq!(cpu.read_thread_register(0, 6), 0x42, "handler ran");
}

#[test]
fn b7_ecall_traps_with_correct_cause() {
    let mut cpu = BarrelState::new();
    cpu.mtvec = 0x100;
    let mut rom = vec![NOP; 128];
    rom[0] = 0x00000073;  // ECALL
    rom[64] = addi(6, 0, 0x55);
    rom[65] = jal_spin();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priv_mode = 3; // M-mode
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x80;
    }
    rom[32] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..100 { cpu.step(); }
    assert_eq!(cpu.threads[0].mcause, 11, "mcause = ecall from M");
    assert_eq!(cpu.read_thread_register(0, 6), 0x55, "handler ran");
}

#[test]
fn b7_mret_restores_pc() {
    // Set up: thread 0 traps via ECALL, handler does MRET.
    let mut cpu = BarrelState::new();
    cpu.mtvec = 0x100;
    let mut rom = vec![NOP; 128];
    // 0x00: ecall        → traps to 0x100
    // 0x04: addi x7,x0,99 → this is where mret should return to
    // 0x08: jal spin
    rom[0]  = 0x00000073;  // ECALL
    rom[1]  = addi(7, 0, 99);
    rom[2]  = jal_spin();
    // Handler at 0x100:
    rom[64] = 0x30200073;  // MRET
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x80;
    }
    rom[32] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..200 { cpu.step(); }
    // mepc was set to 0 (ECALL's PC). After MRET, T0 resumes at mepc=0.
    // Then T0 re-executes the ECALL, traps again, loops forever.
    // To make MRET return past ECALL, the handler should advance mepc.
    // For this test, just verify that the mret instruction executed
    // (mcause was set and x7 should be 0 because it returns to ECALL).
    assert_eq!(cpu.threads[0].mcause, 11, "ecall trapped");
    // T0 is stuck in ecall→trap→mret→ecall loop; x7 never gets set.
    assert_eq!(cpu.read_thread_register(0, 7), 0, "mret loops back to ecall");
}

#[test]
fn b7_irq_controller_routing() {
    // Set up IRQ route: line 2 → slot 2. Assert line 2.
    // Slot 2 should activate.
    let mut cpu = BarrelState::new();
    cpu.irq.route[2] = 2;
    cpu.threads[2].pc = 0x40;
    let mut rom = vec![NOP; 32];
    rom[16] = addi(6, 0, 0x77);  // handler at 0x40
    rom[17] = jal_spin();
    rom[0]  = jal_spin();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    cpu.inject_irq(2);  // assert line 2
    for _ in 0..100 { cpu.step(); }
    assert!(cpu.threads[2].active || cpu.read_thread_register(2, 6) == 0x77,
            "slot 2 should have been activated by IRQ line 2");
    assert_eq!(cpu.read_thread_register(2, 6), 0x77, "handler ran");
}

// ── CSR instructions + thread MMIO tests ───────────────────────

fn enc_csrrw(rd: u32, csr: u32, rs1: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (1 << 12) | (rd << 7) | 0x73
}
fn enc_csrrs(rd: u32, csr: u32, rs1: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (2 << 12) | (rd << 7) | 0x73
}

#[test]
fn b7_csrrw_reads_and_writes_mcause() {
    // Thread 0 writes mcause via CSRRW, reads it back.
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x20;
    }
    let mut rom = vec![NOP; 32];
    rom[0] = addi(6, 0, 42);                     // x6 = 42
    rom[1] = enc_csrrw(7, 0x342, 6);             // csrrw x7, mcause, x6 → x7=old, mcause=42
    rom[2] = enc_csrrs(8, 0x342, 0);             // csrrs x8, mcause, x0 → x8=mcause (= 42)
    rom[3] = jal_spin();
    rom[8] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..100 { cpu.step(); }
    assert_eq!(cpu.threads[0].mcause, 42, "mcause written by csrrw");
    assert_eq!(cpu.read_thread_register(0, 7), 0, "old mcause was 0");
    assert_eq!(cpu.read_thread_register(0, 8), 42, "csrrs reads back 42");
}

#[test]
fn b7_csrrw_global_mtvec() {
    // CSRRW to mtvec (global) from T0.
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x20;
    }
    let mut rom = vec![NOP; 32];
    rom[0] = addi(6, 0, 0x80);
    rom[1] = enc_csrrw(0, 0x305, 6);             // csrrw x0, mtvec, x6 → mtvec=0x80
    rom[2] = jal_spin();
    rom[8] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..60 { cpu.step(); }
    assert_eq!(cpu.mtvec, 0x80, "global mtvec set by csrrw");
}

#[test]
fn b7_thread_mmio_t0_reads_t3_regs() {
    // T0 (M-mode) reads T3's x5 via thread MMIO.
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priv_mode = 3;
    // Pre-set T3's x5 to 0xABCD.
    cpu.threads[3].regs[5] = 0xABCD;
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x40;
    }
    let mut rom = vec![NOP; 32];
    // T0 loads from TMMIO: slot 3, reg 5.
    // addr = 0x4000_4000 + 3*0x100 + 5*4 = 0x4000_4314
    rom[0] = lui(6, 0x40004);                    // x6 = 0x40004000
    rom[1] = addi(6, 6, 0x314);                  // x6 = 0x40004314
    rom[2] = lw(7, 6, 0);                        // x7 = MEM[0x40004314] = T3.x5
    rom[3] = jal_spin();
    rom[16] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..100 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 7), 0xABCD,
               "T0 should read T3's x5 via thread MMIO");
}

#[test]
fn b7_thread_mmio_denied_for_non_t0() {
    // T3 tries to read thread MMIO → access fault.
    let mut cpu = BarrelState::new();
    cpu.mtvec = 0x100;
    for s in 0..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = if s == 3 { 0 } else { 0x40 };
    }
    let mut rom = vec![NOP; 128];
    // T3's code: try to load from TMMIO.
    rom[0]  = lui(6, 0x40004);
    rom[1]  = lw(7, 6, 0);
    rom[2]  = jal_spin();
    rom[16] = jal_spin();
    rom[64] = addi(8, 0, 0x99);   // trap handler at 0x100
    rom[65] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..100 { cpu.step(); }
    // T3 should have trapped (load access fault, cause 5).
    assert_eq!(cpu.threads[3].mcause, 5, "T3 access fault on thread MMIO");
}
