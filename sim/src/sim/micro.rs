//! Phase-by-phase ("micro") execution state for the visualizer.
//!
//! `MicroState` carries every intermediate value that the 9 phases produce,
//! so a UI can show the value of each component / bus *as the phases run*.
//!
//! All fields are `Option<u32>` (or `Option<bool>` for 1-bit signals) and
//! start `None`. Each phase sets the values it produces. Resetting at the
//! start of a new instruction clears the lot back to `None`.

use crate::sim::phase::Phase;

#[derive(Debug, Default, Clone)]
pub struct MicroState {
    /// The phase that just executed (None = before Fetch, instruction not started).
    pub current: Option<Phase>,

    // ── Buses (mirror netlist net IDs) ─────────────────────────────────
    pub pc_out:        Option<u32>,
    pub instr:         Option<u32>,
    pub pc_plus4:      Option<u32>,
    pub immediate:     Option<u32>,
    pub rs1_data:      Option<u32>,
    pub rs2_data:      Option<u32>,
    pub alu_b:         Option<u32>,
    pub alu_control:   Option<u8>,
    pub alu_result:    Option<u32>,
    pub mem_read_data: Option<u32>,
    pub write_data:    Option<u32>,
    pub pc_offset:     Option<u32>,
    pub jalr_target:   Option<u32>,
    pub next_pc:       Option<u32>,

    // ── Control signals (1-bit / small) ─────────────────────────────────
    pub ctrl_reg_write: Option<bool>,
    pub ctrl_alu_src:   Option<bool>,
    pub ctrl_mem_read:  Option<bool>,
    pub ctrl_mem_write: Option<bool>,
    pub ctrl_alu_op:    Option<u8>,
    pub ctrl_wb_sel:    Option<u8>,
    pub ctrl_pc_sel:    Option<u8>,
    pub ctrl_imm_sel:   Option<u8>,
    pub zero_flag:      Option<bool>,
    pub neg_flag:       Option<bool>,
    pub carry_flag:     Option<bool>,
    pub overflow_flag:  Option<bool>,

    // ── Decoded fields (latched from instr at Decode) ──────────────────
    pub opcode:  Option<u8>,
    pub rd:      Option<u8>,
    pub funct3:  Option<u8>,
    pub rs1_num: Option<u8>,
    pub rs2_num: Option<u8>,

    // ── IrqCheck phase outputs ─────────────────────────────────────────
    /// True iff the most recent IrqCheck or instruction took a trap.
    pub trap_taken: bool,
}

impl MicroState {
    pub fn reset(&mut self) { *self = MicroState::default(); }
}

/// Order in which `micro_step()` advances. Wraps from `PcUpdate` back to `Fetch`.
pub const MICRO_ORDER: [Phase; 10] = [
    Phase::IrqCheck, Phase::Fetch, Phase::Decode, Phase::Control, Phase::Imm,
    Phase::RegRead, Phase::Alu, Phase::Mem, Phase::Wb, Phase::PcUpdate,
];

pub fn next_phase(cur: Option<Phase>) -> Phase {
    match cur {
        None => Phase::IrqCheck,
        Some(p) => {
            let i = MICRO_ORDER.iter().position(|&x| x == p).unwrap_or(MICRO_ORDER.len() - 1);
            if i + 1 >= MICRO_ORDER.len() { Phase::IrqCheck } else { MICRO_ORDER[i + 1] }
        }
    }
}
