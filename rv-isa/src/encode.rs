//! Instruction encoders (RV32I, Zmmul, Zicsr, privileged).
//!
//! Format encoders take the fields high bit to low bit, as they sit in the
//! word; the named helpers take operands in assembly order
//! (`addi(rd, rs1, imm)` = `addi rd, rs1, imm`). Registers are `u32` 0..31,
//! immediates `i32` (masked to their field).

// ── Formats ─────────────────────────────────────────────────────────────────

/// R-type: funct7 | rs2 | rs1 | funct3 | rd | opcode
pub const fn r(f7: u32, rs2: u32, rs1: u32, f3: u32, rd: u32, op: u32) -> u32 {
    f7 << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op
}

/// I-type: imm[11:0] | rs1 | funct3 | rd | opcode
pub const fn i(imm: i32, rs1: u32, f3: u32, rd: u32, op: u32) -> u32 {
    ((imm as u32) & 0xfff) << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op
}

/// Shift-immediate (SLLI / SRLI / SRAI): funct7 | shamt | rs1 | funct3 | rd | opcode
pub const fn shift(f7: u32, shamt: u32, rs1: u32, f3: u32, rd: u32, op: u32) -> u32 {
    f7 << 25 | (shamt & 0x1f) << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op
}

/// S-type store (opcode 0100011): imm[11:5] | rs2 | rs1 | funct3 | imm[4:0]
pub const fn s(imm: i32, rs2: u32, rs1: u32, f3: u32) -> u32 {
    let m = imm as u32;
    ((m >> 5) & 0x7f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | (m & 0x1f) << 7 | 0x23
}

/// B-type branch (opcode 1100011) to `pc + off`: rs1, rs2 in assembly order.
pub const fn b(off: i32, rs1: u32, rs2: u32, f3: u32) -> u32 {
    let o = off as u32;
    ((o >> 12) & 1) << 31 | ((o >> 5) & 0x3f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12
        | ((o >> 1) & 0xf) << 8 | ((o >> 11) & 1) << 7 | 0x63
}

/// U-type: `imm20` = the upper 20 bits (as written in `lui rd, imm20`)
pub const fn u(imm20: u32, rd: u32, op: u32) -> u32 { (imm20 & 0xfffff) << 12 | rd << 7 | op }

/// J-type to `pc + off`
pub const fn j(off: i32, rd: u32, op: u32) -> u32 {
    let o = off as u32;
    ((o >> 20) & 1) << 31 | ((o >> 1) & 0x3ff) << 21 | ((o >> 11) & 1) << 20 | ((o >> 12) & 0xff) << 12 | rd << 7 | op
}

// ── RV32I ───────────────────────────────────────────────────────────────────

pub const fn lui(rd: u32, imm20: u32) -> u32 { u(imm20, rd, 0x37) }
pub const fn auipc(rd: u32, imm20: u32) -> u32 { u(imm20, rd, 0x17) }
pub const fn jal(rd: u32, off: i32) -> u32 { j(off, rd, 0x6f) }
pub const fn jalr(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 0, rd, 0x67) }

pub const fn beq(rs1: u32, rs2: u32, off: i32) -> u32 { b(off, rs1, rs2, 0) }
pub const fn bne(rs1: u32, rs2: u32, off: i32) -> u32 { b(off, rs1, rs2, 1) }
pub const fn blt(rs1: u32, rs2: u32, off: i32) -> u32 { b(off, rs1, rs2, 4) }
pub const fn bge(rs1: u32, rs2: u32, off: i32) -> u32 { b(off, rs1, rs2, 5) }
pub const fn bltu(rs1: u32, rs2: u32, off: i32) -> u32 { b(off, rs1, rs2, 6) }
pub const fn bgeu(rs1: u32, rs2: u32, off: i32) -> u32 { b(off, rs1, rs2, 7) }

pub const fn lb(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 0, rd, 0x03) }
pub const fn lh(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 1, rd, 0x03) }
pub const fn lw(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 2, rd, 0x03) }
pub const fn lbu(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 4, rd, 0x03) }
pub const fn lhu(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 5, rd, 0x03) }
pub const fn sb(rs2: u32, rs1: u32, off: i32) -> u32 { s(off, rs2, rs1, 0) }
pub const fn sh(rs2: u32, rs1: u32, off: i32) -> u32 { s(off, rs2, rs1, 1) }
pub const fn sw(rs2: u32, rs1: u32, off: i32) -> u32 { s(off, rs2, rs1, 2) }

pub const fn addi(rd: u32, rs1: u32, imm: i32) -> u32 { i(imm, rs1, 0, rd, 0x13) }
pub const fn slti(rd: u32, rs1: u32, imm: i32) -> u32 { i(imm, rs1, 2, rd, 0x13) }
pub const fn sltiu(rd: u32, rs1: u32, imm: i32) -> u32 { i(imm, rs1, 3, rd, 0x13) }
pub const fn xori(rd: u32, rs1: u32, imm: i32) -> u32 { i(imm, rs1, 4, rd, 0x13) }
pub const fn ori(rd: u32, rs1: u32, imm: i32) -> u32 { i(imm, rs1, 6, rd, 0x13) }
pub const fn andi(rd: u32, rs1: u32, imm: i32) -> u32 { i(imm, rs1, 7, rd, 0x13) }
pub const fn slli(rd: u32, rs1: u32, shamt: u32) -> u32 { shift(0, shamt, rs1, 1, rd, 0x13) }
pub const fn srli(rd: u32, rs1: u32, shamt: u32) -> u32 { shift(0, shamt, rs1, 5, rd, 0x13) }
pub const fn srai(rd: u32, rs1: u32, shamt: u32) -> u32 { shift(0x20, shamt, rs1, 5, rd, 0x13) }

