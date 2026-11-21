//! Out-of-order core: Tomasulo + reorder buffer, single issue, RV32I + Zmmul.
//!
//! One instruction is fetched, dispatched and committed per cycle; in
//! between, instructions wait in reservation stations, execute and finish
//! in any order. See sim/docs/ooo/ooo-components.svg and ooo-flow.svg.
//!
//! - Instruction queue 8 · ROB 32 · RS: ALU 4, MUL 4, branch 8 · LSQ 4
//! - 2 ALUs (1 cycle), a pipelined multiplier (3 cycles), a branch unit
//!   (1 cycle), a load / store unit (address 1 cycle + memory)
//! - 2 common data buses: up to 2 results per cycle, oldest first
//! - BHT (2-bit) + BTB at fetch; a mispredict is flushed when the branch
//!   executes; exceptions are taken at commit (precise)
//! - CSR / ECALL / EBREAK / MRET / FENCE wait for an empty ROB and run at commit
//!
//! One cycle runs: commit → resolve branches → load / store memory →
//! issue → writeback (CDB) → dispatch → fetch. So a waiting instruction
//! issues the cycle after its operand is broadcast, and an instruction
//! commits the cycle after its writeback.

pub mod decode;
pub mod examples;
pub mod mem;
pub mod predictor;

use std::collections::VecDeque;

use crate::sim::system::SysUnit;
use decode::{alu, branch_taken, decode, mul, Decoded, Op, Unit};
use mem::{extract, merge, MemMode, OooMem};
use predictor::{Prediction, Predictor};

pub const IQ_SIZE: usize = 8;
pub const ROB_SIZE: usize = 32;
pub const ALU_RS_SIZE: usize = 4;
pub const MUL_RS_SIZE: usize = 4;
pub const BR_RS_SIZE: usize = 8;
pub const LSQ_SIZE: usize = 4;
pub const N_CDB: usize = 2;
pub const N_ALU: usize = 2;
pub const MUL_LATENCY: u64 = 3;
/// Fetch requests in flight (2 keeps one fetch per cycle with 2-cycle memory)
pub const FETCH_IN_FLIGHT: usize = 2;
/// Instructions kept in the timeline (committed or squashed)
pub const HISTORY: usize = 64;

/// A source operand: a value, or waiting for ROB entry `tag` to broadcast.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Operand { pub value: u32, pub tag: Option<u8> }

impl Operand {
    pub fn val(value: u32) -> Self { Self { value, tag: None } }
    pub fn ready(&self) -> bool { self.tag.is_none() }
}

