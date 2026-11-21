//! Browser-facing wasm-bindgen wrapper around `CpuState`.
//!
//! Build with:
//!   wasm-pack build sim --target web --features wasm
//!
//! In the browser:
//!   import init, { JsCpuState } from './pkg/cpu_sim.js';
//!   await init();
//!   const cpu = new JsCpuState();
//!   cpu.load_program(new Uint32Array([0x00700193]));
//!   const trace = cpu.step();   // → { Fetch: ["pc","rom"], ... }
//!   cpu.poke_register(3, 42);

use crate::disasm::csrs::{csr_desc, csr_name};
use crate::disasm::descriptions::mnemonic_desc;
use crate::sim::cpu_state::CpuState;
use crate::sim::phase::Phase;
use crate::sim::trap::{IRQ_MEI, IRQ_MSI, IRQ_MTI, IRQ_SEI, IRQ_SSI, IRQ_STI};
use serde::Serialize;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct JsCpuState {
    inner: CpuState,
}

#[derive(Serialize)]
struct TraceJson {
    cycle: u64,
    pc: u32,
    by_phase: BTreeMap<String, Vec<&'static str>>,
}

#[wasm_bindgen]
impl JsCpuState {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self { inner: CpuState::new() } }

    pub fn reset(&mut self) { self.inner = CpuState::new(); }

    /// Load a program (Uint32Array of instruction words) starting at ROM base.
    pub fn load_program(&mut self, words: &[u32]) { self.inner.load_program(words); }

    /// Execute one instruction. Returns a JS object:
    /// `{ cycle, pc, by_phase: { Fetch: [...], Decode: [...], ... } }`
    pub fn step(&mut self) -> JsValue {
        let trace = self.inner.step_traced();
        let mut by_phase: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
        for (phase, set) in trace.by_phase.iter() {
            by_phase.insert(phase_name(*phase).to_string(), set.iter().copied().collect());
        }
        let payload = TraceJson {
            cycle: self.inner.cycle_count,
            pc: self.inner.pc,
            by_phase,
        };
        serde_wasm_bindgen::to_value(&payload).unwrap_or(JsValue::NULL)
    }

    // ── State accessors ─────────────────────────────────────────────────
    #[wasm_bindgen(getter)] pub fn pc(&self) -> u32 { self.inner.pc }
    #[wasm_bindgen(getter)] pub fn cycle(&self) -> u64 { self.inner.cycle_count }
    #[wasm_bindgen(getter)] pub fn halted(&self) -> bool { self.inner.halted }

    pub fn read_register(&self, idx: u8) -> u32 {
        self.inner.registers[(idx & 0x1f) as usize]
    }

    /// Returns Uint32Array of all 32 registers.
    pub fn registers(&self) -> Vec<u32> { self.inner.registers_snapshot().to_vec() }
    pub fn ram(&self) -> Vec<u32> { self.inner.ram_snapshot().to_vec() }
    pub fn rom(&self) -> Vec<u32> { self.inner.rom_snapshot().to_vec() }

    // ── Live debugger pokes ─────────────────────────────────────────────
    /// Disassemble one 32-bit instruction word.
    /// Returns `{ class, mnemonic, operands }`.
    pub fn disasm(&self, instr: u32) -> JsValue {
        let d = crate::disasm::disassemble(instr);
        let desc = mnemonic_desc(&d.mnemonic).unwrap_or("").to_string();
        let s = DisasmJson { class: d.class, mnemonic: d.mnemonic, operands: d.operands, desc };
        serde_wasm_bindgen::to_value(&s).unwrap_or(JsValue::NULL)
    }
    // (DisasmJson now uses String for mnemonic — see definition below.)

    pub fn poke_register(&mut self, idx: u8, val: u32) { self.inner.poke_register(idx, val); }
    pub fn poke_ram(&mut self, byte_addr: u32, val: u32) { self.inner.poke_ram(byte_addr, val); }
    pub fn poke_rom(&mut self, byte_addr: u32, val: u32) { self.inner.poke_rom(byte_addr, val); }

    // ── Privilege & traps ────────────────────────────────────────────────
    #[wasm_bindgen(getter)] pub fn priv_mode(&self) -> String {
        self.inner.priv_mode().as_str().to_string()
    }
    #[wasm_bindgen(getter)] pub fn in_trap(&self) -> bool { self.inner.in_trap() }

    pub fn raise_irq(&mut self, kind: &str) {
        let mask = match kind {
            "mei" => IRQ_MEI, "msi" => IRQ_MSI, "mti" => IRQ_MTI,
            "sei" => IRQ_SEI, "ssi" => IRQ_SSI, "sti" => IRQ_STI,
            _ => 0,
        };
        if mask != 0 { self.inner.raise_irq(mask); }
    }
    pub fn clear_irq(&mut self) { self.inner.clear_irq(); }

    pub fn csr_read(&self, addr: u16) -> u32 { self.inner.csr_read(addr) }
    pub fn csr_write(&mut self, addr: u16, val: u32) { self.inner.csr_write(addr, val); }

    /// Returns the populated CSRs as `[{addr, name, value}]`, sorted.
    pub fn csr_snapshot(&self) -> JsValue {
        #[derive(Serialize)]
        struct CsrEntry { addr: u16, name: String, value: u32, desc: String }
        let entries: Vec<CsrEntry> = self.inner.csrs_snapshot().into_iter()
            .map(|(addr, value)| CsrEntry {
                addr,
                name: csr_name(addr).unwrap_or("?").to_string(),
                value,
                desc: csr_desc(addr).unwrap_or("").to_string(),
            })
            .collect();
        serde_wasm_bindgen::to_value(&entries).unwrap_or(JsValue::NULL)
    }

    /// Returns the most recent trap, or null.
    pub fn last_trap(&self) -> JsValue {
        #[derive(Serialize)]
        struct TrapJson {
            cycle: u64,
            from: String,
            to: String,
            cause: u32,
            is_interrupt: bool,
            epc: u32,
            tval: u32,
        }
        match self.inner.last_trap() {
            Some(t) => serde_wasm_bindgen::to_value(&TrapJson {
                cycle: t.cycle, from: t.from_mode.as_str().into(),
                to: t.to_mode.as_str().into(), cause: t.cause,
                is_interrupt: t.is_interrupt, epc: t.epc, tval: t.tval,
            }).unwrap_or(JsValue::NULL),
            None => JsValue::NULL,
        }
    }

    // ── Micro-step API ──────────────────────────────────────────────────

    /// Advance one phase. Returns the phase name string.
    pub fn micro_step(&mut self) -> String {
        let p = self.inner.micro_step();
        phase_name(p).to_string()
    }

    /// Snapshot of the current MicroState (all bus + control values populated so far).
    pub fn micro_state(&self) -> JsValue {
        let m = &self.inner.micro;
        let snap = MicroJson {
            current: m.current.map(phase_name).map(str::to_string),
            opcode_name: self.inner.opcode_name().map(str::to_string),
            active: self.inner.last_active().iter().map(|s| s.to_string()).collect(),
            consumed: self.inner.last_consumed().iter().map(|s| s.to_string()).collect(),
            reg_access: self.inner.last_reg_access().to_vec(),
            ram_access: self.inner.last_ram_access().to_vec(),
            rom_access: self.inner.last_rom_access(),
            buses: vec![
                bus("pc_out",        m.pc_out),
                bus("instr",         m.instr),
                bus("pc_plus4",      m.pc_plus4),
                bus("immediate",     m.immediate),
                bus("rs1_data",      m.rs1_data),
                bus("rs2_data",      m.rs2_data),
                bus("alu_b",         m.alu_b),
                bus("alu_control",   m.alu_control.map(|v| v as u32)),
                bus("alu_result",    m.alu_result),
                bus("mem_read_data", m.mem_read_data),
                bus("write_data",    m.write_data),
                bus("pc_offset",     m.pc_offset),
                bus("jalr_target",   m.jalr_target),
                bus("next_pc",       m.next_pc),
            ],
            controls: vec![
                ctrl("reg_write", m.ctrl_reg_write),
                ctrl("alu_src",   m.ctrl_alu_src),
                ctrl("mem_read",  m.ctrl_mem_read),
                ctrl("mem_write", m.ctrl_mem_write),
                ctrl_u("alu_op",  m.ctrl_alu_op),
                ctrl_u("wb_sel",  m.ctrl_wb_sel),
                ctrl_u("pc_sel",  m.ctrl_pc_sel),
                ctrl_u("imm_sel", m.ctrl_imm_sel),
                ctrl("zero",      m.zero_flag),
                ctrl("neg",       m.neg_flag),
                ctrl("carry",     m.carry_flag),
                ctrl("overflow",  m.overflow_flag),
            ],
            components: vec![
                comp("imm_gen",     m.immediate.map(|v| format!("0x{:08x}", v))),
                comp("alu_src_mux", m.alu_b.map(|v| format!("0x{:08x}", v))),
                comp("alu",         m.alu_result.map(|v| format!("0x{:08x}", v))),
                comp("wb_sel_mux",  m.write_data.map(|v| format!("0x{:08x}", v))),
                comp("loadstore",   m.alu_result.map(|v| format!("addr=0x{:08x}", v))),
            ],
        };
        serde_wasm_bindgen::to_value(&snap).unwrap_or(JsValue::NULL)
    }
}

