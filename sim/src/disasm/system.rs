//! SYSTEM opcode `1110011` — privileged + Zicsr.
//!
//! funct3 = 000:
//!   imm[11:0] selects the system instruction:
//!     0x000  ECALL
//!     0x001  EBREAK
//!     0x102  SRET            (funct7 = 0001000, rs2 = 00010)
//!     0x302  MRET            (funct7 = 0011000, rs2 = 00010)
//!     0x105  WFI             (funct7 = 0001000, rs2 = 00101)
//!     funct7 = 0001001 + rs2 = vaddr → SFENCE.VMA
//! funct3 = 001..111:
//!   Zicsr (CSRRW/CSRRS/CSRRC, immediate variants)

use super::{r, Disasm};
use crate::disasm::csrs::csr_name;

pub fn decode(instr: u32) -> Option<Disasm> {
    if instr & 0x7f != 0b1110011 { return None; }

    let rd     = ((instr >>  7) & 0x1f) as u8;
    let funct3 = (instr >> 12) & 0x07;
    let rs1    = ((instr >> 15) & 0x1f) as u8;
    let rs2    = ((instr >> 20) & 0x1f) as u8;
    let funct7 = (instr >> 25) & 0x7f;
    let imm12  = (instr >> 20) & 0xfff;

    if funct3 == 0 {
        // System / privileged instructions
        let mnemonic: &'static str = match (funct7, rs2, rs1, rd) {
            (0b0000000, 0b00000, 0, 0) => "ECALL",
            (0b0000000, 0b00001, 0, 0) => "EBREAK",
            (0b0001000, 0b00010, 0, 0) => "SRET",
            (0b0011000, 0b00010, 0, 0) => "MRET",
            (0b0001000, 0b00101, 0, 0) => "WFI",
            (0b0001001, _,       _, 0) => {
                return Some(Disasm {
                    class: "Privileged",
                    mnemonic: "SFENCE.VMA".into(),
                    operands: format!("{}, {}", r(rs1), r(rs2)),
                });
            }
            _ => return None,
        };
        return Some(Disasm { class: "System", mnemonic: mnemonic.into(), operands: String::new() });
    }

    // Zicsr — funct3 ∈ {001..111}
    let mnemonic: &'static str = match funct3 {
        0b001 => "CSRRW",  0b010 => "CSRRS",  0b011 => "CSRRC",
        0b101 => "CSRRWI", 0b110 => "CSRRSI", 0b111 => "CSRRCI",
        _ => return None,
    };
    // Always show both the symbolic name (if known) and the raw 12-bit address.
    let csr_str: String = match csr_name(imm12 as u16) {
        Some(name) => format!("{}(0x{:03x})", name, imm12),
        None       => format!("0x{:03x}", imm12),
    };
    let operands = if funct3 & 0b100 != 0 {
        // Immediate forms: rs1 field is a zero-extended 5-bit immediate (uimm)
        let uimm = rs1;
        format!("{}, {}, {}", r(rd), csr_str, uimm)
    } else {
        format!("{}, {}, {}", r(rd), csr_str, r(rs1))
    };
    Some(Disasm { class: "Zicsr", mnemonic: mnemonic.into(), operands })
}