/// When an instruction passed each stage (cycle numbers).
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Times {
    pub fetch: u64,
    pub dispatch: Option<u64>,
    pub issue: Option<u64>,
    /// Writeback (CDB broadcast), or done for stores / branches without a result
    pub complete: Option<u64>,
    pub commit: Option<u64>,
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct IqEntry { pub seq: u64, pub pc: u32, pub instr: u32, pub pred: Prediction, pub fetched: u64 }

#[derive(Debug, Clone, Copy)]
struct FetchReq { seq: u64, pc: u32, instr: u32, pred: Prediction, fetched: u64, ready: u64 }

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct RobEntry {
    pub seq: u64,
    pub pc: u32,
    pub instr: u32,
    pub d: Decoded,
    pub value: u32,
    pub done: bool,
    /// (cause, tval) to trap with at commit
    pub exc: Option<(u32, u32)>,
    pub pred: Prediction,
    /// Stores: (address, data, size) once both are known
    pub store: Option<(u32, u32, u8)>,
    /// CSR instructions: rs1's value (read at dispatch; the ROB is empty then)
    pub src1: u32,
    pub t: Times,
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct RsEntry { pub seq: u64, pub rob: u8, pub pc: u32, pub d: Decoded, pub j: Operand, pub k: Operand, pub pred: Prediction }

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LsqState {
    /// Waiting for its base register, then the address
    Address,
    /// Address known: a load waits for older stores, a store for its data
    Ready,
    /// Load: reading memory or forwarded
    Memory,
    /// Store: address and data known, waits for commit
    Done,
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct LsqEntry {
    pub seq: u64,
    pub rob: u8,
    pub store: bool,
    pub size: u8,
    pub signed: bool,
    pub base: Operand,
    pub data: Operand,
    pub imm: u32,
    pub addr: Option<u32>,
    /// Cycle the address is known (after the 1-cycle address unit)
    pub addr_at: u64,
    pub state: LsqState,
}

/// An instruction executing (or done, waiting for a CDB).
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct Exec {
    pub seq: u64,
    pub rob: u8,
    pub unit: Unit,
    /// ALU 0 / 1; 0 for the others
    pub fu: u8,
    pub issued: u64,
    /// First cycle the result can go on a CDB
    pub finish: u64,
    pub value: u32,
    /// Branch unit: resolved actual next PC, taken, and whether it has a result (link)
    pub br: Option<BranchOut>,
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchOut { pub taken: bool, pub next: u32, pub resolved: bool }

/// The stages of one cycle, in the order they run.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage { Commit, Resolve, Memory, Issue, Writeback, Dispatch, Fetch }

pub const STAGES: [Stage; 7] = [Stage::Commit, Stage::Resolve, Stage::Memory, Stage::Issue, Stage::Writeback, Stage::Dispatch, Stage::Fetch];

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallReason { RobFull, RsFull, LsqFull, Serialize }

/// What happened this cycle, for the panel.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[cfg_attr(feature = "wasm", serde(tag = "kind"))]
#[derive(Debug, Clone, PartialEq)]
pub enum OooEvent {
    Fetch { seq: u64, pc: u32, instr: u32, taken: bool, next: u32, btb_hit: bool, latency: u64 },
    Dispatch { seq: u64, rob: u8, pc: u32, unit: Unit, j_tag: Option<u8>, k_tag: Option<u8> },
    Stall { reason: StallReason, unit: Unit },
    Issue { seq: u64, rob: u8, unit: Unit, fu: u8 },
    /// Load / store address computed
    Address { seq: u64, rob: u8, addr: u32 },
    /// Load data taken from an older store in the LSQ
    Forward { seq: u64, rob: u8, from_rob: u8, addr: u32, value: u32 },
    MemRead { seq: u64, rob: u8, addr: u32, latency: u64 },
    StoreReady { seq: u64, rob: u8, addr: u32, data: u32 },
    /// Branch unit result: mispredict = predicted next PC was wrong
    Resolve { seq: u64, rob: u8, pc: u32, taken: bool, next: u32, mispredict: bool },
    Broadcast { bus: u8, seq: u64, rob: u8, value: u32 },
    /// A result finished but both CDBs were taken by older ones
    CdbWait { seq: u64, rob: u8 },
    /// RS / LSQ entry of ROB `rob` took the value of ROB `tag`
    Wakeup { rob: u8, tag: u8 },
    /// Everything younger than `after_seq` thrown away; fetch goes on at `pc`
    Flush { after_seq: u64, squashed: u32, pc: u32 },
    Commit { seq: u64, rob: u8, pc: u32, rd: u8, value: u32 },
    StoreCommit { seq: u64, addr: u32, value: u32, size: u8 },
    Trap { seq: u64, pc: u32, cause: u32, handler: u32 },
    Mret { seq: u64, target: u32 },
    Halt { pc: u32 },
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, Default)]
pub struct OooStats {
    pub cycles: u64,
    pub committed: u64,
    pub branches: u64,
    pub mispredicts: u64,
    pub squashed: u64,
    pub cdb_waits: u64,
    pub stall_rob: u64,
    pub stall_rs: u64,
    pub stall_serialize: u64,
    pub forwards: u64,
    pub loads: u64,
    pub stores: u64,
    pub traps: u64,
}

/// An instruction that left the machine: committed, or squashed by a flush.
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct Retired { pub seq: u64, pub pc: u32, pub instr: u32, pub t: Times, pub squashed: bool }

/// The reorder buffer: a circular array; an entry's index is its tag.
#[derive(Debug, Clone)]
pub struct Rob { pub slots: Vec<Option<RobEntry>>, pub head: usize, pub len: usize }

impl Rob {
    fn new() -> Self { Self { slots: vec![None; ROB_SIZE], head: 0, len: 0 } }
    pub fn full(&self) -> bool { self.len == ROB_SIZE }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn tail(&self) -> usize { (self.head + self.len) % ROB_SIZE }
    pub fn get(&self, tag: u8) -> &RobEntry { self.slots[tag as usize].as_ref().expect("ROB tag in use") }
    fn get_mut(&mut self, tag: u8) -> &mut RobEntry { self.slots[tag as usize].as_mut().expect("ROB tag in use") }
    fn push(&mut self, e: RobEntry) -> u8 {
        let t = self.tail();
        self.slots[t] = Some(e);
        self.len += 1;
        t as u8
    }
    fn pop_head(&mut self) -> RobEntry {
        let e = self.slots[self.head].take().expect("ROB head");
        self.head = (self.head + 1) % ROB_SIZE;
        self.len -= 1;
        e
    }
    fn pop_tail(&mut self) -> RobEntry {
        let t = (self.head + self.len - 1) % ROB_SIZE;
        self.len -= 1;
        self.slots[t].take().expect("ROB tail")
    }
    /// Tags from oldest to youngest.
    pub fn tags(&self) -> impl Iterator<Item = u8> + '_ { (0..self.len).map(move |i| ((self.head + i) % ROB_SIZE) as u8) }
    pub fn head_entry(&self) -> Option<&RobEntry> { if self.len == 0 { None } else { self.slots[self.head].as_ref() } }
}

pub struct OooCpu {
    pub cycle: u64,
    /// Next fetch PC
    pub pc: u32,
    /// Architectural registers (written at commit only)
    pub rf: [u32; 32],
    /// Register alias table: the ROB entry that will write each register (None = the RF has it)
    pub rat: [Option<u8>; 32],
    pub iq: VecDeque<IqEntry>,
    fetchq: VecDeque<FetchReq>,
    pub rob: Rob,
    pub alu_rs: Vec<RsEntry>,
    pub mul_rs: Vec<RsEntry>,
    pub br_rs: Vec<RsEntry>,
    pub lsq: VecDeque<LsqEntry>,
    pub exec: Vec<Exec>,
    pub pred: Predictor,
    pub mem: OooMem,
    pub sys: SysUnit,
    next_seq: u64,
    /// A CSR / system instruction is in the ROB: no dispatch until it commits
    pub serializing: bool,
    /// Committed `j .`
    pub halted: bool,
    pub events: Vec<OooEvent>,
    /// The stage each event of `events` came from
    pub event_stage: Vec<Stage>,
    /// Next stage to run (0 = a new cycle starts)
    pub stage: usize,
    pub stats: OooStats,
    pub history: VecDeque<Retired>,
}

impl OooCpu {
    pub fn new(mode: MemMode) -> Self {
        Self {
            cycle: 0, pc: 0, rf: [0; 32], rat: [None; 32],
            iq: VecDeque::new(), fetchq: VecDeque::new(), rob: Rob::new(),
            alu_rs: Vec::new(), mul_rs: Vec::new(), br_rs: Vec::new(), lsq: VecDeque::new(), exec: Vec::new(),
            pred: Predictor::default(), mem: OooMem::new(mode), sys: SysUnit::new(),
            next_seq: 0, serializing: false, halted: false,
            events: Vec::new(), event_stage: Vec::new(), stage: 0, stats: OooStats::default(), history: VecDeque::new(),
        }
    }

    /// Load words into memory at byte address `pa`.
    pub fn load(&mut self, pa: u32, words: &[u32]) { self.mem.load_words(pa, words); }

    /// Run until `j .` commits or `max` cycles. True if halted.
    pub fn run(&mut self, max: u64) -> bool {
        for _ in 0..max { if self.halted { return true; } self.step(); }
        self.halted
    }

    /// Finish the current cycle (one clock when at its start).
    pub fn step(&mut self) {
        loop { self.step_stage(); if self.stage == 0 { break; } }
    }

    /// Run the next stage of the current cycle; returns it.
    pub fn step_stage(&mut self) -> Stage {
        if self.stage == 0 {
            self.events.clear();
            self.event_stage.clear();
            self.mem.begin_cycle(self.cycle);
        }
        let st = STAGES[self.stage];
        match st {
            Stage::Commit => self.commit(),
            Stage::Resolve => self.resolve(),
            Stage::Memory => self.lsu(),
            Stage::Issue => self.issue(),
            Stage::Writeback => self.writeback(),
            Stage::Dispatch => self.dispatch(),
            Stage::Fetch => self.fetch(),
        }
        self.stage += 1;
        if self.stage == STAGES.len() {
            self.stage = 0;
            self.cycle += 1;
            self.stats.cycles = self.cycle;
        }
        st
    }

    /// Fetches in flight: (pc, instruction, cycle it arrives in the IQ).
    pub fn fetching(&self) -> Vec<(u32, u32, u64)> { self.fetchq.iter().map(|f| (f.pc, f.instr, f.ready)).collect() }

    fn ev(&mut self, e: OooEvent) {
        self.events.push(e);
        self.event_stage.push(STAGES[self.stage]);
    }

    fn retire(&mut self, e: &RobEntry, squashed: bool) {
        self.history.push_back(Retired { seq: e.seq, pc: e.pc, instr: e.instr, t: e.t, squashed });
        while self.history.len() > HISTORY { self.history.pop_front(); }
    }

    // ── commit: the ROB head, in program order ─────────────────────────
    fn commit(&mut self) {
        let Some(h) = self.rob.head_entry() else { return };
        if !h.done { return; }
        let tag = self.rob.head as u8;
        let mut e = self.rob.pop_head();
        let now = self.cycle;
        e.t.commit = Some(now);
        // exceptions are taken here, in order: precise
        let trap = match e.d.op {
            _ if e.exc.is_some() => e.exc,
            Op::Ecall => Some((self.sys.ecall_cause(), 0)),
            Op::Ebreak => Some((3, e.pc)),
            _ => None,
        };
        if let Some((cause, tval)) = trap {
            let handler = self.sys.trap(cause, false, e.pc, tval, now);
            self.stats.traps += 1;
            self.ev(OooEvent::Trap { seq: e.seq, pc: e.pc, cause, handler });
            self.retire(&e, true);
            self.flush_all(handler);
            return;
        }
        let mut value = e.value;
        match e.d.op {
            Op::Store { size } => {
                let (addr, data, _) = e.store.expect("store done");
                self.mem.store(addr, data, size);
                self.lsq.retain(|l| l.seq != e.seq);
                self.stats.stores += 1;
                self.ev(OooEvent::StoreCommit { seq: e.seq, addr, value: data, size });
            }
            Op::Csr { f3, csr } => {
                let zimm = e.d.imm;
                let (src, marker) = if f3 >= 5 { (zimm, zimm as u8) } else { (e.src1, e.d.rs1) };
                value = self.sys.csr(csr, f3, marker, src).old;
            }
            Op::Mret => {
                let target = self.sys.xret(false);
                self.ev(OooEvent::Mret { seq: e.seq, target });
                self.retire(&e, false);
                self.stats.committed += 1;
                self.flush_all(target);
                return;
            }
            _ => {}
        }
        if e.d.unit == Unit::Sys { self.serializing = false; }
        if e.d.rd != 0 {
            self.rf[e.d.rd as usize] = value;
            if self.rat[e.d.rd as usize] == Some(tag) { self.rat[e.d.rd as usize] = None; }
        }
        self.stats.committed += 1;
        self.ev(OooEvent::Commit { seq: e.seq, rob: tag, pc: e.pc, rd: e.d.rd, value });
        self.retire(&e, false);
        if e.instr == 0x0000_006f {
            self.halted = true;
            self.ev(OooEvent::Halt { pc: e.pc });
        }
    }

    // ── branch unit results: train the predictor, flush on a mispredict ──
    fn resolve(&mut self) {
        let now = self.cycle;
        let Some(i) = self.exec.iter().position(|x| x.unit == Unit::Br && x.finish <= now && !x.br.unwrap().resolved) else { return };
        let x = self.exec[i];
        let br = x.br.unwrap();
        let e = *self.rob.get(x.rob);
        let jump = matches!(e.d.op, Op::Jal | Op::Jalr);
        let target = if br.taken { br.next } else { e.pc.wrapping_add(4) };
        self.pred.update(e.pc, jump, br.taken, target);
        let mispredict = e.pred.next != br.next;
        self.stats.branches += 1;
        self.ev(OooEvent::Resolve { seq: x.seq, rob: x.rob, pc: e.pc, taken: br.taken, next: br.next, mispredict });
        if e.d.rd == 0 {
            // no result to broadcast: done now
            self.exec.remove(i);
            let r = self.rob.get_mut(x.rob);
            r.done = true;
            r.t.complete = Some(now);
        } else {
            self.exec[i].br = Some(BranchOut { resolved: true, ..br });
        }
        if mispredict {
            self.stats.mispredicts += 1;
            self.flush_younger(x.seq, br.next);
        }
    }

    // ── load / store queue: stores ready, one load to memory per cycle ──
    fn lsu(&mut self) {
        let now = self.cycle;
        for i in 0..self.lsq.len() {
            let l = self.lsq[i];
            if l.store && l.state == LsqState::Ready && l.addr_at <= now && l.data.ready() {
                let addr = l.addr.unwrap();
                self.lsq[i].state = LsqState::Done;
                let r = self.rob.get_mut(l.rob);
                r.store = Some((addr, l.data.value, l.size));
                r.done = true;
                r.t.complete = Some(now);
                self.ev(OooEvent::StoreReady { seq: l.seq, rob: l.rob, addr, data: l.data.value });
            }
        }
        for i in 0..self.lsq.len() {
            let l = self.lsq[i];
            if l.store || l.state != LsqState::Ready || l.addr_at > now { continue; }
            let addr = l.addr.unwrap();
            // every older store's address must be known
            let older: Vec<LsqEntry> = self.lsq.iter().take(i).filter(|s| s.store).copied().collect();
            if older.iter().any(|s| s.addr.is_none() || s.addr_at > now) { continue; }
            // the youngest older store to the same word decides
            if let Some(s) = older.iter().rev().find(|s| s.addr.unwrap() & !3 == addr & !3) {
                let sa = s.addr.unwrap();
                if sa == addr && s.size == l.size && s.data.ready() {
                    let value = extract(merge(0, sa, s.data.value, s.size), addr, l.size, l.signed);
                    self.start_load(i, value, 1);
                    self.stats.forwards += 1;
                    self.ev(OooEvent::Forward { seq: l.seq, rob: l.rob, from_rob: s.rob, addr, value });
                    return;
                }
                continue;   // partial overlap or data not ready: wait for that store
            }
            let (value, latency) = self.mem.load(addr, l.size, l.signed);
            self.start_load(i, value, latency);
            self.ev(OooEvent::MemRead { seq: l.seq, rob: l.rob, addr, latency });
            return;
        }
    }

    fn start_load(&mut self, i: usize, value: u32, latency: u64) {
        let l = &mut self.lsq[i];
        l.state = LsqState::Memory;
        let (seq, rob) = (l.seq, l.rob);
        self.stats.loads += 1;
        self.exec.push(Exec { seq, rob, unit: Unit::Lsu, fu: 0, issued: self.cycle, finish: self.cycle + latency, value, br: None });
    }

    // ── issue: each unit takes its oldest ready entry ───────────────────
    fn issue(&mut self) {
        let now = self.cycle;
        // ALU ×2
        for fu in 0..N_ALU as u8 {
            let Some(i) = oldest_ready(&self.alu_rs) else { break };
            let r = self.alu_rs.remove(i);
            let Op::Alu { op, .. } = r.d.op else { unreachable!() };
            let value = alu(op, r.j.value, r.k.value, r.pc);
            self.start(r, Unit::Alu, fu, now + 1, value, None);
        }
        // MUL (pipelined: one new per cycle)
        if let Some(i) = oldest_ready(&self.mul_rs) {
            let r = self.mul_rs.remove(i);
            let Op::Mul { f3 } = r.d.op else { unreachable!() };
            self.start(r, Unit::Mul, 0, now + MUL_LATENCY, mul(f3, r.j.value, r.k.value), None);
        }
        // branch unit
        if let Some(i) = oldest_ready(&self.br_rs) {
            let r = self.br_rs.remove(i);
            let (taken, next) = match r.d.op {
                Op::Jal => (true, r.pc.wrapping_add(r.d.imm)),
                Op::Jalr => (true, r.j.value.wrapping_add(r.d.imm) & !1),
                Op::Branch { f3 } => {
                    let t = branch_taken(f3, r.j.value, r.k.value);
                    (t, if t { r.pc.wrapping_add(r.d.imm) } else { r.pc.wrapping_add(4) })
                }
                _ => unreachable!(),
            };
            let link = r.pc.wrapping_add(4);
            self.start(r, Unit::Br, 0, now + 1, link, Some(BranchOut { taken, next, resolved: false }));
        }
        // load / store address unit: oldest entry whose base is ready
        if let Some(i) = self.lsq.iter().position(|l| l.state == LsqState::Address && l.base.ready()) {
            let l = &mut self.lsq[i];
            let addr = l.base.value.wrapping_add(l.imm);
            l.addr = Some(addr);
            l.addr_at = now + 1;
            l.state = LsqState::Ready;
            let (seq, rob) = (l.seq, l.rob);
            self.rob.get_mut(rob).t.issue = Some(now);
            self.ev(OooEvent::Issue { seq, rob, unit: Unit::Lsu, fu: 0 });
            self.ev(OooEvent::Address { seq, rob, addr });
        }
    }

    fn start(&mut self, r: RsEntry, unit: Unit, fu: u8, finish: u64, value: u32, br: Option<BranchOut>) {
        self.rob.get_mut(r.rob).t.issue = Some(self.cycle);
        self.exec.push(Exec { seq: r.seq, rob: r.rob, unit, fu, issued: self.cycle, finish, value, br });
        self.ev(OooEvent::Issue { seq: r.seq, rob: r.rob, unit, fu });
    }

    // ── writeback: up to N_CDB finished results, oldest first ───────────
    fn writeback(&mut self) {
        let now = self.cycle;
        let mut ready: Vec<usize> = (0..self.exec.len())
            .filter(|&i| { let x = &self.exec[i]; x.finish <= now && x.br.is_none_or(|b| b.resolved) })
            .collect();
        ready.sort_by_key(|&i| self.exec[i].seq);
        let (go, wait) = ready.split_at(ready.len().min(N_CDB));
        let go: Vec<Exec> = go.iter().map(|&i| self.exec[i]).collect();
        for &i in wait {
            let x = self.exec[i];
            self.stats.cdb_waits += 1;
            self.ev(OooEvent::CdbWait { seq: x.seq, rob: x.rob });
        }
        for (bus, x) in go.iter().enumerate() {
            self.exec.retain(|e| e.seq != x.seq);
            let r = self.rob.get_mut(x.rob);
            r.value = x.value;
            r.done = true;
            r.t.complete = Some(now);
            if x.unit == Unit::Lsu { self.lsq.retain(|l| l.seq != x.seq); }
            self.ev(OooEvent::Broadcast { bus: bus as u8, seq: x.seq, rob: x.rob, value: x.value });
            self.wake(x.rob, x.value);
        }
    }

    /// Every waiting operand tagged `tag` takes `value`.
    fn wake(&mut self, tag: u8, value: u32) {
        let mut woke = Vec::new();
        for rs in [&mut self.alu_rs, &mut self.mul_rs, &mut self.br_rs] {
            for r in rs.iter_mut() {
                for o in [&mut r.j, &mut r.k] {
                    if o.tag == Some(tag) { *o = Operand::val(value); woke.push(r.rob); }
                }
            }
        }
        for l in self.lsq.iter_mut() {
            for o in [&mut l.base, &mut l.data] {
                if o.tag == Some(tag) { *o = Operand::val(value); woke.push(l.rob); }
            }
        }
        for rob in woke { self.ev(OooEvent::Wakeup { rob, tag }); }
    }

    // ── dispatch: rename and send to a reservation station, in order ────
    fn dispatch(&mut self) {
        let now = self.cycle;
        // fetched instructions arrive in the instruction queue, in order
        while let Some(f) = self.fetchq.front() {
            if f.ready > now || self.iq.len() >= IQ_SIZE { break; }
            let f = self.fetchq.pop_front().unwrap();
            self.iq.push_back(IqEntry { seq: f.seq, pc: f.pc, instr: f.instr, pred: f.pred, fetched: f.fetched });
        }
        let Some(&q) = self.iq.front() else { return };
        let d = decode(q.instr);
        let stall = |s: &mut Self, reason| { match reason {
            StallReason::RobFull => s.stats.stall_rob += 1,
            StallReason::Serialize => s.stats.stall_serialize += 1,
            _ => s.stats.stall_rs += 1,
        } s.ev(OooEvent::Stall { reason, unit: d.unit }); };
        // an illegal instruction just traps if it reaches commit: no need to drain the ROB
        if self.serializing || (d.unit == Unit::Sys && d.op != Op::Illegal && !self.rob.is_empty()) { stall(self, StallReason::Serialize); return; }
        if self.rob.full() { stall(self, StallReason::RobFull); return; }
        let room = match d.unit {
            Unit::Alu => self.alu_rs.len() < ALU_RS_SIZE,
            Unit::Mul => self.mul_rs.len() < MUL_RS_SIZE,
            Unit::Br => self.br_rs.len() < BR_RS_SIZE,
            Unit::Lsu => self.lsq.len() < LSQ_SIZE,
            Unit::Sys => true,
        };
        if !room { stall(self, if d.unit == Unit::Lsu { StallReason::LsqFull } else { StallReason::RsFull }); return; }
        self.iq.pop_front();

        let read = |s: &Self, r: u8| -> Operand {
            if r == 0 { return Operand::val(0); }
            match s.rat[r as usize] {
                None => Operand::val(s.rf[r as usize]),
                Some(t) => { let e = s.rob.get(t); if e.done { Operand::val(e.value) } else { Operand { value: 0, tag: Some(t) } } }
            }
        };
        let j = read(self, d.rs1);
        let k = match d.op { Op::Alu { imm: true, .. } => Operand::val(d.imm), _ => read(self, d.rs2) };
        let mut e = RobEntry {
            seq: q.seq, pc: q.pc, instr: q.instr, d, value: 0, done: false, exc: None, pred: q.pred,
            store: None, src1: 0, t: Times { fetch: q.fetched, dispatch: Some(now), ..Times::default() },
        };
        if d.unit == Unit::Sys {
            // runs at commit; the ROB is empty, so rs1 comes straight from the RF
            e.done = true;
            e.t.complete = Some(now);
            e.src1 = self.rf[d.rs1 as usize];
            if d.op == Op::Illegal { e.exc = Some((2, q.instr)); } else { self.serializing = true; }
        }
        let tag = self.rob.push(e);
        let rs = RsEntry { seq: q.seq, rob: tag, pc: q.pc, d, j, k, pred: q.pred };
        match d.unit {
            Unit::Alu => self.alu_rs.push(rs),
            Unit::Mul => self.mul_rs.push(rs),
            Unit::Br => self.br_rs.push(rs),
            Unit::Lsu => {
                let (store, size, signed) = match d.op { Op::Store { size } => (true, size, false), Op::Load { size, signed } => (false, size, signed), _ => unreachable!() };
                self.lsq.push_back(LsqEntry { seq: q.seq, rob: tag, store, size, signed, base: j, data: if store { k } else { Operand::val(0) }, imm: d.imm, addr: None, addr_at: 0, state: LsqState::Address });
            }
            Unit::Sys => {}
        }
        if d.rd != 0 { self.rat[d.rd as usize] = Some(tag); }
        self.ev(OooEvent::Dispatch { seq: q.seq, rob: tag, pc: q.pc, unit: d.unit, j_tag: j.tag, k_tag: k.tag });
    }

    // ── fetch: one per cycle, along the predicted path ─────────────────
    fn fetch(&mut self) {
        if self.fetchq.len() >= FETCH_IN_FLIGHT || self.iq.len() + self.fetchq.len() >= IQ_SIZE { return; }
        let now = self.cycle;
        let pc = self.pc;
        let (instr, latency) = self.mem.fetch(pc);
        let pred = self.pred.predict(pc);
        let seq = self.next_seq;
        self.next_seq += 1;
        self.fetchq.push_back(FetchReq { seq, pc, instr, pred, fetched: now, ready: now + latency });
        self.ev(OooEvent::Fetch { seq, pc, instr, taken: pred.taken, next: pred.next, btb_hit: pred.btb_hit, latency });
        self.pc = pred.next;
    }

    // ── flushes ─────────────────────────────────────────────────────────
    /// Mispredict: throw away everything younger than `seq`, fetch from `pc`.
    fn flush_younger(&mut self, seq: u64, pc: u32) {
        let mut squashed = (self.iq.len() + self.fetchq.len()) as u32;
        while self.rob.len > 0 && self.rob.get(((self.rob.head + self.rob.len - 1) % ROB_SIZE) as u8).seq > seq {
            let e = self.rob.pop_tail();
            self.retire(&e, true);
            squashed += 1;
        }
        self.alu_rs.retain(|r| r.seq <= seq);
        self.mul_rs.retain(|r| r.seq <= seq);
        self.br_rs.retain(|r| r.seq <= seq);
        self.lsq.retain(|l| l.seq <= seq);
        self.exec.retain(|x| x.seq <= seq);
        self.iq.clear();
        self.fetchq.clear();
        // the RAT as it was after the branch: rebuilt from the ROB entries still in flight
        self.rat = [None; 32];
        let tags: Vec<u8> = self.rob.tags().collect();
        for t in tags { let rd = self.rob.get(t).d.rd; if rd != 0 { self.rat[rd as usize] = Some(t); } }
        self.serializing = self.rob.tags().any(|t| self.rob.get(t).d.unit == Unit::Sys && self.rob.get(t).d.op != Op::Illegal);
        self.stats.squashed += squashed as u64;
        self.ev(OooEvent::Flush { after_seq: seq, squashed, pc });
        self.pc = pc;
    }

    /// Trap / MRET: everything in flight is thrown away.
    fn flush_all(&mut self, pc: u32) {
        let mut squashed = (self.iq.len() + self.fetchq.len()) as u32;
        while self.rob.len > 0 { let e = self.rob.pop_tail(); self.retire(&e, true); squashed += 1; }
        self.alu_rs.clear();
        self.mul_rs.clear();
        self.br_rs.clear();
        self.lsq.clear();
        self.exec.clear();
        self.iq.clear();
        self.fetchq.clear();
        self.rat = [None; 32];
        self.serializing = false;
        self.stats.squashed += squashed as u64;
        self.ev(OooEvent::Flush { after_seq: 0, squashed, pc });
        self.pc = pc;
    }
}

/// Index of the oldest entry with both operands ready.
fn oldest_ready(rs: &[RsEntry]) -> Option<usize> {
    rs.iter().enumerate().filter(|(_, r)| r.j.ready() && r.k.ready()).min_by_key(|(_, r)| r.seq).map(|(i, _)| i)
}
