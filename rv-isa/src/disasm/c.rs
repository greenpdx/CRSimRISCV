//! RV32C — 16-bit compressed instructions.
//!
//! Selected by the low two bits of the instruction (the "quadrant"):
//!   00 = Q0   01 = Q1   10 = Q2   11 = uncompressed (handled elsewhere)
//!
//! Compressed register fields (3-bit `rd'`/`rs1'`/`rs2'`) refer to one of
//! `x8..x15` (i.e. `s0, s1, a0, a1, a2, a3, a4, a5`).

use super::{fr, r, Disasm};

#[inline] fn rp(idx3: u16) -> u8 { 8 + (idx3 as u8 & 0x7) }   // x8..x15
#[inline] fn sext(value: u32, bits: u32) -> i32 {
    let s = 32 - bits;
    ((value << s) as i32) >> s
}

pub fn decode16(c: u16) -> Disasm {
    let quad = c & 0x3;
    let funct3 = (c >> 13) & 0x7;
    match quad {
        0b00 => decode_q0(c, funct3),
        0b01 => decode_q1(c, funct3),
        0b10 => decode_q2(c, funct3),
        _    => Disasm::unknown(0xff),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Quadrant 0
// ─────────────────────────────────────────────────────────────────────────
fn decode_q0(c: u16, funct3: u16) -> Disasm {
    let rdp  = rp((c >> 2) & 0x7);
    let rsp1 = rp((c >> 7) & 0x7);
    let rsp2 = rp((c >> 2) & 0x7); // rs2' shares the rd' position

    match funct3 {
        0b000 => { // C.ADDI4SPN — rd', x2, nzuimm
            let nz = (((c >> 7) & 0x30)        // bits[5:4] in c[12:11]
                   |  ((c >> 1) & 0x3c0)       // bits[9:6] in c[10:7]
                   |  ((c >> 4) & 0x4)         // bit[2]  in c[6]
                   |  ((c >> 2) & 0x8)) as u32; // bit[3]  in c[5]
            if nz == 0 { return Disasm { class: "C", mnemonic: "C.ILLEGAL".into(), operands: String::new() }; }
            Disasm { class: "C", mnemonic: "C.ADDI4SPN".into(),
                operands: format!("{}, x2, {}", r(rdp), nz) }
        }
        0b001 => { // C.FLD (RV32DC)
            let imm = ((((c >> 10) & 0x7) << 3) | (((c >> 5) & 0x3) << 6)) as i32;
            Disasm { class: "D", mnemonic: "C.FLD".into(),
                operands: format!("{}, {}({})", fr(rdp), imm, r(rsp1)) }
        }
        0b010 => { // C.LW
            let imm = ((((c >> 10) & 0x7) << 3) | (((c >> 6) & 0x1) << 2) | (((c >> 5) & 0x1) << 6)) as i32;
            Disasm { class: "C", mnemonic: "C.LW".into(),
                operands: format!("{}, {}({})", r(rdp), imm, r(rsp1)) }
        }
        0b011 => { // C.FLW (RV32FC)
            let imm = ((((c >> 10) & 0x7) << 3) | (((c >> 6) & 0x1) << 2) | (((c >> 5) & 0x1) << 6)) as i32;
            Disasm { class: "F", mnemonic: "C.FLW".into(),
                operands: format!("{}, {}({})", fr(rdp), imm, r(rsp1)) }
        }
        0b101 => { // C.FSD
            let imm = ((((c >> 10) & 0x7) << 3) | (((c >> 5) & 0x3) << 6)) as i32;
            Disasm { class: "D", mnemonic: "C.FSD".into(),
                operands: format!("{}, {}({})", fr(rsp2), imm, r(rsp1)) }
        }
        0b110 => { // C.SW
            let imm = ((((c >> 10) & 0x7) << 3) | (((c >> 6) & 0x1) << 2) | (((c >> 5) & 0x1) << 6)) as i32;
            Disasm { class: "C", mnemonic: "C.SW".into(),
                operands: format!("{}, {}({})", r(rsp2), imm, r(rsp1)) }
        }
        0b111 => { // C.FSW
            let imm = ((((c >> 10) & 0x7) << 3) | (((c >> 6) & 0x1) << 2) | (((c >> 5) & 0x1) << 6)) as i32;
            Disasm { class: "F", mnemonic: "C.FSW".into(),
                operands: format!("{}, {}({})", fr(rsp2), imm, r(rsp1)) }
        }
        _ => Disasm { class: "C", mnemonic: "C.RESERVED".into(), operands: String::new() },
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Quadrant 1
// ─────────────────────────────────────────────────────────────────────────
fn ci_imm6(c: u16) -> i32 {
    // imm[5] = c[12], imm[4:0] = c[6:2]
    let raw = (((c >> 12) & 0x1) << 5) | ((c >> 2) & 0x1f);
    sext(raw as u32, 6)
}

fn cj_imm12(c: u16) -> i32 {
    // imm[11|4|9:8|10|6|7|3:1|5]
    let bit = |b: u16, n: u32| (((c >> b) & 1) as u32) << n;
    let v = bit(12, 11) | bit(8, 10) | (((c >> 9) & 0x3) as u32) << 8
          | bit(6, 7)  | bit(7, 6)
          | bit(2, 5)  | bit(11, 4)
          | (((c >> 3) & 0x7) as u32) << 1;
    sext(v, 12)
}

fn cb_imm9(c: u16) -> i32 {
    // imm[8|4:3|7:6|2:1|5]
    let bit = |b: u16, n: u32| (((c >> b) & 1) as u32) << n;
    let v = bit(12, 8) | (((c >> 10) & 0x3) as u32) << 3
          | (((c >> 5) & 0x3) as u32) << 6
          | (((c >> 3) & 0x3) as u32) << 1
          | bit(2, 5);
    sext(v, 9)
}

fn decode_q1(c: u16, funct3: u16) -> Disasm {
    let rd_rs1 = ((c >> 7) & 0x1f) as u8;
    let rdp    = rp((c >> 7) & 0x7);
    let rsp2   = rp((c >> 2) & 0x7);
    let imm6   = ci_imm6(c);

    match funct3 {
        0b000 => { // C.NOP / C.ADDI
            if rd_rs1 == 0 && imm6 == 0 {
                Disasm { class: "C", mnemonic: "C.NOP".into(), operands: String::new() }
            } else {
                Disasm { class: "C", mnemonic: "C.ADDI".into(),
                    operands: format!("{}, {}", r(rd_rs1), imm6) }
            }
        }
        0b001 => { // C.JAL (RV32 only)
            Disasm { class: "C", mnemonic: "C.JAL".into(),
                operands: format!("{}", cj_imm12(c)) }
        }
        0b010 => { // C.LI
            Disasm { class: "C", mnemonic: "C.LI".into(),
                operands: format!("{}, {}", r(rd_rs1), imm6) }
        }
        0b011 => {
            if rd_rs1 == 2 { // C.ADDI16SP
                // imm[9|4|6|8:7|5]
                let bit = |b: u16, n: u32| (((c >> b) & 1) as u32) << n;
                let v = bit(12, 9) | bit(2, 5) | bit(5, 6) | (((c >> 3) & 0x3) as u32) << 7 | bit(6, 4);
                let imm = sext(v, 10);
                Disasm { class: "C", mnemonic: "C.ADDI16SP".into(),
                    operands: format!("x2, {}", imm) }
            } else { // C.LUI
                // imm[17|16:12]
                let bit = |b: u16, n: u32| (((c >> b) & 1) as u32) << n;
                let v = bit(12, 17) | (((c >> 2) & 0x1f) as u32) << 12;
                let imm = sext(v, 18);
                Disasm { class: "C", mnemonic: "C.LUI".into(),
                    operands: format!("{}, 0x{:x}", r(rd_rs1), (imm as u32 >> 12) & 0xfffff) }
            }
        }
        0b100 => { // MISC-ALU group
            let sub = (c >> 10) & 0x3;
            let shamt = (((c >> 12) & 0x1) << 5) | ((c >> 2) & 0x1f);
            match sub {
                0b00 => Disasm { class: "C", mnemonic: "C.SRLI".into(),
                    operands: format!("{}, {}", r(rdp), shamt) },
                0b01 => Disasm { class: "C", mnemonic: "C.SRAI".into(),
                    operands: format!("{}, {}", r(rdp), shamt) },
                0b10 => Disasm { class: "C", mnemonic: "C.ANDI".into(),
                    operands: format!("{}, {}", r(rdp), ci_imm6(c)) },
                0b11 => {
                    // Two-operand R-form: c[12]=0 → SUB/XOR/OR/AND ; c[12]=1 reserved in RV32
                    let inner = (c >> 5) & 0x3;
                    let mnem: &'static str = match ((c >> 12) & 0x1, inner) {
                        (0, 0b00) => "C.SUB",
                        (0, 0b01) => "C.XOR",
                        (0, 0b10) => "C.OR",
                        (0, 0b11) => "C.AND",
                        _ => return Disasm { class: "C", mnemonic: "C.RESERVED".into(), operands: String::new() },
                    };
                    Disasm { class: "C", mnemonic: mnem.into(),
                        operands: format!("{}, {}", r(rdp), r(rsp2)) }
                }
                _ => unreachable!(),
            }
        }
        0b101 => { // C.J
            Disasm { class: "C", mnemonic: "C.J".into(),
                operands: format!("{}", cj_imm12(c)) }
        }
        0b110 => { // C.BEQZ
            Disasm { class: "C", mnemonic: "C.BEQZ".into(),
                operands: format!("{}, {}", r(rdp), cb_imm9(c)) }
        }
        0b111 => { // C.BNEZ
            Disasm { class: "C", mnemonic: "C.BNEZ".into(),
                operands: format!("{}, {}", r(rdp), cb_imm9(c)) }
        }
        _ => Disasm::unknown(0xff),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Quadrant 2
// ─────────────────────────────────────────────────────────────────────────
fn decode_q2(c: u16, funct3: u16) -> Disasm {
    let rd_rs1 = ((c >> 7) & 0x1f) as u8;
    let rs2    = ((c >> 2) & 0x1f) as u8;
    let bit12  = (c >> 12) & 0x1;

    match funct3 {
        0b000 => { // C.SLLI
            let shamt = ((bit12 << 5) | ((c >> 2) & 0x1f)) as u32;
            Disasm { class: "C", mnemonic: "C.SLLI".into(),
                operands: format!("{}, {}", r(rd_rs1), shamt) }
        }
        0b001 => { // C.FLDSP (RV32DC)
            // imm[5|4:3|8:6]
            let v = (bit12 << 5) | (((c >> 5) & 0x3) << 3) | (((c >> 2) & 0x7) << 6);
            Disasm { class: "D", mnemonic: "C.FLDSP".into(),
                operands: format!("{}, {}(x2)", fr(rd_rs1), v) }
        }
        0b010 => { // C.LWSP
            // imm[5|4:2|7:6]
            let v = (bit12 << 5) | (((c >> 4) & 0x7) << 2) | (((c >> 2) & 0x3) << 6);
            Disasm { class: "C", mnemonic: "C.LWSP".into(),
                operands: format!("{}, {}(x2)", r(rd_rs1), v) }
        }
        0b011 => { // C.FLWSP (RV32FC)
            let v = (bit12 << 5) | (((c >> 4) & 0x7) << 2) | (((c >> 2) & 0x3) << 6);
            Disasm { class: "F", mnemonic: "C.FLWSP".into(),
                operands: format!("{}, {}(x2)", fr(rd_rs1), v) }
        }
        0b100 => { // C.JR / C.MV / C.EBREAK / C.JALR / C.ADD
            match (bit12, rd_rs1, rs2) {
                (0, 0, 0)              => Disasm { class: "C", mnemonic: "C.RESERVED".into(), operands: String::new() },
                (0, _, 0)              => Disasm { class: "C", mnemonic: "C.JR".into(),
                    operands: format!("{}", r(rd_rs1)) },
                (0, _, _)              => Disasm { class: "C", mnemonic: "C.MV".into(),
                    operands: format!("{}, {}", r(rd_rs1), r(rs2)) },
                (1, 0, 0)              => Disasm { class: "C", mnemonic: "C.EBREAK".into(), operands: String::new() },
                (1, _, 0)              => Disasm { class: "C", mnemonic: "C.JALR".into(),
                    operands: format!("{}", r(rd_rs1)) },
                (1, _, _)              => Disasm { class: "C", mnemonic: "C.ADD".into(),
                    operands: format!("{}, {}", r(rd_rs1), r(rs2)) },
                _ => unreachable!(),
            }
        }
        0b101 => { // C.FSDSP
            let v = (((c >> 10) & 0x7) << 3) | (((c >> 7) & 0x7) << 6);
            Disasm { class: "D", mnemonic: "C.FSDSP".into(),
                operands: format!("{}, {}(x2)", fr(rs2), v) }
        }
        0b110 => { // C.SWSP
            // imm[5:2|7:6]
            let v = (((c >> 9) & 0xf) << 2) | (((c >> 7) & 0x3) << 6);
            Disasm { class: "C", mnemonic: "C.SWSP".into(),
                operands: format!("{}, {}(x2)", r(rs2), v) }
        }
        0b111 => { // C.FSWSP
            let v = (((c >> 9) & 0xf) << 2) | (((c >> 7) & 0x3) << 6);
            Disasm { class: "F", mnemonic: "C.FSWSP".into(),
                operands: format!("{}, {}(x2)", fr(rs2), v) }
        }
        _ => Disasm::unknown(0xff),
    }
}
