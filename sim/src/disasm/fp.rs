//! Floating-point disassembly: F (single), D (double), Zfh (half), Q (quad).
//!
//! Six opcodes:
//!   `0000111` LOAD-FP    — FLW / FLD / FLH / FLQ
//!   `0100111` STORE-FP   — FSW / FSD / FSH / FSQ
//!   `1000011` MADD       — FMADD.{S,D,H,Q}
//!   `1000111` MSUB       — FMSUB.{S,D,H,Q}
//!   `1001011` NMSUB      — FNMSUB.{S,D,H,Q}
//!   `1001111` NMADD      — FNMADD.{S,D,H,Q}
//!   `1010011` OP-FP      — all FP arithmetic, conversions, classify, sign-injection
//!
//! `fmt` field is instr[26:25]:  00=S, 01=D, 10=H, 11=Q.

use super::{fr, r, Disasm};
use crate::data::imm_data::{extract_immediate, ImmSel};

/// Format suffix for the four IEEE widths supported by RISC-V.
fn fmt_suffix(fmt: u32) -> Option<(&'static str, &'static str)> {
    Some(match fmt {
        0b00 => (".S", "F"),
        0b01 => (".D", "D"),
        0b10 => (".H", "Zfh"),
        0b11 => (".Q", "Q"),
        _ => return None,
    })
}

/// Class string for a memory FP instruction based on its funct3 width.
fn width_for_funct3(f3: u32) -> Option<(&'static str, &'static str)> {
    Some(match f3 {
        0b001 => ("H", "Zfh"),  // FLH/FSH
        0b010 => ("W", "F"),    // FLW/FSW
        0b011 => ("D", "D"),    // FLD/FSD
        0b100 => ("Q", "Q"),    // FLQ/FSQ
        _ => return None,
    })
}

pub fn try_decode(instr: u32) -> Option<Disasm> {
    match instr & 0x7f {
        0b0000111 => decode_load_fp(instr),
        0b0100111 => decode_store_fp(instr),
        0b1000011 => decode_fmadd(instr, "FMADD"),
        0b1000111 => decode_fmadd(instr, "FMSUB"),
        0b1001011 => decode_fmadd(instr, "FNMSUB"),
        0b1001111 => decode_fmadd(instr, "FNMADD"),
        0b1010011 => decode_op_fp(instr),
        _ => None,
    }
}

fn decode_load_fp(instr: u32) -> Option<Disasm> {
    let f3  = (instr >> 12) & 0x07;
    let (w, ext) = width_for_funct3(f3)?;
    let rd  = ((instr >>  7) & 0x1f) as u8;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let imm = extract_immediate(instr, ImmSel::I);
    Some(Disasm {
        class: ext,
        mnemonic: format!("FL{}", w),
        operands: format!("{}, {}({})", fr(rd), imm, r(rs1)),
    })
}

fn decode_store_fp(instr: u32) -> Option<Disasm> {
    let f3  = (instr >> 12) & 0x07;
    let (w, ext) = width_for_funct3(f3)?;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;
    let imm = extract_immediate(instr, ImmSel::S);
    Some(Disasm {
        class: ext,
        mnemonic: format!("FS{}", w),
        operands: format!("{}, {}({})", fr(rs2), imm, r(rs1)),
    })
}

fn decode_fmadd(instr: u32, base: &str) -> Option<Disasm> {
    let rd  = ((instr >>  7) & 0x1f) as u8;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;
    let rs3 = ((instr >> 27) & 0x1f) as u8;
    let fmt = (instr >> 25) & 0x3;
    let (suf, ext) = fmt_suffix(fmt)?;
    Some(Disasm {
        class: ext,
        mnemonic: format!("{}{}", base, suf),
        operands: format!("{}, {}, {}, {}", fr(rd), fr(rs1), fr(rs2), fr(rs3)),
    })
}

