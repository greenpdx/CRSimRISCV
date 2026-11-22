//! RISC-V cores for the visualizer, and the wasm API the panels use.
//!
//! - `isa`         the instruction set (crate `rv-isa`)
//! - `data/`       single-cycle tables: decoder, ALU control, ALU, component ids
//! - `sim/`        the cores (single-cycle, pipeline, barrel, out-of-order),
//!                 the system unit, the memory hierarchy
//! - `verify/`     expected (phase → active component set) matrix per opcode

/// The instruction set (fields, immediates, load / store lanes, MUL, CSRs, registers).
pub use rv_isa as isa;

pub mod data {
    pub mod alu_data;
    pub mod alu_control_data;
    pub mod control_data;
    pub mod netlist;
}

pub mod sim {
    pub mod core;
    pub mod phase;
    pub mod trace;
    pub mod micro;
    pub mod system;
    pub mod mem;
    pub mod decode;
    pub mod forwarding;
    pub mod stage;
    pub mod execute;
    pub mod cpu_state;
    pub mod pipeline;
    pub mod barrel;
    pub mod memsys;
    pub mod ooo;
    #[cfg(feature = "wasm")]
    pub mod str_keys;
}

pub mod verify {
    pub mod expected;
    pub mod programs;
}


#[cfg(feature = "wasm")]
pub mod wasm;
