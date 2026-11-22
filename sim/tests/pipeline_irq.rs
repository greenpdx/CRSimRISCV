//! Pipeline interrupts are precise: the handler runs and `mret` returns,
//! whatever is in flight when the interrupt arrives — including the older
//! taken jump of a `j .` idle loop — in every branch mode.

use cpu_sim::isa::csr::addr;
use cpu_sim::isa::encode::*;
use cpu_sim::sim::pipeline::{state::BranchMode, PipelineState};
use cpu_sim::sim::system::IRQ_MEI;

/// Enable MEI, idle in `idle`; handler at 0x80: x10 += 1, x11 = mcause,
/// x12 = mepc, clear the line, mret.
fn program(idle: &[u32]) -> Vec<u32> {
    let mut p = vec![addi(9, 0, 0x80), csrw(addr::MTVEC as u32, 9),
                     lui(9, 1), addi(9, 9, -0x800), csrw(addr::MIE as u32, 9),
                     addi(9, 0, 8), csrw(addr::MSTATUS as u32, 9)];
    p.extend_from_slice(idle);
    p.resize(0x80 / 4, NOP);
    p.extend([addi(10, 10, 1), csrr(11, addr::MCAUSE as u32), csrr(12, addr::MEPC as u32), MRET]);
    p
}

#[test]
fn interrupt_runs_its_handler_from_any_idle_loop() {
    let idles: [(&str, Vec<u32>); 3] = [
        ("j .", vec![SPIN]),
        ("nop; j -4", vec![NOP, jal(0, -4)]),
        ("counting loop", vec![addi(13, 13, 1), bne(13, 0, -4)]),
    ];
    for (name, idle) in &idles {
        for mode in 0..3u8 {
            for at in [40usize, 41, 42, 43] {
                let mut c = PipelineState::new();
                c.branch_mode = BranchMode::from_u8(mode);
                c.load_program(&program(idle));
                for cyc in 0..200 {
                    if cyc == at { c.sys.raise_irq(IRQ_MEI); }
                    if c.read_register(10) == 1 { c.sys.clear_irq(); }
                    c.step();
                }
                let what = format!("{name}, mode {mode}, raised at cycle {at}");
                assert_eq!(c.read_register(10), 1, "handler ran once: {what}");
                assert_eq!(c.read_register(11), 0x8000_000b, "{what}");
                let mepc = c.read_register(12);
                assert!((0x1c..0x1c + 4 * idle.len() as u32).contains(&mepc), "mepc {mepc:#x} in the idle loop: {what}");
                assert_eq!(c.sys.trap_depth, 0, "returned: {what}");
                assert!(c.sys.csrs.read(addr::MSTATUS) & 8 != 0, "MIE back on: {what}");
                // back in the idle loop (the fetch PC runs a few words ahead of a jump in EX)
                assert!((0x1c..0x80).contains(&c.pc), "back in the loop, pc {:#x}: {what}", c.pc);
            }
        }
    }
}
