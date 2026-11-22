//! Every core through the common `Core` interface: load, run, registers,
//! CSRs, reset.

use cpu_sim::isa::encode::*;
use cpu_sim::sim::barrel::BarrelState;
use cpu_sim::sim::core::Core;
use cpu_sim::sim::cpu_state::CpuState;
use cpu_sim::sim::ooo::{mem::MemMode, OooCpu};
use cpu_sim::sim::pipeline::PipelineState;

/// (core, code base, data base)
fn cores() -> Vec<(Box<dyn Core>, u32, u32)> {
    let mut barrel = BarrelState::new();
    barrel.reset_boot();
    vec![
        (Box::new(CpuState::new()), 0, 0x1_0000),
        (Box::new(PipelineState::new()), 0, 0x1_0000),
        (Box::new(barrel), 0, 0x1000_0000),
        (Box::new(OooCpu::new(MemMode::Fixed)), 0, 0x1000),
    ]
}

/// x3 = 7 * 6 by addition, stored to and loaded back from `db`. Straight
/// line: the single-thread barrel mishandles taken branches (issue #7).
fn program(db: u32) -> Vec<u32> {
    let mut p = vec![lui(31, db >> 12), addi(1, 0, 7), addi(3, 0, 0)];
    p.extend([add(3, 3, 1); 6]);
    p.extend([sw(3, 31, 4), lw(4, 31, 4), csrrw(5, 0x340, 3), SPIN]);   // mscratch ← x3
    p
}

#[test]
fn every_core_runs_the_same_program() {
    for (mut c, code, db) in cores() {
        let name = c.name();
        c.load(code, &program(db));
        let mut retired = 0;
        for _ in 0..200 { if c.step_instr(400) { retired += 1; } }
        assert!(retired >= 20, "{name}: {retired} instructions");
        assert_eq!((c.reg(3), c.reg(4)), (42, 42), "{name}");
        assert_eq!(c.csr_read(0x340), 42, "{name}: mscratch");
        assert_eq!(c.bus().peek(db + 4), 42, "{name}: memory");
        assert!(c.last_trap().is_none(), "{name}");
    }
}

#[test]
fn registers_csrs_and_reset() {
    for (mut c, code, _) in cores() {
        let name = c.name();
        c.load(code, &[NOP, SPIN]);
        c.poke_reg(0, 5);
        c.poke_reg(9, 0x99);
        assert_eq!((c.reg(0), c.reg(9)), (0, 0x99), "{name}: x0 stays 0");
        c.csr_write(0x340, 0x1234);
        assert_eq!(c.csr_snapshot().iter().find(|&&(a, _)| a == 0x340).map(|&(_, v)| v), Some(0x1234), "{name}");
        c.run(30);
        c.reset();
        assert_eq!(c.reg(9), 0, "{name}: reset clears registers");
        assert_eq!(c.bus().peek(code + 4), SPIN, "{name}: reset keeps memory");
    }
}
