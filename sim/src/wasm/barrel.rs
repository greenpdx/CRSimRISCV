//! `Barrel`: the 8-thread barrel processor (MicroStepBarrelView). The common
//! methods show hart 0; interrupt lines reach every thread.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use super::{core_api, js};
use crate::sim::core::Core;
use crate::sim::barrel::{
    self, BarrelState, IfDi as BIfDi, DiEx as BDiEx, ExMa as BExMa, MaWb as BMaWb,
    LtIf as BLtIf, SchedMode as BSchedMode, N_THREADS as BN_THREADS,
};

#[wasm_bindgen]
pub struct Barrel {
    inner: BarrelState,
}

core_api!(Barrel);

impl Default for Barrel {
    fn default() -> Self { Self::new() }
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
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

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
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
            mem_read:  c.mem.is_load(),
            mem_write: c.mem.is_store(),
            custom_op: c.custom_op as u8,
        }
    }
}

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct BLtIfJson { valid: bool, thread: u8, pc: u32 }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct BIfDiJson { valid: bool, thread: u8, pc: u32, instr: u32, src: u8 }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct BDiExJson {
    valid: bool, thread: u8, pc: u32, instr: u32,
    rs1_num: u8, rs2_num: u8, rd_num: u8,
    ctrl: BCtrlJson,
}
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct BExMaJson {
    valid: bool, thread: u8, instr: u32,
    alu_result: u32, rd_num: u8,
    branch_taken: bool, branch_target: u32,
    ctrl: BCtrlJson,
}
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
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

#[derive(Serialize, tsify::Tsify)]

#[tsify(missing_as_null)]
struct BCacheJson { hits: u64, misses: u64, busy: bool }
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
struct BBusJson { state: u8 }    // 0=Idle 1=FillI 2=FillD 3=Storing
#[derive(Serialize, tsify::Tsify)]
#[tsify(missing_as_null)]
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
impl Barrel {
    /// A processor with hart 0 booted (active at PC 0), the others idle.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let mut inner = BarrelState::new();
        inner.reset_boot();
        Self { inner }
    }

    /// A fresh processor (memories cleared, hart 0 booted).
    pub fn clear(&mut self) { *self = Self::new(); }

    // Memory loaders
    pub fn load_isram(&mut self, slot: u8, words: &[u32]) { self.inner.load_isram(slot, words); }
    pub fn prewarm_icache(&mut self) { self.inner.prewarm_icache(); }
    pub fn read_isram_word(&self, slot: u8, byte_off: u32) -> u32 { self.inner.isram_read(slot, byte_off) }

    // Thread setup
    pub fn set_thread_active(&mut self, slot: u8, active: bool) {
        if (slot as usize) < BN_THREADS { self.inner.threads[slot as usize].active = active; }
    }
    pub fn set_thread_pc(&mut self, slot: u8, pc: u32) { self.inner.set_thread_pc(slot, pc); }
    pub fn set_thread_priority(&mut self, slot: u8, prio: u8) {
        if (slot as usize) < BN_THREADS { self.inner.threads[slot as usize].priority = prio & 0x3; }
    }
    pub fn activate_all(&mut self) { self.inner.activate_all(); }

    /// Interrupt one thread (B7 thread IRQ).
    pub fn inject_irq(&mut self, slot: u8) { self.inner.inject_irq(slot); }
    /// A CSR of thread `slot` (shared CSRs read the same on every thread).
    pub fn thread_csr_read(&self, slot: u8, addr: u16) -> u32 { self.inner.read_csr(slot, addr) }
    pub fn thread_csr_write(&mut self, slot: u8, addr: u16, val: u32) { self.inner.write_csr(slot, addr, val); }

    // Scheduler
    #[wasm_bindgen(getter)] pub fn sched_mode(&self) -> u8 { self.inner.sched_mode as u8 }
    pub fn set_sched_mode(&mut self, m: u8) { self.inner.sched_mode = BSchedMode::from_u8(m); }

    // Snapshot
    #[wasm_bindgen(unchecked_return_type = "BSnapshot")]
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
        js(&snap)
    }
}
