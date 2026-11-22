//! `Ooo`: the out-of-order core (Tomasulo + ROB; OooView). Its smallest
//! `step` is one stage of a clock.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use super::mempanel::MpExample;
use super::{core_api, js};
use crate::sim::core::Core;
use crate::sim::ooo::{self, mem::MemMode, OooCpu};

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct OoRob { tag: u8, e: ooo::RobEntry }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct OoFetch { pc: u32, instr: u32, ready: u64 }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
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
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct OoEvents<'a> { events: &'a [ooo::OooEvent], stages: &'a [ooo::Stage] }

#[wasm_bindgen]
pub struct Ooo { inner: OooCpu, example: usize }

core_api!(Ooo);

impl Default for Ooo {
    fn default() -> Self { Self::new(false) }
}

#[wasm_bindgen]
impl Ooo {
    /// `caches`: memory through L1I / L1D → L2 → memory; else fixed 2 cycles.
    #[wasm_bindgen(constructor)]
    pub fn new(caches: bool) -> Self {
        Self { inner: OooCpu::new(if caches { MemMode::Caches } else { MemMode::Fixed }), example: 0 }
    }

    /// [{ name, about }] of the examples.
    #[wasm_bindgen(unchecked_return_type = "MpExample[]")]
    pub fn examples(&self) -> JsValue {
        let v: Vec<MpExample> = ooo::examples::examples().iter().map(|e| MpExample { name: e.name, about: e.about }).collect();
        js(&v)
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

    /// Run the next stage of the cycle (as `step`); returns its index (0 Commit … 6 Fetch).
    pub fn step_stage(&mut self) -> usize { let st = self.inner.step_stage(); ooo::STAGES.iter().position(|s| *s == st).unwrap_or(0) }

    /// Finish the current cycle (a full clock from a cycle start).
    pub fn step_cycle(&mut self) { self.inner.step(); }

    /// Up to `n` cycles; stops at `j .`. True if it halted.
    pub fn run_cycles(&mut self, n: u32) -> bool { self.inner.run(n as u64) }

    /// This cycle's events so far: { events: [...], stages: [...] } (same length).
    #[wasm_bindgen(unchecked_return_type = "OoEvents")]
    pub fn events(&self) -> JsValue {
        js(&OoEvents { events: &self.inner.events, stages: &self.inner.event_stage })
    }

    /// Everything the panel draws.
    #[wasm_bindgen(unchecked_return_type = "OoSnap")]
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
        js(&snap)
    }

}
