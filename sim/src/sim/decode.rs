//! Decode for the decoder-table cores (single-cycle, 5-stage pipeline,
//! barrel): which instruction it is comes from `isa::instr::decode`; the
//! control signals of an RV32I instruction from the decoder table
//! (`data::control_data`).
//!
//! Generic over register-file access via the `RegRead` trait so
//! the same decode functions work with the pipeline's single
//! regfile and the barrel's per-thread regfiles.

use crate::data::control_data::opcode_by_value;
use crate::isa::instr::{decode, Instr};
use crate::isa::fields::InstrFields;
use crate::isa::imm::{extract_immediate, ImmSel};

// ── Classification helpers ──────────────────────────────────────

pub fn is_branch(opcode: u8) -> bool { opcode == 0b1100011 }
pub fn is_jump(opcode: u8) -> bool { opcode == 0b1101111 || opcode == 0b1100111 }
pub fn is_jalr(opcode: u8) -> bool { opcode == 0b1100111 }
pub fn is_auipc(opcode: u8) -> bool { opcode == 0b0010111 }

// ── Register access trait ───────────────────────────────────────

/// Trait abstracting register-file reads. The pipeline implements
/// this with its single regfile; the barrel implements it with
/// per-thread regfiles indexed by the current thread ID.
pub trait RegRead {
    fn read_reg(&self, idx: u8) -> u32;
}

// ── Standard RV32I decode (generic over RegRead) ────────────────

/// Intermediate result of decoding a standard RV32I instruction.
/// Contains everything the architecture-specific decoder needs to
/// build its output struct (IdEx or DiEx).
#[derive(Debug, Clone, Copy)]
pub struct StandardDecode {
    pub rs1_data: u32,
    pub rs2_data: u32,
    pub imm: u32,
    pub reg_write: bool,
    pub alu_src: bool,
    pub alu_op: u8,
    pub mem_read: bool,
    pub mem_write: bool,
    pub wb_sel: u8,
    pub branch: bool,
    pub jump: bool,
}

/// Decode a standard RV32I instruction. Returns `None` for unknown
/// opcodes and reserved funct3 / funct7 encodings (the caller should treat it as an illegal-instruction
/// trap). `f` must be from `isa::fields::extract_fields(instr)`.
pub fn decode_standard<R: RegRead>(
    regs: &R, instr: u32, f: &InstrFields,
) -> Option<StandardDecode> {
    if !decode(instr).is_base_rv32i() { return None; }
    let s = opcode_by_value(f.opcode)?.signals;
    let imm = extract_immediate(instr, ImmSel::from_u8(s.imm_sel)) as u32;
    Some(StandardDecode {
        rs1_data: regs.read_reg(f.rs1),
        rs2_data: regs.read_reg(f.rs2),
        imm,
        reg_write: s.reg_write,
        alu_src:   s.alu_src,
        alu_op:    s.alu_op,
        mem_read:  s.mem_read,
        mem_write: s.mem_write,
        wb_sel:    s.wb_sel,
        branch: is_branch(f.opcode),
        jump:   is_jump(f.opcode),
    })
}

// ── CSR instruction decode (generic over RegRead) ───────────────

/// Intermediate result of decoding a CSR instruction.
#[derive(Debug, Clone, Copy)]
pub struct CsrDecode {
    pub csr_op: u8,       // 1=RW, 2=RS, 3=RC
    pub csr_addr: u16,
    pub csr_uimm: u8,
    pub csr_imm_form: bool,
    pub write_val: u32,   // rs1 value or zero-extended uimm
}

/// Decode a SYSTEM funct3!=0 CSR instruction. Returns `None` if
/// the funct3 doesn't map to a valid CSR op (= illegal).
pub fn decode_csr_instr<R: RegRead>(regs: &R, instr: u32) -> Option<CsrDecode> {
    let Instr::Csr { op, imm_form, src, csr, .. } = decode(instr) else { return None };
    let write_val = if imm_form { src as u32 } else { regs.read_reg(src) };
    Some(CsrDecode { csr_op: op.to_u8(), csr_addr: csr, csr_uimm: src, csr_imm_form: imm_form, write_val })
}
