//! The instruction decoder: what a 32-bit word *is*. One decoder for every
//! core; each core maps an [`Instr`] to its own control (decoder table, FSM
//! class, issue unit) and decides which instructions it implements.
//!
//! Strict: reserved encodings (non-zero fields that must be zero, unused
//! funct3 / funct7) decode as [`Instr::Illegal`]. Immediates are sign-extended
//! and stored as `u32`.

use crate::alu::AluOp;
use crate::branch::Cond;
use crate::csr::{is_imm_form, CsrOp};
use crate::fields::extract_fields;
use crate::imm::{imm_b, imm_i, imm_j, imm_s, imm_u};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    Lui   { rd: u8, imm: u32 },
    Auipc { rd: u8, imm: u32 },
    Jal   { rd: u8, imm: u32 },
    Jalr  { rd: u8, rs1: u8, imm: u32 },
    Branch { cond: Cond, rs1: u8, rs2: u8, imm: u32 },
    /// `size` 1 / 2 / 4 bytes
    Load  { rd: u8, rs1: u8, imm: u32, size: u8, signed: bool },
    Store { rs1: u8, rs2: u8, imm: u32, size: u8 },
    /// OP-IMM; for the shifts `imm` is the whole I immediate (shamt = imm[4:0])
    OpImm { op: AluOp, rd: u8, rs1: u8, imm: u32 },
    Op    { op: AluOp, rd: u8, rs1: u8, rs2: u8 },
    /// Zmmul: `op` = funct3 0 MUL · 1 MULH · 2 MULHSU · 3 MULHU
    Mul   { op: u8, rd: u8, rs1: u8, rs2: u8 },
    /// custom-0 opcode space (0001011), decoded by the core that has it
    Custom0 { funct7: u8, funct3: u8, rd: u8, rs1: u8, rs2: u8 },
    Fence,
    FenceI,
    Ecall,
    Ebreak,
    Mret,
    Sret,
    Wfi,
    SfenceVma { rs1: u8, rs2: u8 },
    /// Zicsr; `src` = rs1 number, or zimm for the immediate forms
    Csr { op: CsrOp, imm_form: bool, rd: u8, src: u8, csr: u16 },
    Illegal,
}

impl Instr {
    /// RV32I instruction with a row in the decoder-table cores' control table.
    pub const fn is_base_rv32i(&self) -> bool {
        matches!(self, Instr::Lui { .. } | Instr::Auipc { .. } | Instr::Jal { .. } | Instr::Jalr { .. }
            | Instr::Branch { .. } | Instr::Load { .. } | Instr::Store { .. } | Instr::OpImm { .. } | Instr::Op { .. })
    }
}

