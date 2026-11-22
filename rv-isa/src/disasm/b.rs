//! RV32B (bit-manipulation) — Zba + Zbb + Zbc + Zbs.
//!
//! These extensions all share opcodes `0110011` (R-type) and `0010011`
//! (I-ALU) with the base ISA, distinguished by funct7 / funct3 / rs2.

use super::{r, Disasm};

pub fn try_decode(instr: u32) -> Option<Disasm> {
    let opcode = instr & 0x7f;
    match opcode {
        0b0110011 => decode_op(instr),
        0b0010011 => decode_op_imm(instr),
        _ => None,
    }
}

fn decode_op(instr: u32) -> Option<Disasm> {
    let rd  = ((instr >>  7) & 0x1f) as u8;
    let f3  = (instr >> 12) & 0x07;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;
    let f7  = (instr >> 25) & 0x7f;

    // (funct7, funct3) → (mnemonic, extension)
    let (mnemonic, ext): (&'static str, &'static str) = match (f7, f3) {
        // ── Zba ────────────────────────────────────────────────────
        (0b0010000, 0b010) => ("SH1ADD", "Zba"),
        (0b0010000, 0b100) => ("SH2ADD", "Zba"),
        (0b0010000, 0b110) => ("SH3ADD", "Zba"),
        // ── Zbb (R-form) ───────────────────────────────────────────
        (0b0100000, 0b111) => ("ANDN",  "Zbb"),
        (0b0100000, 0b110) => ("ORN",   "Zbb"),
        (0b0100000, 0b100) => ("XNOR",  "Zbb"),
        (0b0000101, 0b110) => ("MAX",   "Zbb"),
        (0b0000101, 0b111) => ("MAXU",  "Zbb"),
        (0b0000101, 0b100) => ("MIN",   "Zbb"),
        (0b0000101, 0b101) => ("MINU",  "Zbb"),
        (0b0110000, 0b001) => ("ROL",   "Zbb"),
        (0b0110000, 0b101) => ("ROR",   "Zbb"),
        // ZEXT.H (RV32 form): funct7=0000100 funct3=100 rs2=00000
        (0b0000100, 0b100) if rs2 == 0 => {
            return Some(Disasm { class: "Zbb", mnemonic: "ZEXT.H".into(),
                operands: format!("{}, {}", r(rd), r(rs1)) });
        }
        // ── Zbc ────────────────────────────────────────────────────
        (0b0000101, 0b001) => ("CLMUL",  "Zbc"),
        (0b0000101, 0b010) => ("CLMULR", "Zbc"),
        (0b0000101, 0b011) => ("CLMULH", "Zbc"),
        // ── Zbs ────────────────────────────────────────────────────
        (0b0100100, 0b001) => ("BCLR", "Zbs"),
        (0b0100100, 0b101) => ("BEXT", "Zbs"),
        (0b0110100, 0b001) => ("BINV", "Zbs"),
        (0b0010100, 0b001) => ("BSET", "Zbs"),
        _ => return None,
    };
    Some(Disasm { class: ext, mnemonic: mnemonic.into(),
        operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)) })
}

fn decode_op_imm(instr: u32) -> Option<Disasm> {
    let rd  = ((instr >>  7) & 0x1f) as u8;
    let f3  = (instr >> 12) & 0x07;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;   // also acts as shamt
    let f7  = (instr >> 25) & 0x7f;
    let shamt = rs2;

    // ── Zbb single-input unary ops with funct7=0110000 ─────────────
    if f7 == 0b0110000 && f3 == 0b001 {
        let mnemonic: Option<&'static str> = match rs2 {
            0b00000 => Some("CLZ"),
            0b00001 => Some("CTZ"),
            0b00010 => Some("CPOP"),
            0b00100 => Some("SEXT.B"),
            0b00101 => Some("SEXT.H"),
            _ => None,
        };
        if let Some(mnem) = mnemonic {
            return Some(Disasm { class: "Zbb", mnemonic: mnem.into(),
                operands: format!("{}, {}", r(rd), r(rs1)) });
        }
    }

    // RORI: funct7=0110000 funct3=101  shamt in rs2
    if f7 == 0b0110000 && f3 == 0b101 {
        return Some(Disasm { class: "Zbb", mnemonic: "RORI".into(),
            operands: format!("{}, {}, {}", r(rd), r(rs1), shamt) });
    }

    // ORC.B: funct7=0010100 funct3=101 rs2=00111
    if f7 == 0b0010100 && f3 == 0b101 && rs2 == 0b00111 {
        return Some(Disasm { class: "Zbb", mnemonic: "ORC.B".into(),
            operands: format!("{}, {}", r(rd), r(rs1)) });
    }

    // REV8 (RV32): funct7=0110100 funct3=101 rs2=11000  (imm12=0x698)
    if f7 == 0b0110100 && f3 == 0b101 && rs2 == 0b11000 {
        return Some(Disasm { class: "Zbb", mnemonic: "REV8".into(),
            operands: format!("{}, {}", r(rd), r(rs1)) });
    }

    // ── Zbs immediate single-bit ops ───────────────────────────────
    let zbs: Option<&'static str> = match (f7, f3) {
        (0b0100100, 0b001) => Some("BCLRI"),
        (0b0100100, 0b101) => Some("BEXTI"),
        (0b0110100, 0b001) => Some("BINVI"),
        (0b0010100, 0b001) => Some("BSETI"),
        _ => None,
    };
    if let Some(mnem) = zbs {
        return Some(Disasm { class: "Zbs", mnemonic: mnem.into(),
            operands: format!("{}, {}, {}", r(rd), r(rs1), shamt) });
    }

    None
}
