//! Pipeline stage 2 — Instruction Decode + Register Read (ID).
//!
//! Uses the shared generic decoder (`sim::decode`) for standard
//! RV32I, CSR, and SYSTEM instructions. Adds pipeline-specific
//! SFENCE.VMA detection, early branch target computation, and
//! flush/stall-bubble propagation.
//!
//! Also runs the **load-use hazard detection**.

use crate::sim::decode::{
    self, InstrFields, RegRead, extract_fields, is_branch, is_jump,
    is_jalr, system_trap_kind,
};
use super::csr::{csr_op_from_funct3, csr_is_imm_form, CsrOp};
use super::state::{ControlBits, IdEx, IfId, PipelineState};

// ── RegRead impl for pipeline's single regfile ──────────────────

struct PipelineRegs<'a> {
    state: &'a PipelineState,
}
impl<'a> RegRead for PipelineRegs<'a> {
    fn read_reg(&self, idx: u8) -> u32 {
        self.state.read_register(idx)
    }
}

/// Decode an instruction word and produce the next ID/EX register
/// contents (assuming no stall/flush). Returns `None` for an unknown
/// opcode (treated as a NOP bubble by the caller).
pub fn compute_id(state: &PipelineState, if_id: &IfId) -> Option<IdEx> {
    if !if_id.valid {
        let mut b = IdEx::bubble();
        b.flush_bubble = if_id.flush_bubble;
        return Some(b);
    }

    // IF stage already detected an instr page fault — propagate.
    if if_id.trap_kind != 0 {
        return Some(IdEx {
            valid: true, pc: if_id.pc, pc_plus4: if_id.pc_plus4,
            instr: if_id.instr,
            trap_kind: if_id.trap_kind,
            ..IdEx::default()
        });
    }

    let instr = if_id.instr;
    let f = extract_fields(instr);
    let regs = PipelineRegs { state };

    // ── Standard RV32I decode (via shared generic) ──────────
    // Try the shared decoder first; if it returns None the opcode
    // is unknown → illegal trap.
    let is_system = f.opcode == 0b1110011;

    if !is_system {
        match decode::decode_standard(&regs, instr, &f) {
            Some(d) => {
                let early_branch = d.branch || (d.jump && !is_jalr(f.opcode));
                let early_target = if early_branch {
                    if_id.pc.wrapping_add(d.imm)
                } else { 0 };

                return Some(IdEx {
                    valid: true, pc: if_id.pc, pc_plus4: if_id.pc_plus4, instr,
                    rs1_data: d.rs1_data, rs2_data: d.rs2_data, imm: d.imm,
                    rs1_num: f.rs1, rs2_num: f.rs2, rd_num: f.rd,
                    funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
                    ctrl: ControlBits {
                        reg_write: d.reg_write, alu_src: d.alu_src, alu_op: d.alu_op,
                        mem_read: d.mem_read, mem_write: d.mem_write, wb_sel: d.wb_sel,
                        pc_sel: crate::data::control_data::opcode_by_value(f.opcode)
                            .map(|e| e.signals.pc_sel).unwrap_or(0),
                        branch: d.branch, jump: d.jump,
                        csr_op: 0, is_sfence: false,
                    },
                    csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
                    trap_kind: 0, stall_reg: 0, flush_bubble: false,
                    early_target, early_branch, id_redirected: false,
                });
            }
            None => {
                // Unknown opcode → illegal trap.
                return Some(IdEx {
                    valid: true, pc: if_id.pc, pc_plus4: if_id.pc_plus4, instr,
                    opcode: f.opcode, trap_kind: 1,
                    ..IdEx::default()
                });
            }
        }
    }

    // ── SYSTEM opcode (0x73) ────────────────────────────────
    if f.funct3 != 0 {
        // CSR instruction — use shared decode.
        match decode::decode_csr_instr(&regs, instr, &f) {
            Some(c) => {
                let csr_op = csr_op_from_funct3(f.funct3);
                let _is_csr = csr_op != CsrOp::None;
                return Some(IdEx {
                    valid: true, pc: if_id.pc, pc_plus4: if_id.pc_plus4, instr,
                    rs1_data: c.write_val, rs2_data: 0, imm: 0,
                    rs1_num: f.rs1, rs2_num: f.rs2, rd_num: f.rd,
                    funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
                    ctrl: ControlBits {
                        reg_write: true,
                        wb_sel: 3,
                        csr_op: csr_op.to_u8(),
                        ..ControlBits::default()
                    },
                    csr_addr: c.csr_addr, csr_uimm: c.csr_uimm,
                    csr_imm_form: c.csr_imm_form,
                    trap_kind: 0, stall_reg: 0, flush_bubble: false,
                    early_target: 0, early_branch: false, id_redirected: false,
                });
            }
            None => {
                return Some(IdEx {
                    valid: true, pc: if_id.pc, pc_plus4: if_id.pc_plus4, instr,
                    opcode: f.opcode, trap_kind: 1,
                    ..IdEx::default()
                });
            }
        }
    }

    // SYSTEM funct3==0 — ECALL/EBREAK/MRET/SRET/WFI/SFENCE.VMA
    let funct12 = (instr >> 20) & 0xfff;
    let funct7  = (instr >> 25) & 0x7f;
    let mut trap_kind = 0u8;
    let mut is_sfence = false;
    if funct7 == 0x09 {
        is_sfence = true;
    } else {
        trap_kind = system_trap_kind(funct12);
    }

    Some(IdEx {
        valid: true, pc: if_id.pc, pc_plus4: if_id.pc_plus4, instr,
        rs1_data: 0, rs2_data: 0, imm: 0,
        rs1_num: 0, rs2_num: 0, rd_num: 0,
        funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
        ctrl: ControlBits { is_sfence, ..ControlBits::default() },
        csr_addr: 0, csr_uimm: 0, csr_imm_form: false,
        trap_kind, stall_reg: 0, flush_bubble: false,
        early_target: 0, early_branch: false, id_redirected: false,
    })
}

/// Detect a load-use hazard (unchanged from original).
pub fn detect_load_use_hazard(if_id: &IfId, id_ex: &IdEx) -> bool {
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
