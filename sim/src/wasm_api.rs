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
