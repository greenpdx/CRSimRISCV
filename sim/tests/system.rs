//! Shared system unit: Zicsr execution, trap entry / return, IRQ sampling.

use cpu_sim::sim::system::{addr, csr_exec, PrivMode, SysUnit, IRQ_MTI, INT_MTI, EXC_ECALL_FROM_M};

#[test]
fn csr_exec_ops() {
    // CSRRW / CSRRS / CSRRC with a register source
    assert_eq!(csr_exec(0xF0, 0b001, 5, 0x0F), cpu_sim::sim::system::CsrResult { old: 0xF0, new: 0x0F, write: true });
    assert_eq!(csr_exec(0xF0, 0b010, 5, 0x0F).new, 0xFF);
    assert_eq!(csr_exec(0xF0, 0b011, 5, 0x30).new, 0xC0);
    // RS / RC with rs1 = x0 do not write; RW does
    assert!(!csr_exec(0xF0, 0b010, 0, 0xFFFF).write);
    assert!(!csr_exec(0xF0, 0b011, 0, 0xFFFF).write);
    assert!(csr_exec(0xF0, 0b001, 0, 0).write);
    // immediate forms use the rs1 field as zimm
    assert_eq!(csr_exec(0, 0b101, 7, 0xDEAD).new, 7);
    assert_eq!(csr_exec(0x10, 0b110, 1, 0).new, 0x11);
    assert!(!csr_exec(0x10, 0b111, 0, 0).write);
}

#[test]
fn trap_and_mret_round_trip() {
    let mut s = SysUnit::new();
    s.csrs.write(addr::MTVEC, 0x0040_0100);
    s.csrs.write(addr::MSTATUS, 1 << 3); // MIE
    let pc = s.trap(s.ecall_cause(), false, 0x0040_0020, 0, 9);
    assert_eq!(pc, 0x0040_0100);
    assert_eq!(s.csrs.read(addr::MEPC), 0x0040_0020);
    assert_eq!(s.csrs.read(addr::MCAUSE), EXC_ECALL_FROM_M);
    assert_eq!(s.csrs.read(addr::MSTATUS) & (1 << 3), 0, "MIE cleared");
    assert!(s.in_trap());
    assert_eq!(s.xret(false), 0x0040_0020);
    assert_ne!(s.csrs.read(addr::MSTATUS) & (1 << 3), 0, "MIE restored");
    assert!(!s.in_trap());
}

#[test]
fn irq_sampling_and_vectored_mode() {
    let mut s = SysUnit::new();
    s.csrs.write(addr::MTVEC, 0x0040_0101); // vectored
    s.raise_irq(IRQ_MTI);
    assert_eq!(s.sample_irq(), None, "not enabled yet");
    s.csrs.write(addr::MIE, IRQ_MTI);
    s.csrs.write(addr::MSTATUS, 1 << 3);
    assert_eq!(s.irq_would_take(), Some(INT_MTI));
    assert_eq!(s.vector_for(INT_MTI, true), 0x0040_0100 + 4 * INT_MTI);
    let c = s.sample_irq().unwrap();
    assert_eq!(s.trap(c, true, 0x0040_0008, 0, 1), 0x0040_011C);
    assert_eq!(s.csrs.read(addr::MCAUSE), 0x8000_0000 | INT_MTI);
    assert_eq!(s.priv_mode, PrivMode::M);
}
