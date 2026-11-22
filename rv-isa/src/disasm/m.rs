//! RV32M extension — multiply / divide.
//! Shares opcode `0110011` with R-type; distinguished by funct7 = 0000001.

use super::{r, Disasm};

pub fn try_decode(instr: u32) -> Option<Disasm> {
    let opcode = instr & 0x7f;
    let funct7 = (instr >> 25) & 0x7f;
    if opcode != 0b0110011 || funct7 != 0b0000001 { return None; }

    let rd  = ((instr >>  7) & 0x1f) as u8;
    let f3  = ((instr >> 12) & 0x07) as u8;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;

    let mnemonic: &'static str = match f3 {
        0b000 => "MUL",  0b001 => "MULH",
        0b010 => "MULHSU", 0b011 => "MULHU",
        0b100 => "DIV",  0b101 => "DIVU",
        0b110 => "REM",  0b111 => "REMU",
        _ => return None,
    };
    Some(Disasm {
        class: "M",
        mnemonic: mnemonic.into(),
        operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)),
    })
}