fn decode_op_fp(instr: u32) -> Option<Disasm> {
    let rd  = ((instr >>  7) & 0x1f) as u8;
    let f3  = (instr >> 12) & 0x07;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;
    let f7  = (instr >> 25) & 0x7f;
    let fmt = f7 & 0b11;
    let op  = f7 >> 2;
    let (suf, ext) = fmt_suffix(fmt)?;

    // Two-operand FP-FP arithmetic
    let arith_base: Option<&'static str> = match op {
        0b00000 => Some("FADD"),
        0b00001 => Some("FSUB"),
        0b00010 => Some("FMUL"),
        0b00011 => Some("FDIV"),
        _ => None,
    };
    if let Some(base) = arith_base {
        return Some(Disasm { class: ext, mnemonic: format!("{}{}", base, suf),
            operands: format!("{}, {}, {}", fr(rd), fr(rs1), fr(rs2)) });
    }

    // FSQRT — single operand, rs2 must be 0
    if op == 0b01011 && rs2 == 0 {
        return Some(Disasm { class: ext, mnemonic: format!("FSQRT{}", suf),
            operands: format!("{}, {}", fr(rd), fr(rs1)) });
    }

    // FSGNJ family
    if op == 0b00100 {
        let mnem = match f3 {
            0b000 => "FSGNJ",
            0b001 => "FSGNJN",
            0b010 => "FSGNJX",
            _ => return None,
        };
        return Some(Disasm { class: ext, mnemonic: format!("{}{}", mnem, suf),
            operands: format!("{}, {}, {}", fr(rd), fr(rs1), fr(rs2)) });
    }

    // FMIN / FMAX
    if op == 0b00101 {
        let mnem = match f3 {
            0b000 => "FMIN",
            0b001 => "FMAX",
            _ => return None,
        };
        return Some(Disasm { class: ext, mnemonic: format!("{}{}", mnem, suf),
            operands: format!("{}, {}, {}", fr(rd), fr(rs1), fr(rs2)) });
    }

    // Compares
    if op == 0b10100 {
        let mnem = match f3 {
            0b000 => "FLE",
            0b001 => "FLT",
            0b010 => "FEQ",
            _ => return None,
        };
        return Some(Disasm { class: ext, mnemonic: format!("{}{}", mnem, suf),
            operands: format!("{}, {}, {}", r(rd), fr(rs1), fr(rs2)) });
    }

    // FCVT FP→int
    if op == 0b11000 {
        let int_kind = match rs2 {
            0b00000 => "W",
            0b00001 => "WU",
            0b00010 => "L",   // RV64 only, decode anyway
            0b00011 => "LU",
            _ => return None,
        };
        return Some(Disasm {
            class: ext,
            mnemonic: format!("FCVT.{}{}", int_kind, suf),
            operands: format!("{}, {}", r(rd), fr(rs1)),
        });
    }

    // FCVT int→FP
    if op == 0b11010 {
        let int_kind = match rs2 {
            0b00000 => "W",
            0b00001 => "WU",
            0b00010 => "L",
            0b00011 => "LU",
            _ => return None,
        };
        return Some(Disasm {
            class: ext,
            mnemonic: format!("FCVT{}.{}", suf, int_kind),
            operands: format!("{}, {}", fr(rd), r(rs1)),
        });
    }

    // FCVT FP→FP (between formats): op=01000, rs2 selects source format
    if op == 0b01000 {
        let (src_suf, _) = fmt_suffix(rs2 as u32 & 0b11)?;
        return Some(Disasm {
            class: ext,
            mnemonic: format!("FCVT{}{}", suf, src_suf),
            operands: format!("{}, {}", fr(rd), fr(rs1)),
        });
    }

    // FMV.X.W / FCLASS  (op=11100, rs2=0)
    if op == 0b11100 && rs2 == 0 {
        let int_w_suf = match fmt {
            0b00 => ".X.W",
            0b01 => ".X.D",
            0b10 => ".X.H",
            _ => return None,
        };
        let mnem = match f3 {
            0b000 => format!("FMV{}", int_w_suf),
            0b001 => format!("FCLASS{}", suf),
            _ => return None,
        };
        return Some(Disasm { class: ext, mnemonic: mnem,
            operands: format!("{}, {}", r(rd), fr(rs1)) });
    }

    // FMV.W.X / FMV.D.X / FMV.H.X (op=11110, f3=000, rs2=0)
    if op == 0b11110 && f3 == 0 && rs2 == 0 {
        let mnem = match fmt {
            0b00 => "FMV.W.X",
            0b01 => "FMV.D.X",
            0b10 => "FMV.H.X",
            _ => return None,
        };
        return Some(Disasm { class: ext, mnemonic: mnem.into(),
            operands: format!("{}, {}", fr(rd), r(rs1)) });
    }

    None
}
