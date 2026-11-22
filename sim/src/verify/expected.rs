//! Expected (Phase → active component set) per opcode class.
//!
//! Each entry is the ground truth that the simulator must reproduce when
//! executing one canonical instruction of that class. The verifier diffs the
//! `ActivityTrace` returned by `CpuState::step_traced()` against this table.

use crate::data::netlist as nl;
use crate::sim::phase::Phase;
use crate::sim::trace::ActivityTrace;
use crate::data::netlist::ComponentId;

#[derive(Debug, Clone, Copy)]
pub struct OpcodeClass(pub &'static str);

pub fn build(rows: &[(Phase, &[ComponentId])]) -> ActivityTrace {
    let mut t = ActivityTrace::new();
    for (phase, comps) in rows {
        for c in *comps { t.mark(*phase, *c); }
    }
    t
}

pub fn r_type() -> ActivityTrace {
    build(&[
        (Phase::Fetch,    &[nl::PC, nl::ROM]),
        (Phase::Decode,   &[nl::DECODER]),
        (Phase::Control,  &[nl::DECODER]),
        // imm_sel = 5 (none) → no Imm phase entry
        (Phase::RegRead,  &[nl::REGFILE]),
        (Phase::Alu,      &[nl::ALU_CTRL, nl::MUX_ALUSRC, nl::ALU]),
        // No Mem phase
        (Phase::Wb,       &[nl::MUX_WB, nl::REGFILE]),
        (Phase::PcUpdate, &[nl::MUX_PC_OFFSET, nl::PC_ADDER, nl::MUX_PC, nl::PC]),
    ])
}

pub fn i_alu() -> ActivityTrace {
    build(&[
        (Phase::Fetch,    &[nl::PC, nl::ROM]),
        (Phase::Decode,   &[nl::DECODER]),
        (Phase::Control,  &[nl::DECODER]),
        (Phase::Imm,      &[nl::IMM_GEN]),
        (Phase::RegRead,  &[nl::REGFILE]),
        (Phase::Alu,      &[nl::ALU_CTRL, nl::MUX_ALUSRC, nl::ALU]),
        (Phase::Wb,       &[nl::MUX_WB, nl::REGFILE]),
        (Phase::PcUpdate, &[nl::MUX_PC_OFFSET, nl::PC_ADDER, nl::MUX_PC, nl::PC]),
    ])
}

pub fn load() -> ActivityTrace {
    build(&[
        (Phase::Fetch,    &[nl::PC, nl::ROM]),
        (Phase::Decode,   &[nl::DECODER]),
        (Phase::Control,  &[nl::DECODER]),
        (Phase::Imm,      &[nl::IMM_GEN]),
        (Phase::RegRead,  &[nl::REGFILE]),
        (Phase::Alu,      &[nl::ALU_CTRL, nl::MUX_ALUSRC, nl::ALU]),
        (Phase::Mem,      &[nl::LOADSTORE, nl::RAM]),
        (Phase::Wb,       &[nl::MUX_WB, nl::REGFILE]),
        (Phase::PcUpdate, &[nl::MUX_PC_OFFSET, nl::PC_ADDER, nl::MUX_PC, nl::PC]),
    ])
}

pub fn store() -> ActivityTrace {
    build(&[
        (Phase::Fetch,    &[nl::PC, nl::ROM]),
        (Phase::Decode,   &[nl::DECODER]),
        (Phase::Control,  &[nl::DECODER]),
        (Phase::Imm,      &[nl::IMM_GEN]),
        (Phase::RegRead,  &[nl::REGFILE]),
        (Phase::Alu,      &[nl::ALU_CTRL, nl::MUX_ALUSRC, nl::ALU]),
        (Phase::Mem,      &[nl::LOADSTORE, nl::RAM]),
        // reg_write=false → no Wb phase
        (Phase::PcUpdate, &[nl::MUX_PC_OFFSET, nl::PC_ADDER, nl::MUX_PC, nl::PC]),
    ])
}

pub fn branch() -> ActivityTrace {
    build(&[
        (Phase::Fetch,    &[nl::PC, nl::ROM]),
        (Phase::Decode,   &[nl::DECODER]),
        (Phase::Control,  &[nl::DECODER]),
        (Phase::Imm,      &[nl::IMM_GEN]),
        (Phase::RegRead,  &[nl::REGFILE]),
        (Phase::Alu,      &[nl::ALU_CTRL, nl::MUX_ALUSRC, nl::ALU]),
        (Phase::PcUpdate, &[nl::MUX_PC_OFFSET, nl::PC_ADDER, nl::MUX_PC, nl::PC]),
    ])
}

pub fn lui() -> ActivityTrace { i_alu() } // same active set as I-ALU
pub fn auipc() -> ActivityTrace { i_alu() }

pub fn jal() -> ActivityTrace {
    build(&[
        (Phase::Fetch,    &[nl::PC, nl::ROM]),
        (Phase::Decode,   &[nl::DECODER]),
        (Phase::Control,  &[nl::DECODER]),
        (Phase::Imm,      &[nl::IMM_GEN]),
        (Phase::RegRead,  &[nl::REGFILE]),
        (Phase::Alu,      &[nl::ALU_CTRL, nl::MUX_ALUSRC, nl::ALU]),
        (Phase::Wb,       &[nl::MUX_WB, nl::REGFILE]),
        (Phase::PcUpdate, &[nl::MUX_PC_OFFSET, nl::PC_ADDER, nl::MUX_PC, nl::PC]),
    ])
}

pub fn jalr() -> ActivityTrace { jal() }

pub fn csr() -> ActivityTrace {
    build(&[
        (Phase::Fetch,    &[nl::PC, nl::ROM]),
        (Phase::Decode,   &[nl::DECODER]),
        (Phase::Control,  &[nl::DECODER]),
        (Phase::RegRead,  &[nl::REGFILE]),
        (Phase::Alu,      &[nl::ALU_CTRL, nl::MUX_ALUSRC, nl::ALU]),
        (Phase::Wb,       &[nl::MUX_WB, nl::REGFILE]),
        (Phase::PcUpdate, &[nl::MUX_PC_OFFSET, nl::PC_ADDER, nl::MUX_PC, nl::PC]),
    ])
}
