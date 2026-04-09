//! Stage 3 — Decode + register read.
//!
//! Decodes the instruction word, reads the per-thread regfile (no
//! forwarding needed in barrel mode when ≥ 2 threads are runnable),
//! generates the immediate, and propagates control bits.

use crate::data::control_data::opcode_by_value;
use crate::data::imm_data::{extract_immediate, ImmSel};

use super::state::{BarrelState, ControlBits, CustomOp, DiEx, IfDi};

pub fn compute_di(state: &BarrelState, if_di: &IfDi) -> DiEx {
    if !if_di.valid {
        return DiEx::bubble();
    }
    let instr = if_di.instr;
    let opcode = (instr & 0x7f) as u8;
    let rd_num = ((instr >> 7) & 0x1f) as u8;
    let funct3 = ((instr >> 12) & 0x07) as u8;
    let rs1_num = ((instr >> 15) & 0x1f) as u8;
    let rs2_num = ((instr >> 20) & 0x1f) as u8;
    let funct7 = ((instr >> 25) & 0x7f) as u8;
    let funct7b30 = (funct7 >> 5) & 1;

    // ── Custom-0 (barrel-only opcode 0x0b) ─────────────────────
    // FORK/JOIN/DONE/YIELD/TID/FORKRET. R-type encoding,
    // distinguished by funct7. Decoded here, executed in stage_ma
    // because that stage already has &mut state for mutating
    // other thread slots.
    if opcode == 0b0001011 {
        let custom_op = CustomOp::from_funct7(funct7);
        if custom_op == CustomOp::None { return DiEx::bubble(); }
        let t = &state.threads[if_di.thread as usize];
        // FORKRET reads its source from the rd field (not rs1).
        let rs1_data = match custom_op {
            CustomOp::Forkret => t.regs[rd_num as usize],
            _                 => t.regs[rs1_num as usize],
        };
        let rs2_data = t.regs[rs2_num as usize];
        // FORK and TID write rd; the others don't.
        let reg_write = matches!(custom_op, CustomOp::Fork | CustomOp::Tid);
        return DiEx {
            valid: true,
            thread: if_di.thread,
            pc: if_di.pc,
            pc_plus4: if_di.pc.wrapping_add(4),
            instr,
            rs1_data,
            rs2_data,
            imm: 0,
            rs1_num,
            rs2_num,
            rd_num,
            funct3,
            funct7b30,
            opcode,
            ctrl: ControlBits {
                reg_write,
                alu_src: false,
                alu_op: 0,
                mem_read: false,
                mem_write: false,
                wb_sel: 0,    // result lives in alu_result, set by stage_ma
                branch: false,
                jump: false,
                custom_op,
            },
        };
    }

    let entry = match opcode_by_value(opcode) {
        Some(e) => e,
        None => return DiEx::bubble(), // unknown opcode → NOP
    };
    let s = entry.signals;

    let branch = opcode == 0b1100011;
    let jump = opcode == 0b1101111 || opcode == 0b1100111;

    let ctrl = ControlBits {
        reg_write: s.reg_write,
        alu_src: s.alu_src,
        alu_op: s.alu_op,
        mem_read: s.mem_read,
        mem_write: s.mem_write,
        wb_sel: s.wb_sel,
        branch,
        jump,
        custom_op: CustomOp::None,
    };

    let imm = extract_immediate(instr, ImmSel::from_u8(s.imm_sel)) as u32;
    let t = &state.threads[if_di.thread as usize];
    let rs1_data = t.regs[rs1_num as usize];
    let rs2_data = t.regs[rs2_num as usize];

    DiEx {
        valid: true,
        thread: if_di.thread,
        pc: if_di.pc,
        pc_plus4: if_di.pc.wrapping_add(4),
        instr,
        rs1_data,
        rs2_data,
        imm,
        rs1_num,
        rs2_num,
        rd_num,
        funct3,
        funct7b30,
        opcode,
        ctrl,
    }
}
