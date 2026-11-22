//! `SingleCycle`: the single-cycle core (CpuLayout and the micro-step view).

use serde::Serialize;
use wasm_bindgen::prelude::*;

use super::{core_api, js};
use crate::sim::core::Core;
use crate::sim::cpu_state::CpuState;
use crate::sim::phase::Phase;

#[wasm_bindgen]
pub struct SingleCycle {
    inner: CpuState,
}

core_api!(SingleCycle);

impl Default for SingleCycle {
    fn default() -> Self { Self::new() }
}

#[wasm_bindgen]
impl SingleCycle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self { inner: CpuState::new() } }

    /// Core configured like the single-cycle panel: 64-word ROM / RAM, traps off.
    pub fn panel() -> SingleCycle {
        let mut c = CpuState::with_sizes(64, 64);
        c.traps_enabled = false;
        SingleCycle { inner: c }
    }

    /// A fresh core of the default size (memory cleared).
    pub fn clear(&mut self) { self.inner = CpuState::new(); }

    /// Reset the core (registers, RAM, IO data, CSRs, traps) but keep the
    /// ROM, ROM / RAM sizes, I/O base and the traps option.
    pub fn reset_keep_rom(&mut self) {
        let rom = std::mem::take(&mut self.inner.mem.rom);
        let ram_words = self.inner.mem.ram.len();
        let (io_base, traps, zmmul) = (self.inner.mem.io_base, self.inner.traps_enabled, self.inner.zmmul);
        let extra: Vec<u32> = self.inner.mem.ram_extra.keys().copied().collect();
        self.inner = CpuState::with_sizes(rom.len(), ram_words);
        self.inner.mem.rom = rom;
        self.inner.mem.io_base = io_base;
        self.inner.traps_enabled = traps;
        self.inner.zmmul = zmmul;
        // Written-outside-window words stay listed (zeroed), like the panel's RAM view
        for a in extra { self.inner.mem.ram_extra.insert(a, 0); }
    }

    /// Replace the whole ROM (padded with NOPs to its size).
    pub fn set_rom(&mut self, words: &[u32]) {
        for (i, slot) in self.inner.mem.rom.iter_mut().enumerate() {
            *slot = words.get(i).copied().unwrap_or(0x0000_0013);
        }
    }

    #[wasm_bindgen(getter)] pub fn traps_enabled(&self) -> bool { self.inner.traps_enabled }
    pub fn set_traps_enabled(&mut self, on: bool) { self.inner.traps_enabled = on; }
    #[wasm_bindgen(getter)] pub fn zmmul(&self) -> bool { self.inner.zmmul }
    pub fn set_zmmul(&mut self, on: bool) { self.inner.zmmul = on; }
    #[wasm_bindgen(getter)] pub fn io_data(&self) -> u32 { self.inner.mem.io_data }
    pub fn set_io_data(&mut self, v: u32) { self.inner.mem.io_data = v; }
    #[wasm_bindgen(getter)] pub fn io_base(&self) -> u32 { self.inner.mem.io_base }
    pub fn set_io_base(&mut self, base: u32) { self.inner.mem.io_base = base; }
    #[wasm_bindgen(getter)] pub fn trap_depth(&self) -> u32 { self.inner.sys.trap_depth }

    /// RAM words written outside the RAM window, flat [addr, value, …].
    pub fn ram_extra(&self) -> Vec<u32> {
        self.inner.ram_extra_snapshot().into_iter().flat_map(|(a, v)| [a, v]).collect()
    }

    /// Whole core state as a plain object (`JSON.stringify` it for storage);
    /// restore with `restore`.
    #[wasm_bindgen(unchecked_return_type = "CpuState")]
    pub fn save(&self) -> JsValue {
        let ser = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true).serialize_maps_as_objects(true);
        self.inner.serialize(&ser).unwrap_or(JsValue::NULL)
    }

    /// Restore a `save` object (saves from before `mem` existed are migrated
    /// by the store). Returns false if it does not fit.
    pub fn restore(&mut self, #[wasm_bindgen(unchecked_param_type = "CpuState")] state: JsValue) -> bool {
        match serde_wasm_bindgen::from_value::<CpuState>(state) {
            Ok(c) => { self.inner = c; true }
            Err(_) => false,
        }
    }

    // ── Micro-phases ────────────────────────────────────────────────────

    /// Advance one phase (same as `step`). Returns the phase name.
    pub fn micro_step(&mut self) -> String { phase_name(self.inner.micro_step()).to_string() }

    /// Every MicroState field (bus / control values and decoded fields).
    #[wasm_bindgen(unchecked_return_type = "MicroState")]
    pub fn micro_raw(&self) -> JsValue { js(&self.inner.micro) }

    /// The current MicroState as named buses, controls and components.
    #[wasm_bindgen(unchecked_return_type = "MicroJson")]
    pub fn micro_state(&self) -> JsValue {
        let m = &self.inner.micro;
        js(&MicroJson {
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
        })
    }
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
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
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct BusEntry { name: &'static str, value: Option<u32> }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct CtrlEntry { name: &'static str, value: Option<String> }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct CompEntry { name: &'static str, value: Option<String> }

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
