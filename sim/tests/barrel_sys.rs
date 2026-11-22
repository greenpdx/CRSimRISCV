//! Barrel on the shared system unit (sim/system): one unit per thread,
//! shared mtvec / mie / mip / medeleg / mideleg, interrupt delivery at a
//! thread's fetch slot (precise: its in-flight instructions drain first).

use cpu_sim::isa::csr::addr;
use cpu_sim::sim::system::{IRQ_MTI, IRQ_MEI};
use cpu_sim::sim::barrel::{BarrelState, N_THREADS};
use cpu_sim::isa::encode::{MRET, NOP, addi, csrrw};

fn jal_back4() -> u32 { 0xffdff06f }   // jal x0, -4

/// All 8 threads run (the barrel's normal mode: each thread has at most
/// one instruction in flight). Thread 0: 0x00 addi x5,x5,1 · 0x04 j 0x00;
/// the others spin at 0x80. Handler at 0x100: addi x6,x6,1 · mret.
fn counter() -> BarrelState {
    let mut cpu = BarrelState::new();
    let mut rom = vec![NOP; 128];
    rom[0] = addi(5, 5, 1);
    rom[1] = jal_back4();
    rom[32] = 0x0000006f;                 // 0x80: j .
    rom[64] = addi(6, 6, 1);
    rom[65] = MRET;
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for s in 0..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = if s == 0 { 0 } else { 0x80 };
    }
    cpu.write_csr(0, addr::MTVEC, 0x100);
    cpu.write_csr(0, addr::MIE, IRQ_MTI);
    cpu
}

#[test]
fn mhartid_is_the_slot_and_misa_is_rv32i() {
    let mut cpu = BarrelState::new();
    for s in 0..N_THREADS as u8 {
        assert_eq!(cpu.read_csr(s, addr::MHARTID), s as u32);
        assert_eq!(cpu.read_csr(s, addr::MISA), 0x4000_0100);
    }
    cpu.write_csr(3, addr::MHARTID, 9);
    assert_eq!(cpu.read_csr(3, addr::MHARTID), 3, "read-only");
}

#[test]
fn shared_csrs_are_one_value_per_thread_csrs_are_not() {
    let mut cpu = BarrelState::new();
    cpu.write_csr(5, addr::MTVEC, 0x200);
    cpu.write_csr(5, addr::MSCRATCH, 0x55);
    for s in 0..N_THREADS as u8 {
        assert_eq!(cpu.read_csr(s, addr::MTVEC), 0x200);
        assert_eq!(cpu.read_csr(s, addr::MSCRATCH), if s == 5 { 0x55 } else { 0 });
    }
    // a csrrw from thread 2 also writes every thread's mtvec
    let mut rom = vec![NOP; 64];
    rom[0] = addi(1, 0, 0x300);
    rom[1] = csrrw(0, 0x305, 1);
    rom[2] = 0x0000006f;
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    cpu.threads[2].active = true;
    cpu.threads[2].pc = 0;
    for _ in 0..30 { cpu.step(); }
    for s in 0..N_THREADS as u8 { assert_eq!(cpu.read_csr(s, addr::MTVEC), 0x300); }
    // and a boot reset keeps it (it was global state before)
    cpu.reset_boot();
    assert_eq!(cpu.read_csr(7, addr::MTVEC), 0x300);
}

#[test]
fn interrupt_needs_mie() {
    let mut cpu = counter();                     // mstatus.MIE = 0
    cpu.raise_irq(IRQ_MTI);
    for _ in 0..400 { cpu.step(); }
    assert_eq!(cpu.read_csr(0, addr::MCAUSE), 0);
    assert!(cpu.read_thread_register(0, 5) > 5, "kept counting");
}

