//! Shared instruction decode logic for both the 5-stage pipeline
//! and the 8-thread barrel processor.
//!
//! Generic over register-file access via the `RegRead` trait so
//! the same decode functions work with the pipeline's single
//! regfile and the barrel's per-thread regfiles.

use crate::data::control_data::opcode_by_value;
use crate::data::imm_data::{extract_immediate, ImmSel};

// ── Instruction field extraction ────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct InstrFields {
    pub opcode:   u8,
    pub rd:       u8,
    pub funct3:   u8,
    pub rs1:      u8,
    pub rs2:      u8,
    pub funct7:   u8,
    pub funct7b30: u8,
}

pub fn extract_fields(instr: u32) -> InstrFields {
    InstrFields {
        opcode:    (instr & 0x7f) as u8,
        rd:        ((instr >>  7) & 0x1f) as u8,
        funct3:    ((instr >> 12) & 0x07) as u8,
        rs1:       ((instr >> 15) & 0x1f) as u8,
        rs2:       ((instr >> 20) & 0x1f) as u8,
        funct7:    ((instr >> 25) & 0x7f) as u8,
        funct7b30: ((instr >> 30) & 0x1) as u8,
    }
}

// ── Classification helpers ──────────────────────────────────────

pub fn is_branch(opcode: u8) -> bool { opcode == 0b1100011 }
pub fn is_jump(opcode: u8) -> bool { opcode == 0b1101111 || opcode == 0b1100111 }
pub fn is_jalr(opcode: u8) -> bool { opcode == 0b1100111 }

// ── SYSTEM trap kind ────────────────────────────────────────────

/// Map SYSTEM funct3==0 funct12 → trap kind.
/// 0=none, 1=illegal, 2=ecall, 3=ebreak, 4=mret, 5=sret.
pub fn system_trap_kind(funct12: u32) -> u8 {
    match funct12 {
        0x000 => 2, // ECALL
        0x001 => 3, // EBREAK
        0x302 => 4, // MRET
        0x102 => 5, // SRET
        0x105 => 0, // WFI (treated as NOP)
        _     => 1, // illegal
    }
}

// ── CSR decode ──────────────────────────────────────────────────

/// Decode CSR op from SYSTEM funct3.
/// Returns (csr_op, is_imm_form). csr_op: 0=invalid, 1=RW, 2=RS, 3=RC.
pub fn decode_csr_op(funct3: u8) -> (u8, bool) {
    let op = match funct3 & 0b011 {
        0b01 => 1u8,
        0b10 => 2,
        0b11 => 3,
        _    => 0,
    };
    (op, (funct3 & 0b100) != 0)
}

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
/// opcodes (the caller should treat it as an illegal-instruction
/// trap). `f` must be from `extract_fields(instr)`.
pub fn decode_standard<R: RegRead>(
    regs: &R, instr: u32, f: &InstrFields,
) -> Option<StandardDecode> {
    let entry = opcode_by_value(f.opcode)?;
    let s = entry.signals;
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
pub fn decode_csr_instr<R: RegRead>(
    regs: &R, instr: u32, f: &InstrFields,
) -> Option<CsrDecode> {
    let (csr_op, imm_form) = decode_csr_op(f.funct3);
    if csr_op == 0 { return None; }
    let csr_addr = ((instr >> 20) & 0xfff) as u16;
    let write_val = if imm_form { f.rs1 as u32 } else { regs.read_reg(f.rs1) };
    Some(CsrDecode {
        csr_op,
        csr_addr,
        csr_uimm: f.rs1,
        csr_imm_form: imm_form,
        write_val,
    })
}
