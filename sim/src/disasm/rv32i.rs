//! RV32I base ISA disassembly. The 10 opcode classes from `controlData.ts`,
//! minus SYSTEM (`1110011`) and MISC-MEM (`0001111`) which live in
//! `system.rs` and `misc_mem.rs` so the M / Zicsr extensions can override them.

use super::{r, Disasm};
use crate::data::imm_data::{extract_immediate, ImmSel};

pub fn decode(instr: u32) -> Option<Disasm> {
    if instr == 0 || instr == 0x0000_0013 { return Some(Disasm::nop()); }

    let opcode = (instr & 0x7f) as u8;
    let rd     = ((instr >>  7) & 0x1f) as u8;
    let f3     = ((instr >> 12) & 0x07) as u8;
    let rs1    = ((instr >> 15) & 0x1f) as u8;
    let rs2    = ((instr >> 20) & 0x1f) as u8;
    let f7     = ((instr >> 25) & 0x7f) as u8;

    Some(match opcode {
        0b0110011 => { // R-type (M is split out into m.rs)
            let mnem: &'static str = match (f7, f3) {
                (0b0000000, 0b000) => "ADD",  (0b0100000, 0b000) => "SUB",
                (0b0000000, 0b001) => "SLL",
                (0b0000000, 0b010) => "SLT",  (0b0000000, 0b011) => "SLTU",
                (0b0000000, 0b100) => "XOR",
                (0b0000000, 0b101) => "SRL",  (0b0100000, 0b101) => "SRA",
                (0b0000000, 0b110) => "OR",   (0b0000000, 0b111) => "AND",
                _ => return None,
            };
            Disasm { class: "R-type", mnemonic: mnem.into(),
                operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)) }
        }
        0b0010011 => { // I-ALU
            let mnem: &'static str = match f3 {
                0b000 => "ADDI", 0b010 => "SLTI", 0b011 => "SLTIU",
                0b100 => "XORI", 0b110 => "ORI",  0b111 => "ANDI",
                0b001 => "SLLI",
                0b101 => if f7 == 0b0100000 { "SRAI" } else { "SRLI" },
                _ => return None,
            };
            let imm = extract_immediate(instr, ImmSel::I);
            let operands = if f3 == 0b001 || f3 == 0b101 {
                format!("{}, {}, {}", r(rd), r(rs1), imm & 0x1f)
            } else {
                format!("{}, {}, {}", r(rd), r(rs1), imm)
            };
            Disasm { class: "I-ALU", mnemonic: mnem.into(), operands }
        }
        0b0000011 => { // Load
            let mnem: &'static str = match f3 {
                0b000 => "LB", 0b001 => "LH", 0b010 => "LW",
                0b100 => "LBU", 0b101 => "LHU", _ => return None,
            };
            let imm = extract_immediate(instr, ImmSel::I);
            Disasm { class: "Load", mnemonic: mnem.into(),
                operands: format!("{}, {}({})", r(rd), imm, r(rs1)) }
        }
        0b0100011 => { // Store
            let mnem: &'static str = match f3 {
                0b000 => "SB", 0b001 => "SH", 0b010 => "SW", _ => return None,
            };
            let imm = extract_immediate(instr, ImmSel::S);
            Disasm { class: "Store", mnemonic: mnem.into(),
                operands: format!("{}, {}({})", r(rs2), imm, r(rs1)) }
        }
        0b1100011 => { // Branch
            let mnem: &'static str = match f3 {
                0b000 => "BEQ", 0b001 => "BNE",
                0b100 => "BLT", 0b101 => "BGE",
                0b110 => "BLTU", 0b111 => "BGEU", _ => return None,
            };
            let imm = extract_immediate(instr, ImmSel::B);
            Disasm { class: "Branch", mnemonic: mnem.into(),
                operands: format!("{}, {}, {}", r(rs1), r(rs2), imm) }
        }
        0b0110111 => Disasm { class: "LUI",   mnemonic: "LUI".into(),
            operands: format!("{}, 0x{:x}", r(rd), (instr >> 12) & 0xfffff) },
        0b0010111 => Disasm { class: "AUIPC", mnemonic: "AUIPC".into(),
            operands: format!("{}, 0x{:x}", r(rd), (instr >> 12) & 0xfffff) },
        0b1101111 => { // JAL
            let imm = extract_immediate(instr, ImmSel::J);
            Disasm { class: "JAL", mnemonic: "JAL".into(),
                operands: format!("{}, {}", r(rd), imm) }
        }
        0b1100111 => { // JALR
            let imm = extract_immediate(instr, ImmSel::I);
            Disasm { class: "JALR", mnemonic: "JALR".into(),
                operands: format!("{}, {}({})", r(rd), imm, r(rs1)) }
        }
        _ => return None,
    })
}
