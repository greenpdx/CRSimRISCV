//! Zk — scalar cryptography. Covers RV32:
//!   Zkne / Zknd  — AES32 encrypt / decrypt (4 instructions, with byte-select)
//!   Zknh         — SHA-256 / SHA-512 helpers
//!   Zksh         — SM3 hash compression
//!   Zksed        — SM4 encrypt / key schedule (with byte-select)
//!   Zbkb         — bit-manipulation extras (PACK/PACKH/BREV8)
//!   Zbkx         — crossbar permutations (XPERM4/XPERM8)
//!
//! Zkr (entropy) is CSR-based — see `csrs.rs::seed (0x015)`.
//!
//! All instructions live in opcode `0110011` (R-type) or `0010011` (I-form
//! single-input). The AES/SM4 instructions encode a 2-bit byte-select `bs`
//! in the upper 2 bits of funct7.

use super::{r, Disasm};

pub fn try_decode(instr: u32) -> Option<Disasm> {
    match instr & 0x7f {
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

    // ── AES32 / SM4 — funct7 = bs[1:0] | funct5[4:0], funct3 = 000 ─────
    if f3 == 0 {
        let bs     = (f7 >> 5) & 0x3;
        let funct5 = f7 & 0x1f;
        let aes_sm4: Option<(&'static str, &'static str)> = match funct5 {
            0b10001 => Some(("Zkne", "AES32ESI")),
            0b10011 => Some(("Zkne", "AES32ESMI")),
            0b10101 => Some(("Zknd", "AES32DSI")),
            0b10111 => Some(("Zknd", "AES32DSMI")),
            0b11000 => Some(("Zksed","SM4ED")),
            0b11010 => Some(("Zksed","SM4KS")),
            _ => None,
        };
        if let Some((cls, mnem)) = aes_sm4 {
            return Some(Disasm { class: cls, mnemonic: mnem.into(),
                operands: format!("{}, {}, {}, {}", r(rd), r(rs1), r(rs2), bs) });
        }
    }

    // ── SHA-512 (RV32 only — uses 6 R-type funct7 patterns, f3=000) ────
    if f3 == 0 {
        let sha512: Option<&'static str> = match f7 {
            0b0101000 => Some("SHA512SUM0R"),
            0b0101001 => Some("SHA512SUM1R"),
            0b0101010 => Some("SHA512SIG0L"),
            0b0101011 => Some("SHA512SIG1L"),
            0b0101110 => Some("SHA512SIG0H"),
            0b0101111 => Some("SHA512SIG1H"),
            _ => None,
        };
        if let Some(mnem) = sha512 {
            return Some(Disasm { class: "Zknh", mnemonic: mnem.into(),
                operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)) });
        }
    }

    // ── Zbkb extras: PACK / PACKH ──────────────────────────────────────
    // PACK : f7=0000100, f3=100, rs2 != 0 (rs2=0 is ZEXT.H from Zbb)
    if f7 == 0b0000100 && f3 == 0b100 && rs2 != 0 {
        return Some(Disasm { class: "Zbkb", mnemonic: "PACK".into(),
            operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)) });
    }
    // PACKH : f7=0000100, f3=111
    if f7 == 0b0000100 && f3 == 0b111 {
        return Some(Disasm { class: "Zbkb", mnemonic: "PACKH".into(),
            operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)) });
    }

    // ── Zbkx: XPERM4 / XPERM8 ──────────────────────────────────────────
    if f7 == 0b0010100 && f3 == 0b010 {
        return Some(Disasm { class: "Zbkx", mnemonic: "XPERM4".into(),
            operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)) });
    }
    if f7 == 0b0010100 && f3 == 0b100 {
        return Some(Disasm { class: "Zbkx", mnemonic: "XPERM8".into(),
            operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)) });
    }

    None
}

fn decode_op_imm(instr: u32) -> Option<Disasm> {
    let rd  = ((instr >>  7) & 0x1f) as u8;
    let f3  = (instr >> 12) & 0x07;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8; // op-selector
    let f7  = (instr >> 25) & 0x7f;

    // SHA-256 / SM3 — single-input, funct7=0001000, f3=001, rs2 selects op
    if f7 == 0b0001000 && f3 == 0b001 {
        let (cls, mnem): (&'static str, &'static str) = match rs2 {
            0b00000 => ("Zknh", "SHA256SUM0"),
            0b00001 => ("Zknh", "SHA256SUM1"),
            0b00010 => ("Zknh", "SHA256SIG0"),
            0b00011 => ("Zknh", "SHA256SIG1"),
            0b01000 => ("Zksh", "SM3P0"),
            0b01001 => ("Zksh", "SM3P1"),
            _ => return None,
        };
        return Some(Disasm { class: cls, mnemonic: mnem.into(),
            operands: format!("{}, {}", r(rd), r(rs1)) });
    }

    // BREV8 (Zbkb): funct7=0110100, f3=101, rs2=00111
    if f7 == 0b0110100 && f3 == 0b101 && rs2 == 0b00111 {
        return Some(Disasm { class: "Zbkb", mnemonic: "BREV8".into(),
            operands: format!("{}, {}", r(rd), r(rs1)) });
    }

    None
}