/// Decode one 32-bit instruction word.
pub const fn decode(w: u32) -> Instr {
    let f = extract_fields(w);
    let (rd, rs1, rs2, f3, f7) = (f.rd, f.rs1, f.rs2, f.funct3, f.funct7);
    match f.opcode {
        0b0110111 => Instr::Lui { rd, imm: imm_u(w) as u32 },
        0b0010111 => Instr::Auipc { rd, imm: imm_u(w) as u32 },
        0b1101111 => Instr::Jal { rd, imm: imm_j(w) as u32 },
        0b1100111 if f3 == 0 => Instr::Jalr { rd, rs1, imm: imm_i(w) as u32 },
        0b1100011 => match Cond::from_funct3(f3) {
            Some(cond) => Instr::Branch { cond, rs1, rs2, imm: imm_b(w) as u32 },
            None => Instr::Illegal,
        },
        0b0000011 => {
            let (size, signed) = match f3 { 0 => (1, true), 1 => (2, true), 2 => (4, true), 4 => (1, false), 5 => (2, false), _ => (0, false) };
            if size == 0 { Instr::Illegal } else { Instr::Load { rd, rs1, imm: imm_i(w) as u32, size, signed } }
        }
        0b0100011 if f3 < 3 => Instr::Store { rs1, rs2, imm: imm_s(w) as u32, size: 1 << f3 },
        0b0010011 => {
            let imm = imm_i(w) as u32;
            match f3 {
                1 if f7 == 0 => Instr::OpImm { op: AluOp::Sll, rd, rs1, imm },
                5 if f7 == 0 => Instr::OpImm { op: AluOp::Srl, rd, rs1, imm },
                5 if f7 == 0b010_0000 => Instr::OpImm { op: AluOp::Sra, rd, rs1, imm },
                1 | 5 => Instr::Illegal,
                _ => Instr::OpImm { op: AluOp::from_funct3(f3, false), rd, rs1, imm },
            }
        }
        0b0110011 => match f7 {
            0b000_0000 => Instr::Op { op: AluOp::from_funct3(f3, false), rd, rs1, rs2 },
            0b010_0000 if f3 == 0 || f3 == 5 => Instr::Op { op: AluOp::from_funct3(f3, true), rd, rs1, rs2 },
            0b000_0001 if f3 < 4 => Instr::Mul { op: f3, rd, rs1, rs2 },
            _ => Instr::Illegal,
        },
        0b0001111 => match f3 { 0 => Instr::Fence, 1 => Instr::FenceI, _ => Instr::Illegal },
        0b0001011 => Instr::Custom0 { funct7: f7, funct3: f3, rd, rs1, rs2 },
        0b1110011 => match f3 {
            0 => match w {
                0x0000_0073 => Instr::Ecall,
                0x0010_0073 => Instr::Ebreak,
                0x3020_0073 => Instr::Mret,
                0x1020_0073 => Instr::Sret,
                0x1050_0073 => Instr::Wfi,
                _ if f7 == 0b000_1001 && rd == 0 => Instr::SfenceVma { rs1, rs2 },
                _ => Instr::Illegal,
            },
            4 => Instr::Illegal,
            _ => Instr::Csr { op: CsrOp::from_funct3(f3), imm_form: is_imm_form(f3), rd, src: rs1, csr: (w >> 20) as u16 },
        },
        _ => Instr::Illegal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::*;

    #[test]
    fn decodes() {
        assert_eq!(decode(addi(5, 6, -1)), Instr::OpImm { op: AluOp::Add, rd: 5, rs1: 6, imm: 0xffff_ffff });
        assert_eq!(decode(sub(5, 6, 7)), Instr::Op { op: AluOp::Sub, rd: 5, rs1: 6, rs2: 7 });
        assert_eq!(decode(srai(5, 6, 3)), Instr::OpImm { op: AluOp::Sra, rd: 5, rs1: 6, imm: 0x403 });
        assert_eq!(decode(lhu(1, 2, -2)), Instr::Load { rd: 1, rs1: 2, imm: 0xffff_fffe, size: 2, signed: false });
        assert_eq!(decode(sb(1, 2, 3)), Instr::Store { rs1: 2, rs2: 1, imm: 3, size: 1 });
        assert_eq!(decode(bgeu(1, 2, -8)), Instr::Branch { cond: Cond::Geu, rs1: 1, rs2: 2, imm: 0xffff_fff8 });
        assert_eq!(decode(mulhu(1, 2, 3)), Instr::Mul { op: 3, rd: 1, rs1: 2, rs2: 3 });
        assert_eq!(decode(csrrsi(1, 0x340, 5)), Instr::Csr { op: CsrOp::Rs, imm_form: true, rd: 1, src: 5, csr: 0x340 });
        assert_eq!(decode(sfence(1, 2)), Instr::SfenceVma { rs1: 1, rs2: 2 });
        for (w, i) in [(ECALL, Instr::Ecall), (EBREAK, Instr::Ebreak), (MRET, Instr::Mret), (SRET, Instr::Sret), (WFI, Instr::Wfi), (FENCE_I, Instr::FenceI)] {
            assert_eq!(decode(w), i);
        }
    }

    #[test]
    fn reserved_encodings_are_illegal() {
        for w in [
            0xffff_ffff,
            r(1, 2, 1, 4, 3, 0x33),        // DIV (not Zmmul)
            r(0x20, 2, 1, 1, 3, 0x33),     // "SUB" form of SLL
            shift(0x20, 1, 1, 1, 3, 0x13), // SLLI with funct7 0100000
            i(0, 1, 3, 1, 0x03),           // LD (RV64)
            s(0, 1, 2, 3),                 // SD (RV64)
            b(8, 1, 2, 2),                 // branch funct3 010
            i(0, 1, 1, 1, 0x67),           // JALR funct3 001
            i(0, 0, 2, 0, 0x0f),           // MISC-MEM funct3 010
            MRET | 1 << 7,                 // MRET with rd ≠ 0
            ECALL | 1 << 15,               // ECALL with rs1 ≠ 0
            sfence(1, 2) | 1 << 7,         // SFENCE.VMA with rd ≠ 0
            i(0x340, 1, 4, 1, 0x73),       // SYSTEM funct3 100
        ] {
            assert_eq!(decode(w), Instr::Illegal, "{w:#010x}");
        }
    }
}
