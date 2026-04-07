//! H extension — hypervisor instructions.
//!
//! All hypervisor instructions live in the SYSTEM opcode `1110011` with
//! `funct3 = 100` (HLV*/HSV* loads/stores) or `funct3 = 000` (fence + ret).

use super::{r, Disasm};

pub fn try_decode(instr: u32) -> Option<Disasm> {
    if instr & 0x7f != 0b1110011 { return None; }
    let funct3 = (instr >> 12) & 0x07;
    let rd     = ((instr >>  7) & 0x1f) as u8;
    let rs1    = ((instr >> 15) & 0x1f) as u8;
    let rs2    = ((instr >> 20) & 0x1f) as u8;
    let funct7 = (instr >> 25) & 0x7f;

    // ── HFENCE.VVMA / HFENCE.GVMA / HRET ─────────────────────────────
    if funct3 == 0 {
        // HRET: imm=0x202 (funct7=0010000, rs2=00010)
        if funct7 == 0b0010000 && rs2 == 0b00010 && rs1 == 0 && rd == 0 {
            return Some(Disasm {
                class: "H", mnemonic: "HRET".into(), operands: String::new(),
            });
        }
        // HFENCE.VVMA rs1, rs2  → funct7 = 0010001
        if funct7 == 0b0010001 && rd == 0 {
            return Some(Disasm {
                class: "H", mnemonic: "HFENCE.VVMA".into(),
                operands: format!("{}, {}", r(rs1), r(rs2)),
            });
        }
        // HFENCE.GVMA rs1, rs2  → funct7 = 0110001
        if funct7 == 0b0110001 && rd == 0 {
            return Some(Disasm {
                class: "H", mnemonic: "HFENCE.GVMA".into(),
                operands: format!("{}, {}", r(rs1), r(rs2)),
            });
        }
        return None;
    }

    // ── HLV* / HSV* loads and stores ─────────────────────────────────
    if funct3 != 0b100 { return None; }

    // HLV.* and HLVX.* — funct7 selects width, rs2 selects unsigned/exec
    let load_form: Option<&'static str> = match (funct7, rs2) {
        (0b0110000, 0b00000) => Some("HLV.B"),
        (0b0110000, 0b00001) => Some("HLV.BU"),
        (0b0110010, 0b00000) => Some("HLV.H"),
        (0b0110010, 0b00001) => Some("HLV.HU"),
        (0b0110010, 0b00011) => Some("HLVX.HU"),
        (0b0110100, 0b00000) => Some("HLV.W"),
        (0b0110100, 0b00001) => Some("HLV.WU"),  // RV64 only — decode anyway
        (0b0110100, 0b00011) => Some("HLVX.WU"),
        (0b0110110, 0b00000) => Some("HLV.D"),   // RV64 only
        _ => None,
    };
    if let Some(mnem) = load_form {
        return Some(Disasm {
            class: "H", mnemonic: mnem.into(),
            operands: format!("{}, ({})", r(rd), r(rs1)),
        });
    }

    // HSV.* — store form (uses rs1 as address, rs2 as value, rd must be 0)
    if rd != 0 { return None; }
    let store_form: Option<&'static str> = match funct7 {
        0b0110001 => Some("HSV.B"),
        0b0110011 => Some("HSV.H"),
        0b0110101 => Some("HSV.W"),
        0b0110111 => Some("HSV.D"),  // RV64
        _ => None,
    };
    store_form.map(|mnem| Disasm {
        class: "H", mnemonic: mnem.into(),
        operands: format!("{}, ({})", r(rs2), r(rs1)),
    })
}
