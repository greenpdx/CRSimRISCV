//! RV32I single-cycle simulator + opcode/phase activity verifier.
//!
//! Mirrors the Vue/Pinia visualizer at `src/` (TS) one-to-one:
//! - `data/`        ↔ `src/data/*.ts`        (pure tables + functions)
//! - `components/`  ↔ `src/components/*.vue` (props as struct fields, no display)
//! - `sim/`         ↔ `src/stores/cpuState.ts` (`step()` execution)
//! - `verify/`      expected (phase → active component set) matrix per opcode

pub mod types {
    pub mod cpu;
}

pub mod data {
    pub mod register_data;
    pub mod alu_data;
    pub mod alu_control_data;
    pub mod control_data;
    pub mod imm_data;
    pub mod load_width;
    pub mod netlist;
}

pub mod components {
    pub mod pc_register;
    pub mod pc_adder;
    pub mod alu_unit;
    pub mod alu_control_unit;
    pub mod decoder_control;
    pub mod imm_generator;
    pub mod load_store;
    pub mod io_block;
    pub mod mux_unit;
    pub mod regfile;
    pub mod ram;
    pub mod rom;
}

pub mod sim {
    pub mod phase;
    pub mod trace;
    pub mod micro;
    pub mod trap;
    pub mod system;
    pub mod decode;
    pub mod forwarding;
    pub mod execute;
    pub mod cpu_state;
    pub mod pipeline;
    pub mod barrel;
}

pub mod verify {
    pub mod expected;
    pub mod programs;
}

pub mod disasm;

#[cfg(feature = "wasm")]
pub mod wasm_api;
