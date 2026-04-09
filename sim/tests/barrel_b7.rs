//! Phase B7 — IRQ injection + per-thread CSRs.

use cpu_sim::sim::barrel::{BarrelState, N_THREADS};

const NOP: u32 = 0x00000013;
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
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

    // Inject IRQ → slot 5 wakes.
    cpu.inject_irq(5);
    assert!(cpu.threads[5].active);
    assert_ne!(cpu.threads[5].mip & (1 << 11), 0, "MEIP set on injection");

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
    cpu.write_csr(0, 0x305, 0xdead_0000); // slot 0 mtvec
    cpu.write_csr(3, 0x305, 0xbeef_0000); // slot 3 mtvec
    cpu.write_csr(7, 0x305, 0xcafe_0000); // slot 7 mtvec
    assert_eq!(cpu.read_csr(0, 0x305), 0xdead_0000);
    assert_eq!(cpu.read_csr(3, 0x305), 0xbeef_0000);
    assert_eq!(cpu.read_csr(7, 0x305), 0xcafe_0000);
    // Other CSRs unaffected
    assert_eq!(cpu.read_csr(0, 0x300), 0); // mstatus
    assert_eq!(cpu.read_csr(3, 0x340), 0); // mscratch
}

#[test]
fn b7_csr_addresses_recognized() {
    // Spot-check a few CSRs round-trip.
    let mut cpu = BarrelState::new();
    let pairs = [
        (0x300u16, 0x12345678u32), // mstatus
        (0x305,    0x80001000),    // mtvec
        (0x341,    0xaabb_ccdd),   // mepc
        (0x342,    0x8000_000b),   // mcause (MEI)
        (0x344,    0x800),         // mip (MEIP)
        (0x304,    0x800),         // mie
        (0x180,    0x8000_0010),   // satp
    ];
    for (addr, val) in pairs {
        cpu.write_csr(2, addr, val);
        assert_eq!(cpu.read_csr(2, addr), val, "csr 0x{:x}", addr);
    }
}

#[test]
fn b7_thread_count_constant() {
    assert_eq!(N_THREADS, 8);
}
