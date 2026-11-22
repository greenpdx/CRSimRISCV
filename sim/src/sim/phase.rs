//! 9 fine-grained phases of single-cycle execution.
//! Mirrors the sequential structure of `cpuState.ts::step()` lines 212–390.

#[cfg_attr(feature = "wasm", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    IrqCheck, // sample mip & mie; possibly take a trap before fetching
    Fetch,    // pc → rom → instr
    Decode,   // opcode lookup
    Control,  // decoder control signals asserted
    Imm,      // imm_gen extracts immediate
    RegRead,  // regfile rs1/rs2 read
    Alu,      // mux_alusrc + alu (+ branch_adder, jalr target)
    Mem,      // ram / io / loadstore
    Wb,       // mux_wb selects + regfile latches rd  (same clock edge)
    PcUpdate, // mux_pc + pc latch
}

impl Phase {
    pub const ALL: [Phase; 10] = [
        Phase::IrqCheck, Phase::Fetch, Phase::Decode, Phase::Control,
        Phase::Imm, Phase::RegRead, Phase::Alu, Phase::Mem, Phase::Wb, Phase::PcUpdate,
    ];
}

/// Classic 5-stage RISC pipeline grouping for future reuse with `netlist_if/id/ex/ma/wb.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage { IF, ID, EX, MA, WB }

pub fn stage_of(p: Phase) -> Stage {
    match p {
        Phase::IrqCheck | Phase::Fetch => Stage::IF,
        Phase::Decode | Phase::Control | Phase::Imm | Phase::RegRead => Stage::ID,
        Phase::Alu => Stage::EX,
        Phase::Mem => Stage::MA,
        Phase::Wb | Phase::PcUpdate => Stage::WB,
    }
}
