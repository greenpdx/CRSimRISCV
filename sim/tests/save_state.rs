//! Single-cycle save / restore (the panel keeps the core state in the
//! browser). Needs the `wasm` feature (serde).
#![cfg(feature = "wasm")]

use cpu_sim::sim::cpu_state::CpuState;

/// The state `fixtures/single_cycle_save_v1.json` holds: two instructions
/// (addi x5, x0, 128; sw x6, 0(x5)) after 25 micro-steps.
fn reference() -> CpuState {
    let mut c = CpuState::new();
    c.load_program(&[0x0800_0293, 0x0062_a023]);
    for _ in 0..25 { c.micro_step(); }
    c
}

fn same(a: &CpuState, b: &CpuState) {
    assert_eq!(format!("{:?}", a.mem), format!("{:?}", b.mem));
    assert_eq!((a.pc, a.registers, a.cycle_count), (b.pc, b.registers, b.cycle_count));
    assert_eq!(format!("{:?} {:?}", a.sys, a.micro), format!("{:?} {:?}", b.sys, b.micro));
}

#[test]
fn save_round_trip() {
    let c = reference();
    same(&c, &CpuState::from_json(&serde_json::to_string(&c).unwrap()).unwrap());
}

/// Saves made before memory moved into `mem` (fields at the top level).
#[test]
fn restores_a_save_from_before_mem() {
    let old = include_str!("fixtures/single_cycle_save_v1.json");
    same(&reference(), &CpuState::from_json(old).expect("old save restores"));
}
