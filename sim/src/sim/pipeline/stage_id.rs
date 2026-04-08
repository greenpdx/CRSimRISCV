//! Pipeline stage 2 — Instruction Decode + Register Read (ID).
//!
//! Decodes the instruction word in IF/ID, reads source operands from
//! the regfile, generates the immediate, and produces the new ID/EX
//! register contents.
//!
//! Also runs the **load-use hazard detection**: if the instruction
//! currently in ID/EX is a load whose destination matches one of the
//! source registers being read here, the caller must stall (freeze
//! PC + IF/ID, insert a bubble in ID/EX).
//!
//! Always-active components: `decoder`, `imm_gen`, `regfile` (read
//! ports), `hazard_unit`, `id_ex_reg`.

use crate::data::control_data::opcode_by_value;
use crate::data::imm_data::{extract_immediate, ImmSel};

use super::csr::{csr_op_from_funct3, csr_is_imm_form, CsrOp};
use super::state::{ControlBits, IdEx, IfId, PipelineState};

/// Decode an instruction word and produce the next ID/EX register
/// contents (assuming no stall/flush). Returns `None` for an unknown
/// opcode (treated as a NOP bubble by the caller).
pub fn compute_id(state: &PipelineState, if_id: &IfId) -> Option<IdEx> {
    if !if_id.valid {
        let mut b = IdEx::bubble();
        b.flush_bubble = if_id.flush_bubble;
        return Some(b);
    }

    // IF stage already detected an instr page fault — propagate the
    // trap through ID/EX so the existing trap path delivers it.
    if if_id.trap_kind != 0 {
        return Some(IdEx {
            valid: true,
            pc: if_id.pc,
            pc_plus4: if_id.pc_plus4,
            instr: if_id.instr,
            rs1_data: 0, rs2_data: 0, imm: 0,
            rs1_num: 0, rs2_num: 0, rd_num: 0,
            funct3: 0, funct7b30: 0, opcode: 0,
            ctrl: ControlBits::default(),
            csr_addr: 0, csr_uimm: 0, csr_imm_form: false, stall_reg: 0, flush_bubble: false,
            early_target: 0, early_branch: false,
            trap_kind: if_id.trap_kind,
        });
    }

    let instr = if_id.instr;
    let opcode  = (instr & 0x7f) as u8;
    let rd_num  = ((instr >>  7) & 0x1f) as u8;
    let funct3  = ((instr >> 12) & 0x07) as u8;
    let rs1_num = ((instr >> 15) & 0x1f) as u8;
    let rs2_num = ((instr >> 20) & 0x1f) as u8;
    let funct7b30 = ((instr >> 30) & 0x1) as u8;

    // Unknown opcode → illegal-instruction trap. Carry the instruction
    // word forward as a "bubble that traps in EX" so the pipeline can
    // deliver the trap and update CSRs.
    let entry = match opcode_by_value(opcode) {
        Some(e) => e,
        None => {
            return Some(IdEx {
                valid: true,
                pc: if_id.pc,
                pc_plus4: if_id.pc_plus4,
                instr,
                rs1_data: 0, rs2_data: 0, imm: 0,
                rs1_num: 0, rs2_num: 0, rd_num: 0,
                funct3: 0, funct7b30: 0, opcode,
                ctrl: ControlBits::default(),
                csr_addr: 0, csr_uimm: 0, csr_imm_form: false, stall_reg: 0, flush_bubble: false,
            early_target: 0, early_branch: false,
                trap_kind: 1, // illegal
            });
        }
    };
    let s = entry.signals;

    // Detect "branch" and "jump" categories — both consume `pc_sel != 0`
    // but have different semantics in EX.
    let branch = opcode == 0b1100011;
    let jump   = opcode == 0b1101111 || opcode == 0b1100111; // JAL or JALR

    // Zicsr decode (SYSTEM opcode 0x73). funct3==0 is ECALL/EBREAK
    // (treated as NOP bubble in v1). funct3 1..7 are CSR ops; bit2
    // distinguishes the immediate-form variants.
    let mut csr_op = CsrOp::None;
    let mut csr_imm_form = false;
    let mut csr_addr: u16 = 0;
    if opcode == 0b1110011 && funct3 != 0 {
        csr_op = csr_op_from_funct3(funct3);
        csr_imm_form = csr_is_imm_form(funct3);
        csr_addr = ((instr >> 20) & 0xfff) as u16;
    }
    let csr_uimm = rs1_num; // rs1 field as zero-extended uimm[4:0]
    let is_csr = csr_op != CsrOp::None;

    // SYSTEM funct3 == 0 ⇒ ECALL / EBREAK / MRET (distinguished by funct12).
    let mut trap_kind: u8 = 0;
    let mut is_sfence = false;
    if opcode == 0b1110011 && funct3 == 0 {
        let funct12 = (instr >> 20) & 0xfff;
        let funct7  = (instr >> 25) & 0x7f;
        trap_kind = if funct7 == 0x09 {
            // SFENCE.VMA — flush TLB when this instruction reaches MA.
            is_sfence = true;
            0
        } else {
            match funct12 {
                0x000 => 2, // ECALL
                0x001 => 3, // EBREAK
                0x302 => 4, // MRET
                0x102 => 5, // SRET
                0x105 => 0, // WFI — treat as NOP
                _     => 1, // illegal
            }
        };
    }

    let ctrl = ControlBits {
        // For CSR ops, force reg_write on (rd may be x0; WB stage already
        // suppresses writes to x0). Override wb_sel to 3.
        reg_write: if is_csr { true } else { s.reg_write },
        alu_src:   s.alu_src,
        alu_op:    s.alu_op,
        mem_read:  s.mem_read,
        mem_write: s.mem_write,
        wb_sel:    if is_csr { 3 } else { s.wb_sel },
        pc_sel:    s.pc_sel,
        branch,
        jump,
        csr_op:    csr_op.to_u8(),
        is_sfence,
    };

    let imm = extract_immediate(instr, ImmSel::from_u8(s.imm_sel)) as u32;
    let rs1_data = state.read_register(rs1_num);
    let rs2_data = state.read_register(rs2_num);

    // ID-side branch target adder. For BEQ/BNE/JAL the target is
    // PC + imm. For JALR it's (rs1 + imm) & ~1, which we cannot
    // compute in ID without forwarding for rs1, so JALR's early_target
    // is left at 0 and the EX stage handles it as before.
    let is_jalr = opcode == 0b1100111;
    let early_branch = branch || (jump && !is_jalr);
    let early_target = if early_branch {
        if_id.pc.wrapping_add(imm)
    } else { 0 };

    Some(IdEx {
        valid: true,
        pc: if_id.pc,
        pc_plus4: if_id.pc_plus4,
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
        csr_addr,
        csr_uimm,
        csr_imm_form,
        trap_kind,
        stall_reg: 0,
        flush_bubble: false,
        early_target,
        early_branch,
    })
}

/// Detect a load-use hazard.
///
/// A load-use hazard exists when the instruction currently in ID/EX is
/// a load (`mem_read = true`), and its destination register matches
/// one of the source registers used by the instruction currently being
/// decoded in ID (i.e., the instruction in IF/ID).
///
/// When this returns true, the caller must:
///   1. Freeze the PC (do not increment).
///   2. Freeze the IF/ID register (the same instruction stays in IF/ID).
///   3. Insert a bubble into ID/EX.
pub fn detect_load_use_hazard(if_id: &IfId, id_ex: &IdEx) -> bool {
    // CSR ops also produce their rd value in MA (same slot as loads),
    // so treat them like a load for hazard detection.
    let produces_in_ma = id_ex.ctrl.mem_read || id_ex.ctrl.csr_op != 0;
    if !id_ex.valid || !produces_in_ma || id_ex.rd_num == 0 {
        return false;
    }
    if !if_id.valid {
        return false;
    }
    let instr = if_id.instr;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;
    id_ex.rd_num == rs1 || id_ex.rd_num == rs2
}
