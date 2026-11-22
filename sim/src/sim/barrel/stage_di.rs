//! Stage 3 — Decode + register read.
//!
//! Uses the shared generic decoder (`sim::decode`) for standard
//! RV32I, CSR, and SYSTEM instructions. Adds barrel-specific
//! custom-0 decoding (FORK/JOIN/DONE/YIELD/TID/FORKRET).

use crate::isa::fields::{extract_fields, InstrFields};
use crate::isa::instr::{decode, Instr};
use crate::sim::decode::{self, RegRead};
use crate::sim::stage::system_trap;
use super::state::{BarrelState, ControlBits, CsrOp, CustomOp, DiEx, DiExData, IfDi, MemKind, TrapKind, WbSel};

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

/// A valid DI/EX for `if_di`'s instruction with payload `d` (thread and
/// PC + 4 filled in).
fn latch(if_di: &IfDi, instr: u32, d: DiExData) -> DiEx {
    DiEx::new(if_di.pc, instr, DiExData { thread: if_di.thread, pc_plus4: if_di.pc.wrapping_add(4), ..d })
}

fn illegal_trap(if_di: &IfDi, instr: u32) -> DiEx {
    latch(if_di, instr, DiExData { trap_kind: TrapKind::Illegal, ..DiExData::default() })
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
    latch(if_di, instr, DiExData {
        rs1_data, rs2_data, imm: 0,
        rs1_num: f.rs1, rs2_num: f.rs2, rd_num: f.rd,
        funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
        ctrl: ControlBits { reg_write, wb_sel: WbSel::Alu, custom_op, ..ControlBits::default() },
        ..DiExData::default()
    })
}

// ── SYSTEM: CSR + trap ──────────────────────────────────────────

fn decode_csr(state: &BarrelState, if_di: &IfDi, instr: u32, f: &InstrFields) -> DiEx {
    let t = &state.threads[if_di.thread as usize];
    let regs = ThreadRegs { regs: &t.regs };
    match decode::decode_csr_instr(&regs, instr) {
        Some(c) => latch(if_di, instr, DiExData {
            rs1_data: c.write_val, rs2_data: 0, imm: 0,
            rs1_num: f.rs1, rs2_num: 0, rd_num: f.rd,
            funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
            // The CSR's old value comes back in alu_result (WB selects ALU).
            ctrl: ControlBits { reg_write: true, wb_sel: WbSel::Alu, custom_op: CustomOp::None, ..ControlBits::default() },
            csr_op: CsrOp::from_u8(c.csr_op), csr_addr: c.csr_addr,
            csr_uimm: c.csr_uimm, csr_imm_form: c.csr_imm_form,
            ..DiExData::default()
        }),
        None => illegal_trap(if_di, instr),
    }
}

fn decode_system_trap(if_di: &IfDi, instr: u32, f: &InstrFields) -> DiEx {
    // Barrel has no S-mode: SRET and SFENCE.VMA are illegal.
    let trap_kind = match system_trap(&decode(instr)) { TrapKind::Sret => TrapKind::Illegal, k => k };
    latch(if_di, instr, DiExData {
        funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
        trap_kind,
        ..DiExData::default()
    })
}

// ── Standard RV32I ──────────────────────────────────────────────

fn decode_std(state: &BarrelState, if_di: &IfDi, instr: u32, f: &InstrFields) -> DiEx {
    let t = &state.threads[if_di.thread as usize];
    let regs = ThreadRegs { regs: &t.regs };
    match decode::decode_standard(&regs, instr, f) {
        Some(d) => latch(if_di, instr, DiExData {
            rs1_data: d.rs1_data, rs2_data: d.rs2_data, imm: d.imm,
            rs1_num: f.rs1, rs2_num: f.rs2, rd_num: f.rd,
            funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
            ctrl: ControlBits {
                reg_write: d.reg_write, alu_src: d.alu_src, alu_op: d.alu_op,
                mem: MemKind::from_flags(d.mem_read, d.mem_write), wb_sel: WbSel::from_code(d.wb_sel),
                branch: d.branch, jump: d.jump, custom_op: CustomOp::None,
            },
            ..DiExData::default()
        }),
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
        0b0001111 if matches!(decode(instr), Instr::Fence | Instr::FenceI) => latch(if_di, instr, DiExData {
            funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
            ..DiExData::default()
        }),
        _ => decode_std(state, if_di, instr, &f),
    }
}