#[derive(Serialize)]
struct MicroJson {
    current: Option<String>,
    opcode_name: Option<String>,
    active: Vec<String>,
    consumed: Vec<String>,
    reg_access: Vec<u8>,
    ram_access: Vec<u32>,
    rom_access: Option<u32>,
    buses: Vec<BusEntry>,
    controls: Vec<CtrlEntry>,
    components: Vec<CompEntry>,
}
#[derive(Serialize)]
struct BusEntry { name: &'static str, value: Option<u32> }
#[derive(Serialize)]
struct CtrlEntry { name: &'static str, value: Option<String> }
#[derive(Serialize)]
struct CompEntry { name: &'static str, value: Option<String> }

#[derive(Serialize)]
struct DisasmJson { class: &'static str, mnemonic: String, operands: String, desc: String }

fn bus(name: &'static str, v: Option<u32>) -> BusEntry { BusEntry { name, value: v } }
fn ctrl(name: &'static str, v: Option<bool>) -> CtrlEntry {
    CtrlEntry { name, value: v.map(|b| if b { "1".into() } else { "0".into() }) }
}
fn ctrl_u(name: &'static str, v: Option<u8>) -> CtrlEntry {
    CtrlEntry { name, value: v.map(|n| n.to_string()) }
}
fn comp(name: &'static str, v: Option<String>) -> CompEntry { CompEntry { name, value: v } }

fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::IrqCheck => "IrqCheck",
        Phase::Fetch    => "Fetch",
        Phase::Decode   => "Decode",
        Phase::Control  => "Control",
        Phase::Imm      => "Imm",
        Phase::RegRead  => "RegRead",
        Phase::Alu      => "Alu",
        Phase::Mem      => "Mem",
        Phase::Wb       => "Wb",
        Phase::PcUpdate => "PcUpdate",
    }
}

// ─────────────────────────────────────────────────────────────────────────
// JsPipelineState — wasm wrapper around the 5-stage pipeline simulator.
// ─────────────────────────────────────────────────────────────────────────

use crate::sim::pipeline::{
    self, BranchMode, ControlBits, ExMa, ForwardSrc, IdEx, IfId, MaWb, PipelineState, Stage,
};

#[wasm_bindgen]
pub struct JsPipelineState {
    inner: PipelineState,
}

#[derive(Serialize)]
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
            mem_read:  c.mem_read,  mem_write: c.mem_write, wb_sel: c.wb_sel,
            pc_sel: c.pc_sel, branch: c.branch, jump: c.jump,
            csr_op: c.csr_op,
        }
    }
}

#[derive(Serialize)]
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

#[derive(Serialize)]
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

#[derive(Serialize)]
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

#[derive(Serialize)]
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

#[derive(Serialize)]
struct HazardJson { stall: bool, flush: bool, stall_reg: u8 }
#[derive(Serialize)]
struct ForwardJson { a: String, b: String }

#[derive(Serialize)]
struct ActiveComponentsJson {
    #[serde(rename = "IF")] if_: Vec<&'static str>,
    #[serde(rename = "ID")] id:  Vec<&'static str>,
    #[serde(rename = "EX")] ex:  Vec<&'static str>,
    #[serde(rename = "MA")] ma:  Vec<&'static str>,
    #[serde(rename = "WB")] wb:  Vec<&'static str>,
}

#[derive(Serialize)]
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
    csrs: Vec<CsrEntryJson>,
}

