//! Unit tests for alu_compute() across all 11 ALU ops.

use cpu_sim::data::alu_data::alu_compute;

#[test]
fn add_basic() {
    let r = alu_compute(0b0010, 5, 7);
    assert_eq!(r.result, 12);
    assert!(!r.flags.zero);
}

#[test]
fn sub_zero_flag() {
    let r = alu_compute(0b0110, 9, 9);
    assert_eq!(r.result, 0);
    assert!(r.flags.zero);
}

#[test]
fn sub_carry_borrow() {
    // 1 - 2 → borrow (carry=true)
    let r = alu_compute(0b0110, 1, 2);
    assert!(r.flags.carry);
}

#[test]
fn and_or_xor() {
    assert_eq!(alu_compute(0b0000, 0xff, 0x0f).result, 0x0f);
    assert_eq!(alu_compute(0b0001, 0xf0, 0x0f).result, 0xff);
    assert_eq!(alu_compute(0b0011, 0xff, 0x0f).result, 0xf0);
}

#[test]
fn shifts() {
    assert_eq!(alu_compute(0b0100, 1, 4).result, 16);                         // SLL
    assert_eq!(alu_compute(0b0101, 0x80, 1).result, 0x40);                    // SRL
    assert_eq!(alu_compute(0b1001, 0xffff_ff00, 4).result, 0xffff_fff0);      // SRA
}

#[test]
fn slt_sltu() {
    assert_eq!(alu_compute(0b0111, (-1i32) as u32, 0).result, 1); // signed -1 < 0
    assert_eq!(alu_compute(0b1000, (-1i32) as u32, 0).result, 0); // unsigned 0xffff_ffff > 0
}

#[test]
fn lui_passthrough() {
    assert_eq!(alu_compute(0b1010, 0xdead, 0x1234_5000).result, 0x1234_5000);
}

#[test]
fn signed_overflow_add() {
    let r = alu_compute(0b0010, 0x7fff_ffff, 1);
    assert!(r.flags.overflow);
    assert!(r.flags.negative);
}
