//! Stage 3 — Decode + register read.
//!
//! Uses the shared generic decoder (`sim::decode`) for standard
//! RV32I, CSR, and SYSTEM instructions. Adds barrel-specific
//! custom-0 decoding (FORK/JOIN/DONE/YIELD/TID/FORKRET).

use crate::isa::fields::{extract_fields, InstrFields};
use crate::isa::instr::{decode, Instr};
use crate::sim::decode::{
    self, RegRead, system_trap_kind,
};
use super::state::{BarrelState, ControlBits, CustomOp, DiEx, IfDi};

// ── RegRead impl for barrel per-thread regfile ──────────────────

struct ThreadRegs<'a> {
    regs: &'a [u32; 32],
}
impl<'a> RegRead for ThreadRegs<'a> {
    fn read_reg(&self, idx: u8) -> u32 {
        self.regs[(idx as usize) & 0x1f]
    }
}

// ── Helpers ─────────────────────────────────────────────────────

fn illegal_trap(if_di: &IfDi, instr: u32) -> DiEx {
    DiEx {
        valid: true, thread: if_di.thread, pc: if_di.pc,
        pc_plus4: if_di.pc.wrapping_add(4), instr,
        trap_kind: 1,
        ..DiEx::default()
    }
}

// ── Custom-0 (barrel-only opcode 0x0b) ──────────────────────────

fn decode_custom0(state: &BarrelState, if_di: &IfDi, instr: u32, f: &InstrFields) -> DiEx {
    let custom_op = CustomOp::from_funct7(f.funct7);
    if custom_op == CustomOp::None {
        return illegal_trap(if_di, instr);
    }
    let t = &state.threads[if_di.thread as usize];
    let rs1_data = match custom_op {
        CustomOp::Forkret => t.regs[f.rd as usize],
        _                 => t.regs[f.rs1 as usize],
    };
    let rs2_data = t.regs[f.rs2 as usize];
    let reg_write = matches!(custom_op, CustomOp::Fork | CustomOp::Tid);
    DiEx {
        valid: true, thread: if_di.thread,
        pc: if_di.pc, pc_plus4: if_di.pc.wrapping_add(4), instr,
        rs1_data, rs2_data, imm: 0,
        rs1_num: f.rs1, rs2_num: f.rs2, rd_num: f.rd,
        funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
        ctrl: ControlBits {
            reg_write, wb_sel: 0, custom_op,
            ..ControlBits::default()
        },
        trap_kind: 0,
        csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
    }
}

// ── SYSTEM: CSR + trap ──────────────────────────────────────────

fn decode_csr(state: &BarrelState, if_di: &IfDi, instr: u32, f: &InstrFields) -> DiEx {
    let t = &state.threads[if_di.thread as usize];
    let regs = ThreadRegs { regs: &t.regs };
    match decode::decode_csr_instr(&regs, instr) {
        Some(c) => DiEx {
            valid: true, thread: if_di.thread,
            pc: if_di.pc, pc_plus4: if_di.pc.wrapping_add(4), instr,
            rs1_data: c.write_val, rs2_data: 0, imm: 0,
            rs1_num: f.rs1, rs2_num: 0, rd_num: f.rd,
            funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
            ctrl: ControlBits { reg_write: true, wb_sel: 0, custom_op: CustomOp::None, ..ControlBits::default() },
            trap_kind: 0,
            csr_op: c.csr_op, csr_addr: c.csr_addr,
            csr_uimm: c.csr_uimm, csr_imm_form: c.csr_imm_form,
        },
        None => illegal_trap(if_di, instr),
    }
}

fn decode_system_trap(if_di: &IfDi, instr: u32, f: &InstrFields) -> DiEx {
    // Barrel has no S-mode: SRET (5) and SFENCE.VMA are illegal.
    let trap_kind = match system_trap_kind(&decode(instr)) { 5 => 1, k => k };
    DiEx {
        valid: true, thread: if_di.thread,
        pc: if_di.pc, pc_plus4: if_di.pc.wrapping_add(4), instr,
        rs1_data: 0, rs2_data: 0, imm: 0,
        rs1_num: 0, rs2_num: 0, rd_num: 0,
        funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
        ctrl: ControlBits::default(),
        trap_kind,
        csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
    }
}

// ── Standard RV32I ──────────────────────────────────────────────

fn decode_std(state: &BarrelState, if_di: &IfDi, instr: u32, f: &InstrFields) -> DiEx {
    let t = &state.threads[if_di.thread as usize];
    let regs = ThreadRegs { regs: &t.regs };
    match decode::decode_standard(&regs, instr, f) {
        Some(d) => DiEx {
            valid: true, thread: if_di.thread,
            pc: if_di.pc, pc_plus4: if_di.pc.wrapping_add(4), instr,
            rs1_data: d.rs1_data, rs2_data: d.rs2_data, imm: d.imm,
            rs1_num: f.rs1, rs2_num: f.rs2, rd_num: f.rd,
            funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
            ctrl: ControlBits {
                reg_write: d.reg_write, alu_src: d.alu_src, alu_op: d.alu_op,
                mem_read: d.mem_read, mem_write: d.mem_write, wb_sel: d.wb_sel,
                branch: d.branch, jump: d.jump, custom_op: CustomOp::None,
            },
            trap_kind: 0,
            csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
        },
        None => illegal_trap(if_di, instr),
    }
}

// ── Main entry point ────────────────────────────────────────────

pub fn compute_di(state: &BarrelState, if_di: &IfDi) -> DiEx {
    if !if_di.valid { return DiEx::bubble(); }
    let instr = if_di.instr;
    let f = extract_fields(instr);
    match f.opcode {
        0b0001011 => decode_custom0(state, if_di, instr, &f),
        0b1110011 if f.funct3 != 0 => decode_csr(state, if_di, instr, &f),
        0b1110011 => decode_system_trap(if_di, instr, &f),
        // FENCE / FENCE.I: no-op (memory is in order; stores write through)
        0b0001111 if matches!(decode(instr), Instr::Fence | Instr::FenceI) => DiEx {
            valid: true, thread: if_di.thread, pc: if_di.pc, pc_plus4: if_di.pc.wrapping_add(4), instr,
            funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
            ..DiEx::default()
        },
        _ => decode_std(state, if_di, instr, &f),
    }
}
