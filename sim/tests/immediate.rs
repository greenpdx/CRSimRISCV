//! Unit tests for extract_immediate() across all 5 RV32I immediate formats.

use cpu_sim::data::imm_data::{extract_immediate, ImmSel};

#[test]
fn i_type_positive() {
    // ADDI x0, x0, 5  →  imm[11:0] = 0x005
    let inst: u32 = 0x005_00013;
    assert_eq!(extract_immediate(inst, ImmSel::I), 5);
}

#[test]
fn i_type_negative() {
    // ADDI x0, x0, -1 →  imm = 0xfff
    let inst: u32 = 0xfff_00013;
    assert_eq!(extract_immediate(inst, ImmSel::I), -1);
}

#[test]
fn s_type() {
    // SW x7, 12(x6) — imm = 12
    let inst: u32 = (0u32 << 25) | (7u32 << 20) | (6u32 << 15)
        | (0b010 << 12) | (12u32 << 7) | 0b0100011;
    assert_eq!(extract_immediate(inst, ImmSel::S), 12);
}

#[test]
fn b_type() {
    // BEQ x5, x6, 16
    let imm = 16i32 as u32;
    let bit12  = (imm >> 12) & 0x1;
    let bit11  = (imm >> 11) & 0x1;
    let bit10_5 = (imm >> 5) & 0x3f;
    let bit4_1  = (imm >> 1) & 0xf;
    let inst: u32 = (bit12 << 31) | (bit10_5 << 25) | (6u32 << 20) | (5u32 << 15)
        | (0b000 << 12) | (bit4_1 << 8) | (bit11 << 7) | 0b1100011;
    assert_eq!(extract_immediate(inst, ImmSel::B), 16);
}

#[test]
fn u_type() {
    // LUI x5, 0x12345
    let inst: u32 = (0x12345 << 12) | (5 << 7) | 0b0110111;
    assert_eq!(extract_immediate(inst, ImmSel::U) as u32, 0x12345_000);
}

#[test]
fn j_type() {
    // JAL x1, 16
    let imm = 16u32;
    let bit20    = (imm >> 20) & 0x1;
    let bit19_12 = (imm >> 12) & 0xff;
    let bit11    = (imm >> 11) & 0x1;
    let bit10_1  = (imm >> 1)  & 0x3ff;
    let inst: u32 = (bit20 << 31) | (bit10_1 << 21) | (bit11 << 20)
        | (bit19_12 << 12) | (1u32 << 7) | 0b1101111;
    assert_eq!(extract_immediate(inst, ImmSel::J), 16);
}

#[test]
fn r_type_none() {
    assert_eq!(extract_immediate(0xdead_beef, ImmSel::None), 0);
}
