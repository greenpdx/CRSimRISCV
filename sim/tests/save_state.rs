//! Single-cycle save / restore (the panel keeps the core state in the
//! browser as JSON). Needs the `wasm` feature (serde). In the browser the
//! state goes through serde-wasm-bindgen + JSON.stringify / JSON.parse; the
//! JSON is the same as serde_json's (maps have string keys: `str_keys`).
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
    let json = serde_json::to_string(&c).unwrap();
    same(&c, &serde_json::from_str::<CpuState>(&json).unwrap());
}

/// Maps are written with string keys (JSON objects), and read back.
#[test]
fn maps_have_string_keys() {
    let mut c = reference();
    c.mem.ram_extra.insert(0x2_0000, 99);
    let v: serde_json::Value = serde_json::to_value(&c).unwrap();
    assert_eq!(v["mem"]["ram_extra"]["131072"], 99);
    assert!(v["sys"]["csrs"]["regs"].as_object().unwrap().keys().all(|k| k.parse::<u16>().is_ok()));
    same(&c, &serde_json::from_value::<CpuState>(v).unwrap());
}

/// Saves made before memory moved into `mem` (fields at the top level).
#[test]
fn restores_a_save_from_before_mem() {
    // The store (src/stores/cpuState.ts, migrateSave) moves the old top-level
    // memory fields into `mem` before restoring; the same step here.
    let mut v: serde_json::Value = serde_json::from_str(include_str!("fixtures/single_cycle_save_v1.json")).unwrap();
    let o = v.as_object_mut().unwrap();
    assert!(!o.contains_key("mem"));
    let mut mem = serde_json::Map::new();
    for k in ["rom", "ram", "io_data", "io_base", "ram_extra"] {
        if let Some(x) = o.remove(k) { mem.insert(k.into(), x); }
    }
    o.insert("mem".into(), mem.into());
    same(&reference(), &serde_json::from_value::<CpuState>(v).expect("old save restores"));
}
