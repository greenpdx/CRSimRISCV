//! Trap subsystem: external IRQs, ECALL, MRET/SRET, M↔S delegation.

use cpu_sim::sim::cpu_state::CpuState;
use cpu_sim::sim::trap::{
    addr as csra, IRQ_MTI, IRQ_SEI, PrivMode,
    EXC_ECALL_FROM_M, EXC_ECALL_FROM_U,
};

/// Run one full instruction (10 micro-steps) and return.
fn step(cpu: &mut CpuState) {
    for _ in 0..10 { cpu.micro_step(); }
}

/// Run only the IrqCheck phase (the first micro-step of a fresh instruction).
/// Stops there so callers can observe the post-trap-redirect state directly.
fn irq_check_only(cpu: &mut CpuState) {
    cpu.micro_step();
}

#[test]
fn external_mti_takes_m_trap() {
    let mut cpu = CpuState::new();
    cpu.load_program(&[0x00000013, 0x00000013, 0x00000013]); // 3 NOPs
    // Enable global MIE and the MTI bit in mie.
    cpu.csr_write(csra::MSTATUS, 1 << 3);    // MIE
    cpu.csr_write(csra::MIE,     IRQ_MTI);
    cpu.csr_write(csra::MTVEC,   0x80);

    cpu.raise_irq(IRQ_MTI);
    irq_check_only(&mut cpu);

    // PC must now be the trap vector and a trap should be recorded.
    assert_eq!(cpu.pc, 0x80);
    let t = cpu.last_trap.expect("trap should have been taken");
    assert!(t.is_interrupt);
    assert_eq!(t.cause, 7); // INT_MTI
    assert_eq!(cpu.priv_mode(), PrivMode::M);
    // mepc should hold the original PC (0).
    assert_eq!(cpu.csr_read(csra::MEPC), 0);
}

#[test]
fn ecall_from_m_traps_to_m() {
    let mut cpu = CpuState::new();
    cpu.load_program(&[0x00000073]); // ECALL
    cpu.csr_write(csra::MTVEC, 0x100);
    step(&mut cpu);
    let t = cpu.last_trap.unwrap();
    assert!(!t.is_interrupt);
    assert_eq!(t.cause, EXC_ECALL_FROM_M);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn mret_returns_and_restores_mode() {
    let mut cpu = CpuState::new();
    // Pretend a trap from U has already been taken: pc=handler, mode=M,
    // mepc=0x40, mstatus.MPP=00 (U), mstatus.MPIE=1.
    cpu.load_program(&[0x30200073]); // MRET (0x30200073 = mret)
    cpu.csr_write(csra::MEPC, 0x40);
    // mstatus = MPIE | MPP=U(0)
    cpu.csr_write(csra::MSTATUS, 1 << 7);
    step(&mut cpu);
    assert_eq!(cpu.pc, 0x40);
    assert_eq!(cpu.priv_mode(), PrivMode::U);
    // MIE should now be set (restored from MPIE).
    let mstatus = cpu.csr_read(csra::MSTATUS);
    assert_ne!(mstatus & (1 << 3), 0, "MIE should be restored from MPIE");
}

#[test]
fn delegation_sei_to_s() {
    let mut cpu = CpuState::new();
    cpu.load_program(&[0x00000013]); // NOP
    // Start in U mode so an S-delegated interrupt is takeable.
    // Set up: SEI in mie, mideleg.SEI=1, stvec=0x200.
    // Force CPU into U:
    cpu.csr_write(csra::MIDELEG, IRQ_SEI);
    cpu.csr_write(csra::MIE,     IRQ_SEI);
    cpu.csr_write(csra::STVEC,   0x200);
    // Use mret to drop to U:  set mepc=0, MPP=U
    cpu.csr_write(csra::MEPC,    0);
    cpu.csr_write(csra::MSTATUS, 1 << 7); // MPIE=1 so MIE will be set after mret (irrelevant for S delegation)
    cpu.load_word(0x000, 0x30200073);     // MRET at 0
    cpu.load_word(0x004, 0x00000013);     // NOP at 4
    cpu.csr_write(csra::MEPC, 0x004);
    // Run MRET → drops to U, pc=4
    step(&mut cpu);
    assert_eq!(cpu.priv_mode(), PrivMode::U);
    assert_eq!(cpu.pc, 0x004);

    // Now raise SEI; it should delegate to S because mideleg.SEI=1.
    cpu.raise_irq(IRQ_SEI);
    irq_check_only(&mut cpu);
    assert_eq!(cpu.priv_mode(), PrivMode::S);
    assert_eq!(cpu.pc, 0x200);
    let t = cpu.last_trap.unwrap();
    assert_eq!(t.to_mode, PrivMode::S);
}

#[test]
fn ecall_from_u_traps_to_m_when_not_delegated() {
    let mut cpu = CpuState::new();
    // Drop to U via MRET first, then run ECALL.
    cpu.load_word(0x000, 0x30200073); // MRET
    cpu.load_word(0x004, 0x00000073); // ECALL
    cpu.csr_write(csra::MEPC,    0x004);
    cpu.csr_write(csra::MSTATUS, 0);          // MPP=U, MPIE=0
    cpu.csr_write(csra::MTVEC,   0x300);
    step(&mut cpu); // MRET → U, pc=4
    assert_eq!(cpu.priv_mode(), PrivMode::U);
    step(&mut cpu); // ECALL from U → M (medeleg=0)
    let t = cpu.last_trap.unwrap();
    assert_eq!(t.cause, EXC_ECALL_FROM_U);
    assert_eq!(t.to_mode, PrivMode::M);
    assert_eq!(cpu.pc, 0x300);
}
