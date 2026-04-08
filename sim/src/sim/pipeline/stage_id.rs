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

use super::state::{ControlBits, IdEx, IfId, PipelineState};

/// Decode an instruction word and produce the next ID/EX register
/// contents (assuming no stall/flush). Returns `None` for an unknown
/// opcode (treated as a NOP bubble by the caller).
pub fn compute_id(state: &PipelineState, if_id: &IfId) -> Option<IdEx> {
    if !if_id.valid { return Some(IdEx::bubble()); }

    let instr = if_id.instr;
    let opcode  = (instr & 0x7f) as u8;
    let rd_num  = ((instr >>  7) & 0x1f) as u8;
    let funct3  = ((instr >> 12) & 0x07) as u8;
    let rs1_num = ((instr >> 15) & 0x1f) as u8;
    let rs2_num = ((instr >> 20) & 0x1f) as u8;
    let funct7b30 = ((instr >> 30) & 0x1) as u8;

    let entry = opcode_by_value(opcode)?;
    let s = entry.signals;

    // Detect "branch" and "jump" categories — both consume `pc_sel != 0`
    // but have different semantics in EX.
    let branch = opcode == 0b1100011;
    let jump   = opcode == 0b1101111 || opcode == 0b1100111; // JAL or JALR

    let ctrl = ControlBits {
        reg_write: s.reg_write,
        alu_src:   s.alu_src,
        alu_op:    s.alu_op,
        mem_read:  s.mem_read,
        mem_write: s.mem_write,
        wb_sel:    s.wb_sel,
        pc_sel:    s.pc_sel,
        branch,
        jump,
    };

    let imm = extract_immediate(instr, ImmSel::from_u8(s.imm_sel)) as u32;
    let rs1_data = state.read_register(rs1_num);
    let rs2_data = state.read_register(rs2_num);

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
    if !id_ex.valid || !id_ex.ctrl.mem_read || id_ex.rd_num == 0 {
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
