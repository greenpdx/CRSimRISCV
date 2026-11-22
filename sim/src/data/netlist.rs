//! Mirrors `src/data/netlist.ts` — single-cycle component IDs.
//!
//! Stable string IDs used as keys in the ActivityTrace and the verifier's
//! expected matrix. The full Vue netlist additionally carries port/net
//! topology; the simulator only needs the component identifier set, since
//! per-phase activation is driven by `step()` rather than by topology walking.

/// Stable component identifier (key of the activity trace).
pub type ComponentId = &'static str;

pub const PC:             ComponentId = "pc";
pub const ROM:            ComponentId = "rom";
pub const DECODER:        ComponentId = "decoder";
pub const IMM_GEN:        ComponentId = "imm_gen";
pub const REGFILE:        ComponentId = "regfile";
pub const ALU_CTRL:       ComponentId = "alu_ctrl";
pub const MUX_ALUSRC:     ComponentId = "mux_alusrc";
pub const ALU:            ComponentId = "alu";
pub const LOADSTORE:      ComponentId = "loadstore";
pub const RAM:            ComponentId = "ram";
pub const IO:             ComponentId = "io";
pub const MUX_WB:         ComponentId = "mux_wb";
pub const PC_ADDER:       ComponentId = "pc_adder";
pub const MUX_PC_OFFSET:  ComponentId = "mux_pc_offset";
pub const MUX_PC:         ComponentId = "mux_pc";

pub const ALL_COMPONENTS: &[ComponentId] = &[
    PC, ROM, DECODER, IMM_GEN, REGFILE, ALU_CTRL, MUX_ALUSRC, ALU,
    LOADSTORE, RAM, IO, MUX_WB, PC_ADDER, MUX_PC_OFFSET, MUX_PC,
];