#[test]
fn thread_takes_an_interrupt_and_returns() {
    let mut cpu = counter();
    cpu.write_csr(0, addr::MSTATUS, 1 << 3);     // MIE
    for _ in 0..400 { cpu.step(); }
    let before = cpu.read_thread_register(0, 5);
    assert!(before > 3);
    cpu.raise_irq(IRQ_MTI);
    let mut taken = false;
    for _ in 0..40 {
        cpu.step();
        if cpu.threads[0].sys.in_trap() { taken = true; break; }
    }
    let x6_before = cpu.read_thread_register(0, 6);
    assert!(taken, "interrupt taken within the drain window");
    assert_eq!(cpu.read_csr(0, addr::MCAUSE), 0x8000_0007);
    let mepc = cpu.read_csr(0, addr::MEPC);
    assert!(mepc == 0 || mepc == 4, "mepc inside the loop: {mepc:#x}");
    assert_eq!(cpu.read_csr(0, addr::MSTATUS) & (1 << 3), 0, "MIE cleared in the handler");
    cpu.clear_irq();
    for _ in 0..400 { cpu.step(); }
    assert_eq!(x6_before, 0);
    assert_eq!(cpu.read_thread_register(0, 6), 1, "handler ran once");
    assert!(!cpu.threads[0].sys.in_trap(), "mret returned");
    assert!(cpu.read_thread_register(0, 5) > before + 3, "loop resumed");
}

#[test]
fn only_threads_with_mie_take_it() {
    // T0 and T4 run the counter; only T4 enables interrupts
    let mut cpu = counter();
    cpu.threads[4].pc = 0;
    cpu.write_csr(0, addr::MIE, IRQ_MEI);
    cpu.write_csr(4, addr::MSTATUS, 1 << 3);
    cpu.raise_irq(IRQ_MEI);
    for _ in 0..60 { cpu.step(); }
    assert_eq!(cpu.read_csr(4, addr::MCAUSE), 0x8000_000b);
    assert_eq!(cpu.read_csr(0, addr::MCAUSE), 0);
}

#[test]
fn instret_counts_per_thread() {
    let mut cpu = counter();
    cpu.threads[3].active = false;
    for _ in 0..80 { cpu.step(); }
    assert!(cpu.read_csr(0, addr::INSTRET) > 5);
    assert_eq!(cpu.read_csr(3, addr::INSTRET), 0);
    assert_eq!(cpu.read_csr(3, addr::CYCLE), 79);
}

#[test]
fn lone_thread_drains_then_traps_precisely() {
    // One thread, straight-line code (addi x5 +1 per word), so it has
    // instructions in flight every cycle: the interrupt must wait for them
    // to drain, then mepc = the next instruction and x5 = mepc / 4.
    let mut cpu = BarrelState::new();
    cpu.reset_boot();
    let mut rom = vec![addi(5, 5, 1); 64];
    rom.resize(128, NOP);                   // handler at 0x100: addi x6, then NOPs
    rom[64] = addi(6, 6, 1);
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    cpu.write_csr(0, addr::MTVEC, 0x100);
    cpu.write_csr(0, addr::MIE, IRQ_MTI);
    cpu.write_csr(0, addr::MSTATUS, 1 << 3);
    for _ in 0..30 { cpu.step(); }
    cpu.raise_irq(IRQ_MTI);
    let mut n = 0;
    while !cpu.threads[0].sys.in_trap() && n < 20 { cpu.step(); n += 1; }
    assert!(cpu.threads[0].sys.in_trap(), "taken");
    assert!(n <= 6, "drained within the pipeline depth ({n} cycles)");
    let mepc = cpu.read_csr(0, addr::MEPC);
    // nothing in flight at the trap: every instruction before mepc has
    // retired, none after it was fetched
    assert_eq!(cpu.read_thread_register(0, 5), mepc / 4, "x5 at the trap");
    for _ in 0..10 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 5), mepc / 4, "nothing after mepc ran");
    assert_eq!(cpu.read_thread_register(0, 6), 1, "handler ran");
}
