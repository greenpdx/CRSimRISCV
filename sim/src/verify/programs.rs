//! One canonical encoded RV32I instruction per opcode class.
//!
//! These are hand-picked simple encodings used by the opcode/phase verifier.
//! They aren't a conformance suite — just enough to drive each control path.

pub struct Canonical {
    pub class:   &'static str,
    pub instr:   u32,
    pub asm:     &'static str,
}

// Encoding helpers (small, only what the canonical set needs).
const fn r(funct7: u32, rs2: u32, rs1: u32, funct3: u32, rd: u32, opcode: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}
const fn i(imm: i32, rs1: u32, funct3: u32, rd: u32, opcode: u32) -> u32 {
    let imm12 = (imm as u32) & 0xfff;
    (imm12 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}
const fn s(imm: i32, rs2: u32, rs1: u32, funct3: u32, opcode: u32) -> u32 {
    let imm12 = (imm as u32) & 0xfff;
    let hi = (imm12 >> 5) & 0x7f;
    let lo = imm12 & 0x1f;
    (hi << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (lo << 7) | opcode
}
const fn b(imm: i32, rs2: u32, rs1: u32, funct3: u32, opcode: u32) -> u32 {
    let imm13 = (imm as u32) & 0x1fff;
    let bit12  = (imm13 >> 12) & 0x1;
    let bit11  = (imm13 >> 11) & 0x1;
    let bit10_5 = (imm13 >> 5) & 0x3f;
    let bit4_1  = (imm13 >> 1) & 0xf;
    (bit12 << 31) | (bit10_5 << 25) | (rs2 << 20) | (rs1 << 15)
        | (funct3 << 12) | (bit4_1 << 8) | (bit11 << 7) | opcode
}
const fn u(imm20: u32, rd: u32, opcode: u32) -> u32 {
    ((imm20 & 0xfffff) << 12) | (rd << 7) | opcode
}
const fn j(imm: i32, rd: u32, opcode: u32) -> u32 {
    let imm21 = (imm as u32) & 0x1f_ffff;
    let bit20    = (imm21 >> 20) & 0x1;
    let bit19_12 = (imm21 >> 12) & 0xff;
    let bit11    = (imm21 >> 11) & 0x1;
    let bit10_1  = (imm21 >> 1)  & 0x3ff;
    (bit20 << 31) | (bit10_1 << 21) | (bit11 << 20) | (bit19_12 << 12) | (rd << 7) | opcode
}

pub const CANONICAL: &[Canonical] = &[
    // ADD x5, x6, x7
    Canonical { class: "R-type", asm: "add x5, x6, x7",
        instr: r(0b0000000, 7, 6, 0b000, 5, 0b0110011) },
    // ADDI x5, x6, 12
    Canonical { class: "I-ALU", asm: "addi x5, x6, 12",
        instr: i(12, 6, 0b000, 5, 0b0010011) },
    // LW x5, 8(x6)
    Canonical { class: "Load", asm: "lw x5, 8(x6)",
        instr: i(8, 6, 0b010, 5, 0b0000011) },
    // SW x7, 12(x6)
    Canonical { class: "Store", asm: "sw x7, 12(x6)",
        instr: s(12, 7, 6, 0b010, 0b0100011) },
    // BEQ x5, x6, 16  (control-path test, taken-or-not irrelevant for active set)
    Canonical { class: "Branch", asm: "beq x5, x6, 16",
        instr: b(16, 6, 5, 0b000, 0b1100011) },
    // LUI x5, 0x12345
    Canonical { class: "LUI", asm: "lui x5, 0x12345",
        instr: u(0x12345, 5, 0b0110111) },
    // AUIPC x5, 0x1
    Canonical { class: "AUIPC", asm: "auipc x5, 0x1",
        instr: u(0x1, 5, 0b0010111) },
    // JAL x1, 16
    Canonical { class: "JAL", asm: "jal x1, 16",
        instr: j(16, 1, 0b1101111) },
    // JALR x1, x5, 4
    Canonical { class: "JALR", asm: "jalr x1, x5, 4",
        instr: i(4, 5, 0b000, 1, 0b1100111) },
    // CSRRW x5, mstatus, x6   (csr 0x300, funct3=001)
    Canonical { class: "CSR", asm: "csrrw x5, mstatus, x6",
        instr: i(0x300, 6, 0b001, 5, 0b1110011) },
];
