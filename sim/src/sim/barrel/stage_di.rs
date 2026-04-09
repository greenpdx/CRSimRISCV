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
        if custom_op == CustomOp::None {
            // Unknown custom-0 funct7 → illegal instruction trap.
            return DiEx { valid: true, thread: if_di.thread, pc: if_di.pc,
                pc_plus4: if_di.pc.wrapping_add(4), instr,
                trap_kind: 1,
                csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
                ..DiEx::default() };
        }
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
            trap_kind: 0,
            csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
        };
    }

    // ── SYSTEM opcode (0x73) ──────────────────────────────────
    // funct3 == 0: ECALL/EBREAK/MRET (trap/return)
    // funct3 != 0: CSR instructions (CSRRW/RS/RC + imm variants)
    if opcode == 0b1110011 && funct3 != 0 {
        // CSR instruction. funct3 low 2 bits = op (01=RW, 10=RS, 11=RC).
        // funct3 bit 2 = immediate form (uimm in rs1 field).
        let csr_op = match funct3 & 0b011 {
            0b01 => 1u8, // RW
            0b10 => 2,   // RS
            0b11 => 3,   // RC
            _    => 0,
        };
        if csr_op == 0 {
            return DiEx { valid: true, thread: if_di.thread, pc: if_di.pc,
                pc_plus4: if_di.pc.wrapping_add(4), instr,
                trap_kind: 1,
                csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
                ..DiEx::default() }; // illegal
        }
        let csr_imm_form = (funct3 & 0b100) != 0;
        let csr_addr = ((instr >> 20) & 0xfff) as u16;
        let csr_uimm = rs1_num; // rs1 field doubles as uimm[4:0]
        let t = &state.threads[if_di.thread as usize];
        let rs1_data = if csr_imm_form { csr_uimm as u32 } else { t.regs[rs1_num as usize] };
        return DiEx {
            valid: true,
            thread: if_di.thread,
            pc: if_di.pc,
            pc_plus4: if_di.pc.wrapping_add(4),
            instr,
            rs1_data,
            rs2_data: 0,
            imm: 0,
            rs1_num,
            rs2_num: 0,
            rd_num,
            funct3, funct7b30, opcode,
            ctrl: ControlBits {
                reg_write: true,  // CSR always writes rd (old value)
                wb_sel: 0,        // result in alu_result
                custom_op: CustomOp::None,
                ..ControlBits::default()
            },
            trap_kind: 0,
            csr_op,
            csr_addr,
            csr_uimm,
            csr_imm_form,
        };
    }
    if opcode == 0b1110011 && funct3 == 0 {
        let funct12 = (instr >> 20) & 0xfff;
        let trap_kind = match funct12 {
            0x000 => 2, // ECALL (cause depends on priv_mode)
            0x001 => 3, // EBREAK
            0x302 => 4, // MRET
            _     => 1, // unknown → illegal
        };
        return DiEx {
            valid: true, thread: if_di.thread, pc: if_di.pc,
            pc_plus4: if_di.pc.wrapping_add(4), instr,
            rs1_data: 0, rs2_data: 0, imm: 0,
            rs1_num: 0, rs2_num: 0, rd_num: 0,
            funct3, funct7b30, opcode,
            ctrl: ControlBits::default(),
            trap_kind,
            csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
        };
    }

    let entry = match opcode_by_value(opcode) {
        Some(e) => e,
        None => {
            // Unknown opcode → illegal instruction trap.
            return DiEx { valid: true, thread: if_di.thread, pc: if_di.pc,
                pc_plus4: if_di.pc.wrapping_add(4), instr,
                trap_kind: 1,
                csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
                ..DiEx::default() };
        }
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
        trap_kind: 0,
        csr_op: 0, csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
    }
}