pub const fn add(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0, rs2, rs1, 0, rd, 0x33) }
pub const fn sub(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0x20, rs2, rs1, 0, rd, 0x33) }
pub const fn sll(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0, rs2, rs1, 1, rd, 0x33) }
pub const fn slt(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0, rs2, rs1, 2, rd, 0x33) }
pub const fn sltu(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0, rs2, rs1, 3, rd, 0x33) }
pub const fn xor(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0, rs2, rs1, 4, rd, 0x33) }
pub const fn srl(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0, rs2, rs1, 5, rd, 0x33) }
pub const fn sra(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0x20, rs2, rs1, 5, rd, 0x33) }
pub const fn or(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0, rs2, rs1, 6, rd, 0x33) }
pub const fn and(rd: u32, rs1: u32, rs2: u32) -> u32 { r(0, rs2, rs1, 7, rd, 0x33) }

// ── Zmmul ───────────────────────────────────────────────────────────────────

pub const fn mul(rd: u32, rs1: u32, rs2: u32) -> u32 { r(1, rs2, rs1, 0, rd, 0x33) }
pub const fn mulh(rd: u32, rs1: u32, rs2: u32) -> u32 { r(1, rs2, rs1, 1, rd, 0x33) }
pub const fn mulhsu(rd: u32, rs1: u32, rs2: u32) -> u32 { r(1, rs2, rs1, 2, rd, 0x33) }
pub const fn mulhu(rd: u32, rs1: u32, rs2: u32) -> u32 { r(1, rs2, rs1, 3, rd, 0x33) }

// ── Zicsr ───────────────────────────────────────────────────────────────────

pub const fn csrrw(rd: u32, csr: u32, rs1: u32) -> u32 { i(csr as i32, rs1, 1, rd, 0x73) }
pub const fn csrrs(rd: u32, csr: u32, rs1: u32) -> u32 { i(csr as i32, rs1, 2, rd, 0x73) }
pub const fn csrrc(rd: u32, csr: u32, rs1: u32) -> u32 { i(csr as i32, rs1, 3, rd, 0x73) }
pub const fn csrrwi(rd: u32, csr: u32, zimm: u32) -> u32 { i(csr as i32, zimm, 5, rd, 0x73) }
pub const fn csrrsi(rd: u32, csr: u32, zimm: u32) -> u32 { i(csr as i32, zimm, 6, rd, 0x73) }
pub const fn csrrci(rd: u32, csr: u32, zimm: u32) -> u32 { i(csr as i32, zimm, 7, rd, 0x73) }
/// csrr rd, csr  (= csrrs rd, csr, x0)
pub const fn csrr(rd: u32, csr: u32) -> u32 { csrrs(rd, csr, 0) }
/// csrw csr, rs1  (= csrrw x0, csr, rs1)
pub const fn csrw(csr: u32, rs1: u32) -> u32 { csrrw(0, csr, rs1) }

// ── Privileged / fences ─────────────────────────────────────────────────────

/// sfence.vma rs1, rs2 (x0 = all addresses / all ASIDs)
pub const fn sfence(rs1: u32, rs2: u32) -> u32 { r(0x09, rs2, rs1, 0, 0, 0x73) }

pub const NOP: u32 = 0x0000_0013;
/// `j .` — jump to itself
pub const SPIN: u32 = 0x0000_006f;
pub const ECALL: u32 = 0x0000_0073;
pub const EBREAK: u32 = 0x0010_0073;
pub const MRET: u32 = 0x3020_0073;
pub const SRET: u32 = 0x1020_0073;
pub const WFI: u32 = 0x1050_0073;
pub const FENCE_I: u32 = 0x0000_100f;
/// sfence.vma x0, x0
pub const SFENCE_VMA: u32 = 0x1200_0073;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_words() {
        assert_eq!(addi(5, 0, 128), 0x0800_0293);
        assert_eq!(add(5, 3, 4), 0x0041_82b3);
        assert_eq!(sub(5, 3, 4), 0x4041_82b3);
        assert_eq!(lui(5, 0x12345), 0x1234_52b7);
        assert_eq!(auipc(5, 0x12345), 0x1234_5297);
        assert_eq!(sw(6, 5, 0), 0x0062_a023);
        assert_eq!(lw(7, 5, 0), 0x0002_a383);
        assert_eq!(bne(10, 0, -4), 0xfe05_1ee3);
        assert_eq!(jal(0, 0), SPIN);
        assert_eq!(csrrw(0, 0x305, 5), 0x3052_9073);
        assert_eq!(mul(6, 5, 5), 0x0252_8333);
        assert_eq!(srai(5, 1, 3), 0x4030_d293);
        assert_eq!(sfence(0, 0), SFENCE_VMA);
    }

    #[test]
    fn round_trip_immediates() {
        use crate::imm::{imm_b, imm_i, imm_j, imm_s};
        for off in [-4096, -2048, -4, 0, 2, 4094] { assert_eq!(imm_b(beq(1, 2, off)), off); }
        for off in [-1_048_576, -4, 0, 2, 1_048_574] { assert_eq!(imm_j(jal(1, off)), off); }
        for imm in [-2048, -1, 0, 2047] {
            assert_eq!(imm_i(addi(1, 2, imm)), imm);
            assert_eq!(imm_s(sw(1, 2, imm)), imm);
        }
    }
}
