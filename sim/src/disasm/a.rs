//! RV32A extension — atomic memory operations.
//! Opcode `0101111`. funct3=010 selects the .W (32-bit) form.
//! Layout: instr[31:27]=funct5, [26]=aq, [25]=rl, [24:20]=rs2, [19:15]=rs1,
//!         [14:12]=funct3, [11:7]=rd.

use super::{r, Disasm};

pub fn try_decode(instr: u32) -> Option<Disasm> {
    let opcode = instr & 0x7f;
    if opcode != 0b0101111 { return None; }

    let rd     = ((instr >>  7) & 0x1f) as u8;
    let funct3 = (instr >> 12) & 0x07;
    let rs1    = ((instr >> 15) & 0x1f) as u8;
    let rs2    = ((instr >> 20) & 0x1f) as u8;
    let rl     = (instr >> 25) & 0x1;
    let aq     = (instr >> 26) & 0x1;
    let funct5 = (instr >> 27) & 0x1f;

    if funct3 != 0b010 { return None; } // RV32 only has .W variants

    let (base, takes_rs2): (&'static str, bool) = match funct5 {
        0b00010 => ("LR.W",       false),
        0b00011 => ("SC.W",       true),
        0b00001 => ("AMOSWAP.W",  true),
        0b00000 => ("AMOADD.W",   true),
        0b00100 => ("AMOXOR.W",   true),
        0b01100 => ("AMOAND.W",   true),
        0b01000 => ("AMOOR.W",    true),
        0b10000 => ("AMOMIN.W",   true),
        0b10100 => ("AMOMAX.W",   true),
        0b11000 => ("AMOMINU.W",  true),
        0b11100 => ("AMOMAXU.W",  true),
        _ => return None,
    };

    // Acquire/release suffix: ".aq", ".rl", ".aqrl", or none.
    let suffix = match (aq, rl) {
        (1, 1) => ".aqrl",
        (1, 0) => ".aq",
        (0, 1) => ".rl",
        _      => "",
    };
    // We can't dynamically build a &'static str, so the suffix lives only in operands.
    let operands = if takes_rs2 {
        format!("{}, {}, ({}){}", r(rd), r(rs2), r(rs1), suffix)
    } else {
        format!("{}, ({}){}", r(rd), r(rs1), suffix)
    };

    Some(Disasm { class: "A", mnemonic: base.into(), operands })
}
