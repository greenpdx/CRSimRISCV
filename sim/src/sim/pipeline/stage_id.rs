//! Pipeline stage 2 — Instruction Decode + Register Read (ID).
//!
//! Uses the shared generic decoder (`sim::decode`) for standard
//! RV32I, CSR, and SYSTEM instructions. Adds pipeline-specific
//! SFENCE.VMA detection, early branch target computation, and
//! flush/stall-bubble propagation.
//!
//! Also runs the **load-use hazard detection**.

use crate::isa::instr::{decode, Instr};
use crate::isa::fields::extract_fields;
use crate::sim::decode::{self, RegRead, is_jalr};
use crate::sim::stage::system_trap;
use super::state::{ControlBits, CsrOp, IdEx, IdExData, IfId, MemKind, PcSel, PipelineState, TrapKind, WbSel};

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
    let (pc, instr) = (if_id.pc, if_id.instr);
    let latch = |d: IdExData| Some(IdEx::new(pc, instr, IdExData { pc_plus4: if_id.pc_plus4, ..d }));

    // IF stage already detected an instr page fault — propagate.
    if if_id.trap_kind.is_some() {
        return latch(IdExData { trap_kind: if_id.trap_kind, ..IdExData::default() });
    }

    let f = extract_fields(instr);
    let regs = PipelineRegs { state };
    let illegal = || latch(IdExData { opcode: f.opcode, trap_kind: TrapKind::Illegal, ..IdExData::default() });

    // ── Standard RV32I decode (via shared generic) ──────────
    // Try the shared decoder first; if it returns None the opcode
    // is unknown → illegal trap.
    let is_system = f.opcode == 0b1110011;

    let d = decode(instr);

    // FENCE, FENCE.I: a NOP here — memory is in order, and in memory-panel
    // mode a store already invalidates the matching L1I line.
    if matches!(d, Instr::Fence | Instr::FenceI) {
        return latch(IdExData { opcode: f.opcode, ..IdExData::default() });
    }

    if !is_system {
        let Some(d) = decode::decode_standard(&regs, instr, &f) else { return illegal() };
        let early_branch = d.branch || (d.jump && !is_jalr(f.opcode));
        let early_target = if early_branch { pc.wrapping_add(d.imm) } else { 0 };
        let pc_sel = crate::data::control_data::opcode_by_value(f.opcode).map(|e| e.signals.pc_sel).unwrap_or(0);
        return latch(IdExData {
            rs1_data: d.rs1_data, rs2_data: d.rs2_data, imm: d.imm,
            rs1_num: f.rs1, rs2_num: f.rs2, rd_num: f.rd,
            funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
            ctrl: ControlBits {
                reg_write: d.reg_write, alu_src: d.alu_src, alu_op: d.alu_op,
                mem: MemKind::from_flags(d.mem_read, d.mem_write),
                wb_sel: WbSel::from_code(d.wb_sel),
                pc_sel: PcSel::from_code(pc_sel),
                branch: d.branch, jump: d.jump,
                csr_op: CsrOp::None, is_sfence: false,
            },
            early_target, early_branch,
            ..IdExData::default()
        });
    }

    // ── SYSTEM opcode (0x73) ────────────────────────────────
    if f.funct3 != 0 {
        // CSR instruction — use shared decode.
        let Some(c) = decode::decode_csr_instr(&regs, instr) else { return illegal() };
        return latch(IdExData {
            rs1_data: c.write_val, rs2_data: 0, imm: 0,
            rs1_num: f.rs1, rs2_num: f.rs2, rd_num: f.rd,
            funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
            ctrl: ControlBits {
                reg_write: true,
                wb_sel: WbSel::Csr,
                csr_op: CsrOp::from_funct3(f.funct3),
                ..ControlBits::default()
            },
            csr_addr: c.csr_addr, csr_uimm: c.csr_uimm,
            csr_imm_form: c.csr_imm_form,
            ..IdExData::default()
        });
    }

    // SYSTEM funct3==0 — ECALL/EBREAK/MRET/SRET/WFI/SFENCE.VMA
    let is_sfence = matches!(d, Instr::SfenceVma { .. });
    let trap_kind = if is_sfence { TrapKind::None } else { system_trap(&d) };

    // SFENCE.VMA rs1, rs2 reads its operands (vaddr, ASID); EX forwards
    // them: rs1 → ex_ma.csr_write_val, rs2 → ex_ma.rs2_data.
    let (rs1_num, rs2_num) = if is_sfence { (f.rs1, f.rs2) } else { (0, 0) };
    latch(IdExData {
        rs1_data: regs.read_reg(rs1_num), rs2_data: regs.read_reg(rs2_num), imm: 0,
        rs1_num, rs2_num, rd_num: 0,
        funct3: f.funct3, funct7b30: f.funct7b30, opcode: f.opcode,
        ctrl: ControlBits { is_sfence, ..ControlBits::default() },
        trap_kind,
        ..IdExData::default()
    })
}

/// Detect a load-use hazard (unchanged from original).
pub fn detect_load_use_hazard(if_id: &IfId, id_ex: &IdEx) -> bool {
    let produces_in_ma = id_ex.ctrl.mem.is_load() || id_ex.ctrl.csr_op != CsrOp::None;
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
