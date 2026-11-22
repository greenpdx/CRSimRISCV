//! `Pipeline`: the 5-stage pipeline (the base core; MicroStepPipelineView).

use serde::Serialize;
use wasm_bindgen::prelude::*;

use super::{core_api, js};
use crate::sim::core::Core;
use crate::sim::pipeline::{
    self, BranchMode, ControlBits, ExMa, ForwardSrc, IdEx, IfId, MaWb, PipelineState, Stage,
};

#[wasm_bindgen]
pub struct Pipeline {
    inner: PipelineState,
}

core_api!(Pipeline);

impl Default for Pipeline {
    fn default() -> Self { Self::new() }
}


#[derive(Serialize, tsify::Tsify)]


#[tsify(missing_as_null)]
struct CtrlJson {
    reg_write: bool,
    alu_src:   bool,
    alu_op:    u8,
    mem_read:  bool,
    mem_write: bool,
    wb_sel:    u8,
    pc_sel:    u8,
    branch:    bool,
    jump:      bool,
    csr_op:    u8,
}
impl From<&ControlBits> for CtrlJson {
    fn from(c: &ControlBits) -> Self {
        Self {
            reg_write: c.reg_write, alu_src: c.alu_src, alu_op: c.alu_op,
            mem_read:  c.mem.is_load(),  mem_write: c.mem.is_store(), wb_sel: c.wb_sel.code(),
            pc_sel: c.pc_sel.code(), branch: c.branch, jump: c.jump,
            csr_op: c.csr_op.to_u8(),
        }
    }
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct IfIdJson {
    valid: bool, pc: u32, instr: u32, pc_plus4: u32,
    flush_bubble: bool,
}
impl From<&IfId> for IfIdJson {
    fn from(r: &IfId) -> Self {
        Self {
            valid: r.valid, pc: r.pc, instr: r.instr, pc_plus4: r.pc_plus4,
            flush_bubble: r.flush_bubble,
        }
    }
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct IdExJson {
    valid: bool, pc: u32, pc_plus4: u32, instr: u32,
    rs1_data: u32, rs2_data: u32, imm: u32,
    rs1_num: u8, rs2_num: u8, rd_num: u8,
    funct3: u8, funct7b30: u8, opcode: u8,
    ctrl: CtrlJson,
    csr_addr: u16, csr_uimm: u8, csr_imm_form: bool,
    stall_reg: u8,
    flush_bubble: bool,
}
impl From<&IdEx> for IdExJson {
    fn from(r: &IdEx) -> Self {
        Self {
            valid: r.valid, pc: r.pc, pc_plus4: r.pc_plus4, instr: r.instr,
            rs1_data: r.rs1_data, rs2_data: r.rs2_data, imm: r.imm,
            rs1_num: r.rs1_num, rs2_num: r.rs2_num, rd_num: r.rd_num,
            funct3: r.funct3, funct7b30: r.funct7b30, opcode: r.opcode,
            ctrl: (&r.ctrl).into(),
            csr_addr: r.csr_addr, csr_uimm: r.csr_uimm, csr_imm_form: r.csr_imm_form,
            stall_reg: r.stall_reg,
            flush_bubble: r.flush_bubble,
        }
    }
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct ExMaJson {
    valid: bool, instr: u32,
    alu_result: u32, rs2_data: u32, pc_plus4: u32,
    rd_num: u8, funct3: u8,
    branch_taken: bool, branch_target: u32,
    ctrl: CtrlJson,
    csr_addr: u16, csr_write_val: u32,
    stall_reg: u8,
    flush_bubble: bool,
}
impl From<&ExMa> for ExMaJson {
    fn from(r: &ExMa) -> Self {
        Self {
            valid: r.valid, instr: r.instr,
            alu_result: r.alu_result, rs2_data: r.rs2_data,
            pc_plus4: r.pc_plus4, rd_num: r.rd_num, funct3: r.funct3,
            branch_taken: r.branch_taken, branch_target: r.branch_target,
            ctrl: (&r.ctrl).into(),
            csr_addr: r.csr_addr, csr_write_val: r.csr_write_val,
            stall_reg: r.stall_reg,
            flush_bubble: r.flush_bubble,
        }
    }
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct MaWbJson {
    valid: bool, instr: u32,
    alu_result: u32, mem_data: u32, pc_plus4: u32,
    rd_num: u8, ctrl: CtrlJson,
    stall_reg: u8,
    flush_bubble: bool,
}
impl From<&MaWb> for MaWbJson {
    fn from(r: &MaWb) -> Self {
        Self {
            valid: r.valid, instr: r.instr,
            alu_result: r.alu_result, mem_data: r.mem_data,
            pc_plus4: r.pc_plus4, rd_num: r.rd_num,
            ctrl: (&r.ctrl).into(),
            stall_reg: r.stall_reg,
            flush_bubble: r.flush_bubble,
        }
    }
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct HazardJson { stall: bool, flush: bool, stall_reg: u8 }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct ForwardJson { a: String, b: String }

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct ActiveComponentsJson {
    #[serde(rename = "IF")] if_: Vec<&'static str>,
    #[serde(rename = "ID")] id:  Vec<&'static str>,
    #[serde(rename = "EX")] ex:  Vec<&'static str>,
    #[serde(rename = "MA")] ma:  Vec<&'static str>,
    #[serde(rename = "WB")] wb:  Vec<&'static str>,
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct PipelineSnapshot {
    cycle: u64,
    pc: u32,
    halted: bool,
    priv_mode: u8,
    branch_mode: u8,
    registers: Vec<u32>,
    reg_read_mask: u32,
    reg_write_mask: u32,
    last_ram_addr: u32,
    last_ram_kind: u8,
    last_ram_side: u8,
    if_id: IfIdJson,
    id_ex: IdExJson,
    ex_ma: ExMaJson,
    ma_wb: MaWbJson,
    last_wb: MaWbJson,
    hazard: HazardJson,
    forward: ForwardJson,
    active_components: ActiveComponentsJson,
    csrs: Vec<PipeCsrJson>,
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct PipeCsrJson {
    addr: u16,
    value: u32,
    changed: bool,
}

fn forward_name(s: ForwardSrc) -> &'static str {
    match s {
        ForwardSrc::Regfile => "regfile",
        ForwardSrc::ExMa    => "ex_ma",
        ForwardSrc::MaWb    => "ma_wb",
    }
}

#[wasm_bindgen]
impl Pipeline {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self { inner: PipelineState::new() } }

    /// Branch handling mode (`BranchMode` as a number).
    #[wasm_bindgen(getter)] pub fn branch_mode(&self) -> u8 { self.inner.branch_mode.to_u8() }
    pub fn set_branch_mode(&mut self, mode: u8) { self.inner.branch_mode = BranchMode::from_u8(mode); }

    /// User-side RAM (U / S accesses; not on the outside bus `mem` reads).
    pub fn user_ram(&self) -> Vec<u32> { self.inner.mem.flat().map(|f| f.user_ram.clone()).unwrap_or_default() }
    /// Write one user-RAM word at byte address `addr`.
    pub fn poke_user_ram(&mut self, addr: u32, val: u32) { self.inner.user_ram_write(addr, val); }

    /// One-call full snapshot for the Vue UI.
    #[wasm_bindgen(unchecked_return_type = "PipelineSnapshot")]
    pub fn snapshot(&self) -> JsValue {
        let active = ActiveComponentsJson {
            if_: pipeline::active_components(&self.inner, Stage::IF),
            id:  pipeline::active_components(&self.inner, Stage::ID),
            ex:  pipeline::active_components(&self.inner, Stage::EX),
            ma:  pipeline::active_components(&self.inner, Stage::MA),
            wb:  pipeline::active_components(&self.inner, Stage::WB),
        };
        let csrs: Vec<PipeCsrJson> = crate::sim::pipeline::csr::known_addrs()
            .iter().enumerate()
            .map(|(i, &a)| {
                let v = self.inner.sys.csrs.read(a);
                let prev = self.inner.last_csr_snapshot.get(i).copied().unwrap_or(0);
                PipeCsrJson { addr: a, value: v, changed: v != prev }
            })
            .collect();
        let snap = PipelineSnapshot {
            cycle: self.inner.cycle_count,
            pc: self.inner.pc,
            halted: self.inner.halted,
            priv_mode: self.inner.priv_bits(),
            branch_mode: self.inner.branch_mode.to_u8(),
            registers: self.inner.registers.to_vec(),
            reg_read_mask: self.inner.reg_read_mask,
            reg_write_mask: self.inner.reg_write_mask,
            last_ram_addr: self.inner.last_ram_addr,
            last_ram_kind: self.inner.last_ram_kind,
            last_ram_side: self.inner.last_ram_side,
            csrs,
            if_id: (&self.inner.if_id).into(),
            id_ex: (&self.inner.id_ex).into(),
            ex_ma: (&self.inner.ex_ma).into(),
            ma_wb: (&self.inner.ma_wb).into(),
            last_wb: (&self.inner.last_wb).into(),
            hazard: HazardJson {
                stall: self.inner.last_stall,
                flush: self.inner.last_flush,
                stall_reg: self.inner.last_stall_reg,
            },
            forward: ForwardJson {
                a: forward_name(self.inner.last_forward_a).into(),
                b: forward_name(self.inner.last_forward_b).into(),
            },
            active_components: active,
        };
        js(&snap)
    }

}