#[derive(Serialize)]
struct CsrEntryJson {
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
impl JsPipelineState {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self { inner: PipelineState::new() } }

    pub fn reset(&mut self) { self.inner.reset(); }

    pub fn load_program(&mut self, words: &[u32]) {
        self.inner.load_program(words);
    }

    pub fn step(&mut self) { self.inner.step(); }

    // ── Scalar getters ─────────────────────────────────────────────────
    #[wasm_bindgen(getter)] pub fn pc(&self) -> u32 { self.inner.pc }
    #[wasm_bindgen(getter)] pub fn cycle(&self) -> u64 { self.inner.cycle_count }
    #[wasm_bindgen(getter)] pub fn halted(&self) -> bool { self.inner.halted }
    #[wasm_bindgen(getter)] pub fn last_stall(&self) -> bool { self.inner.last_stall }
    #[wasm_bindgen(getter)] pub fn last_flush(&self) -> bool { self.inner.last_flush }
    #[wasm_bindgen(getter)] pub fn last_forward_a(&self) -> String {
        forward_name(self.inner.last_forward_a).to_string()
    }
    #[wasm_bindgen(getter)] pub fn last_forward_b(&self) -> String {
        forward_name(self.inner.last_forward_b).to_string()
    }

    // ── Storage accessors ──────────────────────────────────────────────
    pub fn read_register(&self, idx: u8) -> u32 { self.inner.read_register(idx) }
    pub fn registers(&self) -> Vec<u32> { self.inner.registers.to_vec() }
    /// Returns machine-side RAM (alias kept for back-compat with the
    /// pre-split visualizer code).
    pub fn ram(&self) -> Vec<u32> { self.inner.machine_ram.clone() }
    pub fn machine_ram(&self) -> Vec<u32> { self.inner.machine_ram.clone() }
    pub fn user_ram(&self) -> Vec<u32> { self.inner.user_ram.clone() }
    pub fn rom(&self) -> Vec<u32> { self.inner.rom.clone() }

    // ── Per-cycle activity (for the visualizer) ────────────────────────
    #[wasm_bindgen(getter)] pub fn reg_read_mask(&self) -> u32 { self.inner.reg_read_mask }
    #[wasm_bindgen(getter)] pub fn reg_write_mask(&self) -> u32 { self.inner.reg_write_mask }
    #[wasm_bindgen(getter)] pub fn last_ram_addr(&self) -> u32 { self.inner.last_ram_addr }
    #[wasm_bindgen(getter)] pub fn last_ram_kind(&self) -> u8 { self.inner.last_ram_kind }
    #[wasm_bindgen(getter)] pub fn last_ram_side(&self) -> u8 { self.inner.last_ram_side }
    #[wasm_bindgen(getter)] pub fn priv_mode(&self) -> u8 { self.inner.priv_bits() }
    #[wasm_bindgen(getter)] pub fn branch_mode(&self) -> u8 { self.inner.branch_mode.to_u8() }
    pub fn set_branch_mode(&mut self, mode: u8) { self.inner.branch_mode = BranchMode::from_u8(mode); }

    // ── Pipeline-register snapshots ────────────────────────────────────
    pub fn if_id(&self) -> JsValue {
        let j: IfIdJson = (&self.inner.if_id).into();
        serde_wasm_bindgen::to_value(&j).unwrap_or(JsValue::NULL)
    }
    pub fn id_ex(&self) -> JsValue {
        let j: IdExJson = (&self.inner.id_ex).into();
        serde_wasm_bindgen::to_value(&j).unwrap_or(JsValue::NULL)
    }
    pub fn ex_ma(&self) -> JsValue {
        let j: ExMaJson = (&self.inner.ex_ma).into();
        serde_wasm_bindgen::to_value(&j).unwrap_or(JsValue::NULL)
    }
    pub fn ma_wb(&self) -> JsValue {
        let j: MaWbJson = (&self.inner.ma_wb).into();
        serde_wasm_bindgen::to_value(&j).unwrap_or(JsValue::NULL)
    }
    pub fn last_wb(&self) -> JsValue {
        let j: MaWbJson = (&self.inner.last_wb).into();
        serde_wasm_bindgen::to_value(&j).unwrap_or(JsValue::NULL)
    }

    /// One-call full snapshot for the Vue UI.
    pub fn snapshot(&self) -> JsValue {
        let active = ActiveComponentsJson {
            if_: pipeline::active_components(&self.inner, Stage::IF),
            id:  pipeline::active_components(&self.inner, Stage::ID),
            ex:  pipeline::active_components(&self.inner, Stage::EX),
            ma:  pipeline::active_components(&self.inner, Stage::MA),
            wb:  pipeline::active_components(&self.inner, Stage::WB),
        };
        let csrs: Vec<CsrEntryJson> = crate::sim::pipeline::csr::known_addrs()
            .iter().enumerate()
            .map(|(i, &a)| {
                let v = self.inner.sys.csrs.read(a);
                let prev = self.inner.last_csr_snapshot.get(i).copied().unwrap_or(0);
                CsrEntryJson { addr: a, value: v, changed: v != prev }
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
        serde_wasm_bindgen::to_value(&snap).unwrap_or(JsValue::NULL)
    }

    // ── Live debugger pokes ────────────────────────────────────────────
    pub fn poke_register(&mut self, idx: u8, val: u32) {
        let i = idx as usize & 0x1f;
        if i != 0 { self.inner.registers[i] = val; }
    }
    pub fn poke_ram(&mut self, byte_addr: u32, val: u32) {
        self.inner.ram_write(byte_addr, val);
    }
    pub fn poke_user_ram(&mut self, byte_addr: u32, val: u32) {
        self.inner.user_ram_write(byte_addr, val);
    }
    pub fn poke_machine_ram(&mut self, byte_addr: u32, val: u32) {
        self.inner.ram_write(byte_addr, val);
    }
    pub fn poke_rom(&mut self, byte_addr: u32, val: u32) {
        let idx = (byte_addr >> 2) as usize;
        if idx < self.inner.rom.len() { self.inner.rom[idx] = val; }
    }

    // ── Zicsr CSR accessors ────────────────────────────────────────────
    pub fn csr_read(&self, addr: u16) -> u32 { self.inner.sys.csrs.read(addr) }
    pub fn poke_csr(&mut self, addr: u16, val: u32) { self.inner.sys.csrs.write(addr, val); }
    /// Returns a JS array of `{ addr, value }` for every CSR known to the file.
    pub fn csr_snapshot(&self) -> JsValue {
        #[derive(Serialize)]
        struct Entry { addr: u16, value: u32 }
        let v: Vec<Entry> = crate::sim::pipeline::csr::known_addrs()
            .iter()
            .map(|&a| Entry { addr: a, value: self.inner.sys.csrs.read(a) })
            .collect();
        serde_wasm_bindgen::to_value(&v).unwrap_or(JsValue::NULL)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// JsBarrelState — wasm wrapper around the 8-thread barrel processor.
// ─────────────────────────────────────────────────────────────────────────

use crate::sim::barrel::{
    self, BarrelState, IfDi as BIfDi, DiEx as BDiEx, ExMa as BExMa, MaWb as BMaWb,
    LtIf as BLtIf, SchedMode as BSchedMode, N_THREADS as BN_THREADS,
};

#[wasm_bindgen]
pub struct JsBarrelState {
    inner: BarrelState,
}

#[derive(Serialize)]
struct BThreadJson {
    pc: u32,
    regs: Vec<u32>,
    active: bool,
    waiting: i8,
    priority: u8,
    age: u8,
    miss_pending: bool,
    instr_access_fault: bool,
    priv_mode: u8,
    mstatus: u32,
    mepc:    u32,
    mcause:  u32,
    mtval:   u32,
    mscratch:u32,
    satp:    u32,
}
impl From<&barrel::Thread> for BThreadJson {
    fn from(t: &barrel::Thread) -> Self {
        Self {
            pc: t.pc,
            regs: t.regs.to_vec(),
            active: t.active,
            waiting: t.waiting,
            priority: t.priority,
            age: t.age,
            miss_pending: t.miss_pending,
            instr_access_fault: t.instr_access_fault,
            priv_mode: t.priv_bits(),
            mstatus: t.sys.csrs.read(0x300), mepc: t.sys.csrs.read(0x341),
            mcause: t.sys.csrs.read(0x342), mtval: t.sys.csrs.read(0x343),
            mscratch: t.sys.csrs.read(0x340), satp: t.sys.csrs.read(0x180),
        }
    }
}

#[derive(Serialize)]
struct BCtrlJson {
    reg_write: bool,
    mem_read: bool,
    mem_write: bool,
    custom_op: u8,
}
impl From<&barrel::ControlBits> for BCtrlJson {
    fn from(c: &barrel::ControlBits) -> Self {
        Self {
            reg_write: c.reg_write,
            mem_read:  c.mem_read,
            mem_write: c.mem_write,
            custom_op: c.custom_op as u8,
        }
    }
}

#[derive(Serialize)]
struct BLtIfJson { valid: bool, thread: u8, pc: u32 }
#[derive(Serialize)]
struct BIfDiJson { valid: bool, thread: u8, pc: u32, instr: u32, src: u8 }
#[derive(Serialize)]
struct BDiExJson {
    valid: bool, thread: u8, pc: u32, instr: u32,
    rs1_num: u8, rs2_num: u8, rd_num: u8,
    ctrl: BCtrlJson,
}
#[derive(Serialize)]
struct BExMaJson {
    valid: bool, thread: u8, instr: u32,
    alu_result: u32, rd_num: u8,
    branch_taken: bool, branch_target: u32,
    ctrl: BCtrlJson,
}
#[derive(Serialize)]
struct BMaWbJson {
    valid: bool, thread: u8, instr: u32,
    alu_result: u32, mem_data: u32, rd_num: u8,
    ctrl: BCtrlJson,
}

impl From<&BLtIf> for BLtIfJson {
    fn from(r: &BLtIf) -> Self {
        Self { valid: r.valid, thread: r.thread, pc: r.pc }
    }
}
impl From<&BIfDi> for BIfDiJson {
    fn from(r: &BIfDi) -> Self {
        Self { valid: r.valid, thread: r.thread, pc: r.pc, instr: r.instr, src: r.src as u8 }
    }
}
impl From<&BDiEx> for BDiExJson {
    fn from(r: &BDiEx) -> Self {
        Self {
            valid: r.valid, thread: r.thread, pc: r.pc, instr: r.instr,
            rs1_num: r.rs1_num, rs2_num: r.rs2_num, rd_num: r.rd_num,
            ctrl: (&r.ctrl).into(),
        }
    }
}
impl From<&BExMa> for BExMaJson {
    fn from(r: &BExMa) -> Self {
        Self {
            valid: r.valid, thread: r.thread, instr: r.instr,
            alu_result: r.alu_result, rd_num: r.rd_num,
            branch_taken: r.branch_taken, branch_target: r.branch_target,
            ctrl: (&r.ctrl).into(),
        }
    }
}
impl From<&BMaWb> for BMaWbJson {
    fn from(r: &BMaWb) -> Self {
        Self {
            valid: r.valid, thread: r.thread, instr: r.instr,
            alu_result: r.alu_result, mem_data: r.mem_data, rd_num: r.rd_num,
            ctrl: (&r.ctrl).into(),
        }
    }
}

#[derive(Serialize)]
struct BCacheJson { hits: u64, misses: u64, busy: bool }
#[derive(Serialize)]
struct BBusJson { state: u8 }    // 0=Idle 1=FillI 2=FillD 3=Storing
#[derive(Serialize)]
struct BSnapshot {
    cycle: u64,
    halted: bool,
    sched_mode: u8,
    current_thread: u8,
    mtvec: u32,
    mie: u32,
    mip: u32,
    threads: Vec<BThreadJson>,
    lt_if: BLtIfJson,
    if_di: BIfDiJson,
    di_ex: BDiExJson,
    ex_ma: BExMaJson,
    ma_wb: BMaWbJson,
    last_wb: BMaWbJson,
    sched_history: Vec<u8>,
    icache: BCacheJson,
    dcache: BCacheJson,
    store_buf_count: usize,
    bus: BBusJson,
}

#[wasm_bindgen]
impl JsBarrelState {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self { inner: BarrelState::new() } }

    pub fn reset(&mut self) { self.inner = BarrelState::new(); }
    pub fn step(&mut self) { self.inner.step(); }
    pub fn step_n(&mut self, n: u32) { for _ in 0..n { self.inner.step(); } }

    // Memory loaders
    pub fn load_program(&mut self, words: &[u32]) { self.inner.load_program(words); }
    pub fn load_isram(&mut self, slot: u8, words: &[u32]) { self.inner.load_isram(slot, words); }
    pub fn prewarm_icache(&mut self) { self.inner.prewarm_icache(); }

    // Thread setup
    pub fn set_thread_active(&mut self, slot: u8, active: bool) {
        if (slot as usize) < BN_THREADS { self.inner.threads[slot as usize].active = active; }
    }
    pub fn set_thread_pc(&mut self, slot: u8, pc: u32) { self.inner.set_thread_pc(slot, pc); }
    pub fn set_thread_priority(&mut self, slot: u8, prio: u8) {
        if (slot as usize) < BN_THREADS { self.inner.threads[slot as usize].priority = prio & 0x3; }
    }
    pub fn activate_all(&mut self) { self.inner.activate_all(); }

    // ── IRQ + per-thread CSR access (B7) ──
    pub fn inject_irq(&mut self, slot: u8) { self.inner.inject_irq(slot); }
    /// External interrupt line (mei / msi / mti / sei / ssi / sti) into
    /// every thread's system unit; a thread traps when it allows it.
    pub fn raise_irq(&mut self, kind: &str) {
        let mask = match kind {
            "mei" => IRQ_MEI, "msi" => IRQ_MSI, "mti" => IRQ_MTI,
            "sei" => IRQ_SEI, "ssi" => IRQ_SSI, "sti" => IRQ_STI,
            _ => 0,
        };
        if mask != 0 { self.inner.raise_irq(mask); }
    }
    pub fn clear_irq(&mut self) { self.inner.clear_irq(); }
    pub fn ext_imem(&self) -> Vec<u32> { self.inner.ext_imem.clone() }
    pub fn ext_dmem(&self) -> Vec<u32> { self.inner.ext_dmem.clone() }
    pub fn read_isram_word(&self, slot: u8, byte_off: u32) -> u32 {
        self.inner.isram_read(slot, byte_off)
    }
    pub fn read_csr(&self, slot: u8, addr: u16) -> u32 { self.inner.read_csr(slot, addr) }
    pub fn write_csr(&mut self, slot: u8, addr: u16, val: u32) { self.inner.write_csr(slot, addr, val); }

    // Scheduler
    #[wasm_bindgen(getter)]
    pub fn sched_mode(&self) -> u8 { self.inner.sched_mode as u8 }
    pub fn set_sched_mode(&mut self, m: u8) { self.inner.sched_mode = BSchedMode::from_u8(m); }

    // Scalar getters
    #[wasm_bindgen(getter)]
    pub fn cycle(&self) -> u64 { self.inner.cycle_count }

    // Snapshot
    pub fn snapshot(&self) -> JsValue {
        let snap = BSnapshot {
            cycle: self.inner.cycle_count,
            halted: self.inner.halted,
            sched_mode: self.inner.sched_mode as u8,
            current_thread: self.inner.current_thread,
            mtvec: self.inner.read_csr(0, 0x305),
            mie: self.inner.read_csr(0, 0x304),
            mip: self.inner.read_csr(0, 0x344),
            threads: self.inner.threads.iter().map(|t| t.into()).collect(),
            lt_if: (&self.inner.lt_if).into(),
            if_di: (&self.inner.if_di).into(),
            di_ex: (&self.inner.di_ex).into(),
            ex_ma: (&self.inner.ex_ma).into(),
            ma_wb: (&self.inner.ma_wb).into(),
            last_wb: (&self.inner.last_wb).into(),
            sched_history: self.inner.sched_history.to_vec(),
            icache: BCacheJson {
                hits: self.inner.icache.hits,
                misses: self.inner.icache.misses,
                busy: self.inner.icache.pending.is_some(),
            },
            dcache: BCacheJson {
                hits: self.inner.dcache.hits,
                misses: self.inner.dcache.misses,
                busy: self.inner.dcache.pending.is_some(),
            },
            store_buf_count: self.inner.store_buffer.count,
            bus: BBusJson {
                state: match self.inner.bus.state {
                    barrel::bus::BusState::Idle           => 0,
                    barrel::bus::BusState::FillingI { .. } => 1,
                    barrel::bus::BusState::FillingD { .. } => 2,
                    barrel::bus::BusState::Storing  { .. } => 3,
                },
            },
        };
        serde_wasm_bindgen::to_value(&snap).unwrap_or(JsValue::NULL)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Memory panel: two pipeline cores on the shared cache hierarchy (memsys).
// The panel only draws what these return; all behaviour is in Rust.
// ─────────────────────────────────────────────────────────────────────────

use crate::sim::memsys::{self, system::MemPanelSystem, Mesi};

#[derive(Serialize)]
struct MpStage { valid: bool, pc: u32, instr: u32, bubble: bool }
#[derive(Serialize)]
struct MpTlb { valid: bool, vpn: u32, ppn: u32, pte: u32, is_super: bool, asid: u16, global: bool }
#[derive(Serialize)]
struct MpCore {
    pc: u32, priv_mode: u8,
    /// CSRs the MMU and the trap path use.
    satp: u32, mstatus: u32, mtvec: u32, mepc: u32, mcause: u32, mtval: u32, mhartid: u32,
    /// Frozen waiting for memory, and for how many more cycles.
    stalled: bool, stall_left: u32,
    /// IF, ID, EX, MA, WB.
    stages: Vec<MpStage>,
    itlb: Vec<MpTlb>,
    dtlb: Vec<MpTlb>,
    /// PLRU tree bits of the ITLB / DTLB.
    itlb_plru: u8,
    dtlb_plru: u8,
}
#[derive(Serialize)]
struct MpLine { state: Mesi, tag: u32, lru: u8, addr: u32, data: [u32; 4] }
#[derive(Serialize)]
struct MpCache { sets: usize, ways: usize, lines: Vec<MpLine> }
#[derive(Serialize)]
struct MpCaches { l1i: Vec<MpCache>, l1d: Vec<MpCache>, l2: MpCache }
#[derive(Serialize)]
struct MpExample { name: &'static str, about: &'static str }

fn tlb(t: &crate::sim::pipeline::tlb::Tlb) -> Vec<MpTlb> {
    t.entries.iter().map(|e| MpTlb { valid: e.valid, vpn: e.vpn, ppn: e.ppn, pte: e.pte, is_super: e.is_super, asid: e.asid, global: e.global }).collect()
}

fn mp_cache(c: &memsys::Cache) -> MpCache {
    let lines = (0..c.sets).flat_map(|s| (0..c.ways).map(move |w| (s, w))).map(|(s, w)| {
        let l = c.line(s, w);
        MpLine { state: l.state, tag: l.tag, lru: l.lru, addr: c.line_addr(s, l.tag), data: l.data }
    }).collect();
    MpCache { sets: c.sets, ways: c.ways, lines }
}

#[wasm_bindgen]
pub struct JsMemPanel { inner: MemPanelSystem }

impl Default for JsMemPanel {
    fn default() -> Self { Self::new() }
}

#[wasm_bindgen]
impl JsMemPanel {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self { inner: MemPanelSystem::new() } }

    /// Fresh machine (empty memory, cold caches / TLBs).
    pub fn reset(&mut self) { self.inner = MemPanelSystem::new(); }

    /// [{ name, about }] of the built-in examples.
    pub fn examples(&self) -> JsValue {
        let v: Vec<MpExample> = memsys::examples::examples().iter().map(|e| MpExample { name: e.name, about: e.about }).collect();
        serde_wasm_bindgen::to_value(&v).unwrap_or(JsValue::NULL)
    }

    /// Reset and load example `i`.
    pub fn load_example(&mut self, i: usize) -> bool {
        let ex = memsys::examples::examples();
        let Some(e) = ex.get(i) else { return false };
        self.inner = MemPanelSystem::new();
        (e.setup)(&mut self.inner);
        true
    }

    /// Write words into physical memory at byte address `pa`.
    pub fn load(&mut self, pa: u32, words: &[u32]) { self.inner.load(pa, words); }

    /// One clock; returns this cycle's events.
    pub fn step(&mut self) -> JsValue {
        self.inner.step();
        self.events()
    }

    /// `n` clocks (events of the last one only).
    pub fn run(&mut self, n: u32) { for _ in 0..n { self.inner.step(); } }

    #[wasm_bindgen(getter)] pub fn cycle(&self) -> f64 { self.inner.cycle as f64 }

    /// Events of the last cycle: [{ kind, core, ... }].
    pub fn events(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.inner.mem.borrow().events).unwrap_or(JsValue::NULL)
    }

    /// Per core: pc, mode, CSRs, stall, the instruction in each stage, the ITLB and DTLB.
    pub fn cores(&self) -> JsValue {
        let m = self.inner.mem.borrow();
        let v: Vec<MpCore> = self.inner.cores.iter().enumerate().map(|(i, c)| {
            let st = |valid: bool, pc: u32, instr: u32, bubble: bool| MpStage { valid, pc, instr, bubble };
            MpCore {
                pc: c.pc, priv_mode: c.priv_bits(),
                satp: c.sys.csrs.read(0x180), mstatus: c.sys.csrs.read(0x300), mtvec: c.sys.csrs.read(0x305),
                mepc: c.sys.csrs.read(0x341), mcause: c.sys.csrs.read(0x342), mtval: c.sys.csrs.read(0x343),
                mhartid: c.sys.csrs.read(0xf14),
                stalled: m.stall[i] > 0, stall_left: m.stall[i],
                stages: vec![
                    st(c.if_id.valid, c.if_id.pc, c.if_id.instr, c.if_id.flush_bubble),
                    st(c.id_ex.valid, c.id_ex.pc, c.id_ex.instr, c.id_ex.flush_bubble || c.id_ex.stall_reg != 0),
                    st(c.ex_ma.valid, c.ex_ma.trap_pc, c.ex_ma.instr, c.ex_ma.flush_bubble || c.ex_ma.stall_reg != 0),
                    st(c.ma_wb.valid, c.ma_wb.trap_pc, c.ma_wb.instr, c.ma_wb.flush_bubble || c.ma_wb.stall_reg != 0),
                    st(c.last_wb.valid, c.last_wb.trap_pc, c.last_wb.instr, c.last_wb.flush_bubble || c.last_wb.stall_reg != 0),
                ],
                itlb: tlb(&c.itlb), dtlb: tlb(&c.dtlb), itlb_plru: c.itlb.plru, dtlb_plru: c.dtlb.plru,
            }
        }).collect();
        serde_wasm_bindgen::to_value(&v).unwrap_or(JsValue::NULL)
    }

    /// { l1i: [core0, core1], l1d: [core0, core1], l2 }, lines set-major.
    pub fn caches(&self) -> JsValue {
        let m = self.inner.mem.borrow();
        let v = MpCaches {
            l1i: m.l1i.iter().map(mp_cache).collect(),
            l1d: m.l1d.iter().map(mp_cache).collect(),
            l2: mp_cache(&m.l2),
        };
        serde_wasm_bindgen::to_value(&v).unwrap_or(JsValue::NULL)
    }

    /// Per-core counters.
    pub fn stats(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.inner.mem.borrow().stats).unwrap_or(JsValue::NULL)
    }

    /// `n` words from physical address `pa` as the system sees them (caches included).
    pub fn mem_words(&self, pa: u32, n: u32) -> Vec<u32> {
        (0..n).map(|i| self.inner.peek(pa.wrapping_add(i * 4))).collect()
    }

    /// Words straight from physical memory (no caches) — shows what is stale.
    pub fn mem_raw(&self, pa: u32, n: u32) -> Vec<u32> {
        let m = self.inner.mem.borrow();
        (0..n).map(|i| m.raw(pa.wrapping_add(i * 4))).collect()
    }

    #[wasm_bindgen(getter)] pub fn latency(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.inner.mem.borrow().lat).unwrap_or(JsValue::NULL)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Out-of-order core (Tomasulo + ROB) panel. All behaviour is in Rust
// (sim/ooo); the panel draws the snapshot and the events of each stage.
// ─────────────────────────────────────────────────────────────────────────

use crate::sim::ooo::{self, mem::MemMode, OooCpu};

#[derive(Serialize)]
struct OoRob { tag: u8, e: ooo::RobEntry }
#[derive(Serialize)]
struct OoFetch { pc: u32, instr: u32, ready: u64 }
#[derive(Serialize)]
struct OoSnap<'a> {
    cycle: u64,
    /// Next stage to run (0 = Commit of a new cycle)
    stage: usize,
    pc: u32,
    rf: [u32; 32],
    rat: [Option<u8>; 32],
    iq: Vec<ooo::IqEntry>,
    fetching: Vec<OoFetch>,
    rob: Vec<OoRob>,
    rob_head: usize,
    rob_len: usize,
    alu_rs: &'a [ooo::RsEntry],
    mul_rs: &'a [ooo::RsEntry],
    br_rs: &'a [ooo::RsEntry],
    lsq: Vec<ooo::LsqEntry>,
    exec: &'a [ooo::Exec],
    bht: Vec<u8>,
    btb: Vec<ooo::predictor::BtbEntry>,
    stats: ooo::OooStats,
    history: Vec<ooo::Retired>,
    halted: bool,
    serializing: bool,
    caches: bool,
}
#[derive(Serialize)]
struct OoEvents<'a> { events: &'a [ooo::OooEvent], stages: &'a [ooo::Stage] }

#[wasm_bindgen]
pub struct JsOoo { inner: OooCpu, example: usize }

impl Default for JsOoo {
    fn default() -> Self { Self::new(false) }
}

#[wasm_bindgen]
impl JsOoo {
    /// `caches`: memory through L1I / L1D → L2 → memory; else fixed 2 cycles.
    #[wasm_bindgen(constructor)]
    pub fn new(caches: bool) -> Self {
        Self { inner: OooCpu::new(if caches { MemMode::Caches } else { MemMode::Fixed }), example: 0 }
    }

    /// [{ name, about }] of the examples.
    pub fn examples(&self) -> JsValue {
        let v: Vec<MpExample> = ooo::examples::examples().iter().map(|e| MpExample { name: e.name, about: e.about }).collect();
        serde_wasm_bindgen::to_value(&v).unwrap_or(JsValue::NULL)
    }

    /// Fresh core with example `i` loaded (`caches` picks the memory).
    pub fn load_example(&mut self, i: usize, caches: bool) -> bool {
        let ex = ooo::examples::examples();
        let Some(e) = ex.get(i) else { return false };
        self.inner = OooCpu::new(if caches { MemMode::Caches } else { MemMode::Fixed });
        self.inner.load(0, &e.prog);
        for &(a, v) in &e.data { self.inner.load(a, &[v]); }
        self.example = i;
        true
    }

    /// Words of the loaded example's program (for the listing).
    pub fn program(&self) -> Vec<u32> {
        ooo::examples::examples().get(self.example).map(|e| e.prog.clone()).unwrap_or_default()
    }

    /// Run the next stage of the cycle; returns its index (0 Commit … 6 Fetch).
    pub fn step_stage(&mut self) -> usize { let st = self.inner.step_stage(); ooo::STAGES.iter().position(|s| *s == st).unwrap_or(0) }

    /// Finish the current cycle (a full clock from a cycle start).
    pub fn step(&mut self) { self.inner.step(); }

    /// Up to `n` cycles; stops at `j .`.
    pub fn run(&mut self, n: u32) -> bool { self.inner.run(n as u64) }

    #[wasm_bindgen(getter)] pub fn cycle(&self) -> f64 { self.inner.cycle as f64 }
    #[wasm_bindgen(getter)] pub fn halted(&self) -> bool { self.inner.halted }

    /// This cycle's events so far: { events: [...], stages: [...] } (same length).
    pub fn events(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&OoEvents { events: &self.inner.events, stages: &self.inner.event_stage }).unwrap_or(JsValue::NULL)
    }

    /// Everything the panel draws.
    pub fn snapshot(&self) -> JsValue {
        let c = &self.inner;
        let snap = OoSnap {
            cycle: c.cycle, stage: c.stage, pc: c.pc, rf: c.rf, rat: c.rat,
            iq: c.iq.iter().copied().collect(),
            fetching: c.fetching().into_iter().map(|(pc, instr, ready)| OoFetch { pc, instr, ready }).collect(),
            rob: c.rob.tags().map(|t| OoRob { tag: t, e: *c.rob.get(t) }).collect(),
            rob_head: c.rob.head, rob_len: c.rob.len,
            alu_rs: &c.alu_rs, mul_rs: &c.mul_rs, br_rs: &c.br_rs,
            lsq: c.lsq.iter().copied().collect(),
            exec: &c.exec,
            bht: c.pred.bht.to_vec(), btb: c.pred.btb.to_vec(),
            stats: c.stats, history: c.history.iter().copied().collect(),
            halted: c.halted, serializing: c.serializing,
            caches: c.mem.mode == MemMode::Caches,
        };
        serde_wasm_bindgen::to_value(&snap).unwrap_or(JsValue::NULL)
    }

    /// Memory word as the program sees it.
    pub fn peek(&self, pa: u32) -> u32 { self.inner.mem.peek(pa) }
}
