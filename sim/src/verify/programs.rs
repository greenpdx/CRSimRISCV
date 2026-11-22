//! One canonical encoded RV32I instruction per opcode class.
//!
//! These are hand-picked simple encodings used by the opcode/phase verifier.
//! They aren't a conformance suite — just enough to drive each control path.

pub struct Canonical {
    pub class:   &'static str,
    pub instr:   u32,
    pub asm:     &'static str,
}

use crate::isa::encode::*;

pub const CANONICAL: &[Canonical] = &[
    // ADD x5, x6, x7
    Canonical { class: "R-type", asm: "add x5, x6, x7",
        instr: add(5, 6, 7) },
    // ADDI x5, x6, 12
    Canonical { class: "I-ALU", asm: "addi x5, x6, 12",
        instr: addi(5, 6, 12) },
    // LW x5, 8(x6)
    Canonical { class: "Load", asm: "lw x5, 8(x6)",
        instr: lw(5, 6, 8) },
    // SW x7, 12(x6)
    Canonical { class: "Store", asm: "sw x7, 12(x6)",
        instr: sw(7, 6, 12) },
    // BEQ x5, x6, 16  (control-path test, taken-or-not irrelevant for active set)
    Canonical { class: "Branch", asm: "beq x5, x6, 16",
        instr: beq(5, 6, 16) },
    // LUI x5, 0x12345
    Canonical { class: "LUI", asm: "lui x5, 0x12345",
        instr: lui(5, 0x12345) },
    // AUIPC x5, 0x1
    Canonical { class: "AUIPC", asm: "auipc x5, 0x1",
        instr: auipc(5, 0x1) },
    // JAL x1, 16
    Canonical { class: "JAL", asm: "jal x1, 16",
        instr: jal(1, 16) },
    // JALR x1, x5, 4
    Canonical { class: "JALR", asm: "jalr x1, x5, 4",
        instr: jalr(1, 5, 4) },
    // CSRRW x5, mstatus, x6   (csr 0x300, funct3=001)
    Canonical { class: "CSR", asm: "csrrw x5, mstatus, x6",
        instr: csrrw(5, 0x300, 6) },
];
