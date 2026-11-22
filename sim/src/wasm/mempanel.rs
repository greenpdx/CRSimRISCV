//! `MemPanel`: two pipeline cores on the shared cache hierarchy (memsys,
//! MemoryView). Not a single core, so it has its own methods; `mem` and
//! `cycle` mean the same as on the core classes. The panel only draws what
//! these return; all behaviour is in Rust.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use super::js;
use crate::sim::memsys::{self, system::MemPanelSystem, Mesi};

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct MpStage { valid: bool, pc: u32, instr: u32, bubble: bool }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct MpTlb { valid: bool, vpn: u32, ppn: u32, pte: u32, is_super: bool, asid: u16, global: bool }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
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
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct MpLine { state: Mesi, tag: u32, lru: u8, addr: u32, data: [u32; 4] }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct MpCache { sets: usize, ways: usize, lines: Vec<MpLine> }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct MpCaches { l1i: Vec<MpCache>, l1d: Vec<MpCache>, l2: MpCache }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
pub(crate) struct MpExample { pub name: &'static str, pub about: &'static str }

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
pub struct MemPanel { inner: MemPanelSystem }

impl Default for MemPanel {
    fn default() -> Self { Self::new() }
}

#[wasm_bindgen]
impl MemPanel {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self { inner: MemPanelSystem::new() } }

    /// Fresh machine (empty memory, cold caches / TLBs).
    pub fn reset(&mut self) { self.inner = MemPanelSystem::new(); }

    /// [{ name, about }] of the built-in examples.
    #[wasm_bindgen(unchecked_return_type = "MpExample[]")]
    pub fn examples(&self) -> JsValue {
        let v: Vec<MpExample> = memsys::examples::examples().iter().map(|e| MpExample { name: e.name, about: e.about }).collect();
        js(&v)
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
    #[wasm_bindgen(unchecked_return_type = "Event[]")]
    pub fn step(&mut self) -> JsValue {
        self.inner.step();
        self.events()
    }

    /// `n` clocks (events of the last one only).
    pub fn run(&mut self, n: u32) { for _ in 0..n { self.inner.step(); } }

    #[wasm_bindgen(getter)] pub fn cycle(&self) -> f64 { self.inner.cycle as f64 }

    /// Events of the last cycle: [{ kind, core, ... }].
    #[wasm_bindgen(unchecked_return_type = "Event[]")]
    pub fn events(&self) -> JsValue {
        js(&self.inner.mem.borrow().events)
    }

    /// Per core: pc, mode, CSRs, stall, the instruction in each stage, the ITLB and DTLB.
    #[wasm_bindgen(unchecked_return_type = "MpCore[]")]
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
                    st(c.ex_ma.valid, c.ex_ma.pc, c.ex_ma.instr, c.ex_ma.flush_bubble || c.ex_ma.stall_reg != 0),
                    st(c.ma_wb.valid, c.ma_wb.pc, c.ma_wb.instr, c.ma_wb.flush_bubble || c.ma_wb.stall_reg != 0),
                    st(c.last_wb.valid, c.last_wb.pc, c.last_wb.instr, c.last_wb.flush_bubble || c.last_wb.stall_reg != 0),
                ],
                itlb: tlb(&c.itlb), dtlb: tlb(&c.dtlb), itlb_plru: c.itlb.plru, dtlb_plru: c.dtlb.plru,
            }
        }).collect();
        js(&v)
    }

    /// { l1i: [core0, core1], l1d: [core0, core1], l2 }, lines set-major.
    #[wasm_bindgen(unchecked_return_type = "MpCaches")]
    pub fn caches(&self) -> JsValue {
        let m = self.inner.mem.borrow();
        let v = MpCaches {
            l1i: m.l1i.iter().map(mp_cache).collect(),
            l1d: m.l1d.iter().map(mp_cache).collect(),
            l2: mp_cache(&m.l2),
        };
        js(&v)
    }

    /// Per-core counters.
    #[wasm_bindgen(unchecked_return_type = "Stats[]")]
    pub fn stats(&self) -> JsValue {
        js(&self.inner.mem.borrow().stats)
    }

    /// `n` words from physical address `pa` as the system sees them (caches included).
    pub fn mem(&self, pa: u32, n: u32) -> Vec<u32> {
        (0..n).map(|i| self.inner.peek(pa.wrapping_add(i * 4))).collect()
    }

    /// Words straight from physical memory (no caches) — shows what is stale.
    pub fn mem_raw(&self, pa: u32, n: u32) -> Vec<u32> {
        let m = self.inner.mem.borrow();
        (0..n).map(|i| m.raw(pa.wrapping_add(i * 4))).collect()
    }

    #[wasm_bindgen(getter, unchecked_return_type = "Latency")] pub fn latency(&self) -> JsValue {
        js(&self.inner.mem.borrow().lat)
    }
}

