//! Per-thread state and pipeline registers for the 8-thread barrel
//! processor.

use super::mem::BarrelMem;
use super::bus::Bus;
use super::cache::Cache;
use super::dma::DmaController;
use super::irq_controller::IrqController;
use super::store_buffer::StoreBuffer;
use crate::isa::csr::{addr, csr_exec};
use crate::sim::system::{PrivMode, SysUnit};
use crate::sim::forwarding::RegWriter;
pub use crate::sim::stage::{CsrOp, Latch, MemKind, TrapKind, WbSel};

/// CSRs the barrel shares across all threads (one value, mirrored into
/// every thread's system unit on write): the trap vector, interrupt
/// enable / pending and the delegation registers.
pub const SHARED_CSRS: [u16; 5] = [addr::MTVEC, addr::MIE, addr::MIP, addr::MEDELEG, addr::MIDELEG];

/// A thread's system unit at reset: M-mode, misa = RV32I, mhartid = slot.
pub fn hart_sys(slot: u8) -> SysUnit {
    let mut s = SysUnit::new();
    s.csrs.set_counter(addr::MISA, 0x4000_0100);
    s.csrs.set_counter(addr::MHARTID, slot as u32);
    s
}

pub const N_THREADS: usize = 8;
pub const EXT_IMEM_WORDS: usize = 1024;
pub const EXT_DMEM_WORDS: usize = 1024;
pub const EXT_DMEM_BASE: u32 = 0x1000_0000;

// Local Instruction Scratchpad (ITCM) for slots 0/1/2.
//   slot 0:  0xF000_0000 .. 0xF000_3FFF   (16 KiB = 4096 words)
//   slot 1:  0xF000_4000 .. 0xF000_7FFF
//   slot 2:  0xF000_8000 .. 0xF000_BFFF
pub const ISRAM_BASE: u32 = 0xF000_0000;
pub const ISRAM_BYTES_PER_SLOT: u32 = 0x4000;     // 16 KiB
pub const ISRAM_WORDS_PER_SLOT: usize = (ISRAM_BYTES_PER_SLOT as usize) / 4;
pub const ISRAM_N_SLOTS: usize = 3;
pub const ISRAM_TOP: u32 = ISRAM_BASE + (ISRAM_N_SLOTS as u32) * ISRAM_BYTES_PER_SLOT;

/// Single thread context. Each barrel slot owns one of these.
#[derive(Debug, Clone)]
pub struct Thread {
    // ── execution state ──
    pub pc: u32,
    pub regs: [u32; 32],

    // ── scheduler state ──
    /// Slot is runnable. Scheduler picks only from active threads.
    pub active: bool,
    /// -1 = not waiting; otherwise = TID this thread is JOIN-blocked on.
    pub waiting: i8,
    /// 0..3 (low..high). Used by priority schedulers in later phases.
    pub priority: u8,
    /// Cycles since this slot last executed an instruction (saturating).
    /// Used by aged-priority schedulers in later phases.
    pub age: u8,
    /// One-shot YIELD pulse — scheduler skips this slot for one round
    /// then clears the flag.
    pub yield_pulse: bool,
    /// True while a cache miss is outstanding for this slot. Scheduler
    /// must skip the slot until the miss completes. Phase B1: never
    /// set (no caches yet).
    pub miss_pending: bool,
    /// True if this thread tried to fetch from an instruction
    /// scratchpad it doesn't own. Set by stage_if; the thread is
    /// also deactivated by the same fault.
    pub instr_access_fault: bool,

    // ── Per-thread privileged state ──
    /// Privilege mode, CSRs, trap entry / return and IRQ sampling: the
    /// system unit shared with the other cores (sim/system), one per
    /// thread (hart). mtvec / mie / mip / medeleg / mideleg are kept equal
    /// in all threads (`SHARED_CSRS`, mirrored by `BarrelState::write_csr`).
    pub sys: SysUnit,
}

impl Thread {
    /// Idle (locked) thread. Not active, no JOIN, lowest priority.
    pub fn idle() -> Self {
        Self {
            pc: 0,
            regs: [0; 32],
            active: false,
            waiting: -1,
            priority: 0,
            age: 0,
            yield_pulse: false,
            miss_pending: false,
            instr_access_fault: false,
            sys: hart_sys(0),
        }
    }

    /// Idle thread for slot `slot` (its mhartid = slot).
    pub fn idle_hart(slot: u8) -> Self {
        Self { sys: hart_sys(slot), ..Self::idle() }
    }

    /// Current privilege mode as bits: 3 = M, 1 = S, 0 = U.
    pub fn priv_bits(&self) -> u8 { self.sys.priv_mode as u8 }

    /// Boot-active thread (slot 0 on real reset). Used by `BarrelState`
    /// helpers when only one slot is supposed to be running.
    pub fn boot(pc: u32) -> Self {
        let mut t = Self::idle();
        t.pc = pc;
        t.active = true;
        t.priority = 3;
        t
    }
}

// ─── Pipeline registers ─────────────────────────────────────────────
//
// Each pipeline register carries a `thread` tag (the barrel slot ID
// 0..7) so later stages know which thread context this instruction
// belongs to. WB uses the tag to write back to the correct slot's
// regfile.

/// Custom-0 instruction kind (opcode 0x0b). 0 = not a custom-0 instr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CustomOp {
    None    = 0,
    Fork    = 1,  // FORK    rd, rs1, rs2  — rs1=start_pc, rs2=arg, rd←slot or -1
    Join    = 2,  // JOIN    rs1           — block until thread rs1 done
    Done    = 3,  // DONE                  — wake joiners, deactivate self
    Yield   = 4,  // YIELD                 — skip one scheduler round
    Tid     = 5,  // TID     rd            — rd ← current TID
    Forkret = 6,  // FORKRET rd            — like DONE but pass rd as return val
}
impl Default for CustomOp { fn default() -> Self { CustomOp::None } }
impl CustomOp {
    pub fn from_funct7(f7: u8) -> Self {
        match f7 {
            0x00 => CustomOp::Fork,
            0x01 => CustomOp::Join,
            0x02 => CustomOp::Done,
            0x03 => CustomOp::Yield,
            0x04 => CustomOp::Tid,
            0x05 => CustomOp::Forkret,
            _    => CustomOp::None,
        }
    }
}

/// Control signals decoded by stage_di and propagated forward.
#[derive(Debug, Clone, Copy, Default)]
pub struct ControlBits {
    pub reg_write: bool,
    pub alu_src: bool,    // false = rs2, true = imm
    pub alu_op: u8,
    pub mem: MemKind,
    pub wb_sel: WbSel,    // Alu (also the CSR old value), Mem, Pc4
    pub branch: bool,
    pub jump: bool,
    pub custom_op: CustomOp,
}

/// LT/IF payload: the thread the scheduler picked.
#[derive(Debug, Clone, Copy, Default)]
pub struct LtIfData {
    pub thread: u8,
}
/// LT/IF stage register (`instr` is 0: not fetched yet).
pub type LtIf = Latch<LtIfData>;

/// Where an IF-stage instruction came from. Useful for the
/// visualizer in later phases (B6) to color the fetch path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IfSrc {
    None       = 0,
    LocalIsram = 1,
    ExtMem     = 2,
    ICacheHit  = 3,   // reserved for B4
    ICacheMiss = 4,   // reserved for B4
}
impl Default for IfSrc { fn default() -> Self { IfSrc::None } }

/// IF/DI payload.
#[derive(Debug, Clone, Copy, Default)]
pub struct IfDiData {
    pub thread: u8,
    pub src: IfSrc,
}
/// IF/DI stage register.
pub type IfDi = Latch<IfDiData>;

/// DI/EX payload.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiExData {
    pub thread: u8,
    pub pc_plus4: u32,
    pub rs1_data: u32,
    pub rs2_data: u32,
    pub imm: u32,
    pub rs1_num: u8,
    pub rs2_num: u8,
    pub rd_num: u8,
    pub funct3: u8,
    pub funct7b30: u8,
    pub opcode: u8,
    pub ctrl: ControlBits,
    /// Sync trap / MRET, delivered by step() after EX.
    pub trap_kind: TrapKind,
    // ── Zicsr fields ──
    pub csr_op: CsrOp,
    pub csr_addr: u16,
    pub csr_uimm: u8,
    pub csr_imm_form: bool,
}
/// DI/EX stage register.
pub type DiEx = Latch<DiExData>;

/// EX/MA payload.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExMaData {
    pub thread: u8,
    pub alu_result: u32,
    pub rs1_data: u32,        // carried for custom-0 (FORK start_pc, FORKRET retval)
    pub rs2_data: u32,
    pub pc_plus4: u32,
    pub rd_num: u8,
    pub funct3: u8,
    pub branch_taken: bool,
    pub branch_target: u32,
    pub ctrl: ControlBits,
    /// Sync trap / MRET (mepc = the latch's `pc`).
    pub trap_kind: TrapKind,
    // ── Zicsr fields ──
    pub csr_op: CsrOp,
    pub csr_addr: u16,
    pub csr_write_val: u32,
    pub csr_src_is_zero: bool,
}
/// EX/MA stage register.
pub type ExMa = Latch<ExMaData>;

/// MA/WB payload.
#[derive(Debug, Clone, Copy, Default)]
pub struct MaWbData {
    pub thread: u8,
    pub alu_result: u32,
    pub mem_data: u32,
    pub pc_plus4: u32,
    pub rd_num: u8,
    pub ctrl: ControlBits,
}
/// MA/WB stage register.
pub type MaWb = Latch<MaWbData>;

impl MaWbData {
    /// The value WB writes to rd (and MA/WB forwards).
    pub fn wb_value(&self) -> u32 { self.ctrl.wb_sel.select(self.alu_result, self.mem_data, self.pc_plus4) }
}

impl RegWriter for ExMa {
    fn writes(&self) -> Option<u8> { (self.valid && self.ctrl.reg_write && self.rd_num != 0).then_some(self.rd_num) }
    fn thread(&self) -> u8 { self.data.thread }
    fn fwd_value(&self) -> u32 { self.alu_result }
}
impl RegWriter for MaWb {
    fn writes(&self) -> Option<u8> { (self.valid && self.ctrl.reg_write && self.rd_num != 0).then_some(self.rd_num) }
    fn thread(&self) -> u8 { self.data.thread }
    fn fwd_value(&self) -> u32 { self.wb_value() }
}

// ─── Scheduler mode ─────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SchedMode {
    /// Pick (current+1) mod N, skipping stalled / blocked slots.
    /// Fair and deterministic.
    RoundRobin = 0,
    /// Strict priority: always pick the highest-priority runnable
    /// thread. Round-robin tiebreak within the same priority. Can
    /// starve low-priority threads if a high-priority one is
    /// always runnable.
    Priority = 1,
    /// Effective priority = `base_priority + min(age, 8)`. A
    /// thread's priority bumps up by 1 for every cycle it hasn't
    /// run, so even priority-0 threads eventually win against a
    /// busy priority-3 thread. Anti-starvation by construction.
    AgedPriority = 2,
    /// Two-class: pinned slots (0/1/2) always run if any is
    /// runnable, RR among them. Otherwise RR among general slots
    /// (3..7). Models the "hard real-time + soft real-time" split
    /// common in safety-critical RTOS.
    TwoClass = 3,
}
impl Default for SchedMode { fn default() -> Self { SchedMode::RoundRobin } }
impl SchedMode {
    pub fn from_u8(v: u8) -> SchedMode {
        match v {
            1 => SchedMode::Priority,
            2 => SchedMode::AgedPriority,
            3 => SchedMode::TwoClass,
            _ => SchedMode::RoundRobin,
        }
    }
}

// ─── Top-level state ───────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct BarrelState {
    pub threads: [Thread; N_THREADS],

    // External memory (Princeton-style: separate code and data
    // arrays for B1 simplicity; later phases unify and add caches).
    /// External instruction / data memory, the per-slot I-SRAMs, the I/O word.
    pub mem: BarrelMem,

    /// Shared 4 KiB I-L1 (slots 3..7 always; slots 0..2 when their
    /// PC is outside their local I-SRAM range).
    pub icache: Cache,
    /// Shared 4 KiB D-L1 (all slots).
    pub dcache: Cache,
    /// Write-through store buffer drained by the bus arbiter.
    pub store_buffer: StoreBuffer,
    /// Single shared external memory bus, fixed-priority.
    pub bus: Bus,
    /// DMA controller for loading code into per-slot I-SRAMs.
    pub dma: DmaController,
    /// IRQ controller with routing table.
    pub irq: IrqController,

    // Pipeline registers
    pub lt_if: LtIf,
    pub if_di: IfDi,
    pub di_ex: DiEx,
    pub ex_ma: ExMa,
    pub ma_wb: MaWb,
    /// Snapshot of `ma_wb` from before this cycle's WB consumed it,
    /// so the visualizer can show MA and WB as different instructions.
    pub last_wb: MaWb,

    // Scheduler state
    pub sched_mode: SchedMode,
    /// Which thread the LT stage picked most recently. The next LT
    /// scan starts at `(current_thread + 1) % N`.
    pub current_thread: u8,
    /// Ring buffer of last 32 cycles' dispatched TIDs (255 = idle).
    pub sched_history: [u8; 32],
    pub sched_history_idx: u8,

    pub cycle_count: u64,
    pub halted: bool,
}

impl Default for BarrelState {
    fn default() -> Self {
        Self {
            // Phase B1: all 8 threads start active so round-robin
            // dispatch is observable in tests. Real boot (only slot 0
            // active) is set by `BarrelState::reset_boot()` in later
            // phases when custom-0 / IRQ controller exists.
            threads: std::array::from_fn(|i| Thread::idle_hart(i as u8)),
            mem: BarrelMem::default(),
            icache: Cache::new("I-L1"),
            dcache: Cache::new("D-L1"),
            store_buffer: StoreBuffer::new(),
            bus: Bus::new(),
            dma: DmaController::new(),
            irq: IrqController::new(),
            lt_if: LtIf::default(),
            if_di: IfDi::default(),
            di_ex: DiEx::default(),
            ex_ma: ExMa::default(),
            ma_wb: MaWb::default(),
            last_wb: MaWb::default(),
            sched_mode: SchedMode::default(),
            current_thread: N_THREADS as u8 - 1,
            sched_history: [0xff; 32],
            sched_history_idx: 0,
            cycle_count: 0,
            halted: false,
        }
    }
}

impl BarrelState {
    pub fn new() -> Self { Self::default() }

    /// Make all 8 threads active. Default for tests in Phase B1.
    /// Each thread starts at its own PC; the caller sets these via
    /// `set_thread_pc`.
    pub fn activate_all(&mut self) {
        for t in self.threads.iter_mut() {
            t.active = true;
            t.priority = 1;
        }
    }

    /// Boot reset: only slot 0 active, PC=0, others idle. This is
    /// what real reset will look like in later phases; B1 tests
    /// usually call `activate_all` instead.
    pub fn reset_boot(&mut self) {
        let shared: Vec<(u16, u32)> = SHARED_CSRS.iter().map(|&a| (a, self.read_csr(0, a))).collect();
        self.threads = std::array::from_fn(|i| Thread::idle_hart(i as u8));
        self.threads[0] = Thread::boot(0);
        // the shared CSRs (global before) survive a boot reset
        for (a, v) in shared { self.write_csr(0, a, v); }
    }

    pub fn set_thread_pc(&mut self, slot: u8, pc: u32) {
        if (slot as usize) < N_THREADS {
            self.threads[slot as usize].pc = pc;
        }
    }
    pub fn write_thread_register(&mut self, slot: u8, idx: u8, val: u32) {
        let t = &mut self.threads[slot as usize];
        if idx != 0 && (idx as usize) < 32 { t.regs[idx as usize] = val; }
    }
    pub fn read_thread_register(&self, slot: u8, idx: u8) -> u32 {
        self.threads[slot as usize].regs[(idx as usize) & 0x1f]
    }

    /// Load a flat instruction image into external instruction
    /// memory starting at byte address 0.
    pub fn load_program(&mut self, words: &[u32]) {
        for (d, w) in self.mem.imem.iter_mut().zip(words) { *d = *w; }
    }

    pub fn imem_read(&self, addr: u32) -> u32 { self.mem.imem_read(addr) }
    pub fn dmem_read(&self, addr: u32) -> u32 { self.mem.dmem_read(addr) }
    pub fn dmem_write(&mut self, addr: u32, val: u32) { self.mem.dmem_write(addr, val) }

    /// Load a flat instruction image into a slot's local I-SRAM
    /// (slot must be 0..2). Words past the SRAM size are ignored.
    pub fn load_isram(&mut self, slot: u8, words: &[u32]) { self.mem.load_isram(slot, words) }

    /// Read a single instruction word from a slot's I-SRAM at the
    /// given local byte offset. Out-of-range returns NOP.
    pub fn isram_read(&self, slot: u8, byte_off: u32) -> u32 { self.mem.isram_read(slot, byte_off) }

    /// Returns `Some(slot)` if `pc` falls in slot N's I-SRAM range
    /// (N = 0, 1, or 2). Used by stage_if to decide whether to
    /// route the fetch to local SRAM or external memory.
    pub fn isram_slot_of(pc: u32) -> Option<u8> { BarrelMem::isram_slot_of(pc) }

    /// Read a CSR of thread `slot` (its system unit). The shared CSRs
    /// (`SHARED_CSRS`) read the same in every thread; an out-of-range slot
    /// reads them from thread 0 and everything else as 0.
    pub fn read_csr(&self, slot: u8, addr: u16) -> u32 {
        match self.threads.get(slot as usize) {
            Some(t) => t.sys.csrs.read(addr),
            None if SHARED_CSRS.contains(&addr) => self.threads[0].sys.csrs.read(addr),
            None => 0,
        }
    }

    /// Write a CSR of thread `slot`. A shared CSR is written in every
    /// thread (one value, as the old global mtvec / mie / mip / m*deleg).
    pub fn write_csr(&mut self, slot: u8, addr: u16, val: u32) {
        if SHARED_CSRS.contains(&addr) {
            for t in self.threads.iter_mut() { t.sys.csrs.write(addr, val); }
        } else if let Some(t) = self.threads.get_mut(slot as usize) {
            t.sys.csrs.write(addr, val);
        }
    }

    /// Zicsr read-modify-write for thread `slot` (shared Zicsr unit):
    /// `op` = funct3[1:0] (1 RW · 2 RS · 3 RC), `src_is_zero` = rs1 is x0 /
    /// zimm is 0 (RS / RC then do not write). Returns the old value.
    pub fn csr_rmw(&mut self, slot: u8, addr: u16, op: u8, src_is_zero: bool, val: u32) -> u32 {
        let old = self.read_csr(slot, addr);
        let r = csr_exec(old, op & 3, if src_is_zero { 0 } else { 1 }, val);
        if r.write { self.write_csr(slot, addr, r.new); }
        old
    }

    /// Deliver a synchronous trap on thread `slot` through its system
    /// unit (medeleg delegation, mstatus stacking, xepc / xcause / xtval)
    /// and redirect its pc to the handler.
    pub fn take_sync_trap(&mut self, slot: u8, cause: u32, epc: u32, tval: u32) {
        let cycle = self.cycle_count;
        if let Some(t) = self.threads.get_mut(slot as usize) {
            t.pc = t.sys.trap(cause, false, epc, tval, cycle);
        }
    }

    /// Execute MRET for thread `slot`: restores MIE / priv from the stack
    /// and pc from mepc (shared system unit).
    pub fn do_mret(&mut self, slot: u8) {
        if let Some(t) = self.threads.get_mut(slot as usize) {
            t.pc = t.sys.xret(false);
        }
    }

    /// Interrupt delivery for thread `slot` at its fetch slot: if its
    /// system unit has an interrupt to take (mip & mie, MIE / mode,
    /// mideleg), enter the handler and return the new pc. mepc = the pc
    /// it was about to fetch. The caller only asks when none of the
    /// thread's instructions are in flight, so the trap is precise.
    pub fn take_interrupt(&mut self, slot: u8) -> Option<u32> {
        let cycle = self.cycle_count;
        let t = self.threads.get_mut(slot as usize)?;
        let cause = t.sys.sample_irq()?;
        t.pc = t.sys.trap(cause, true, t.pc, 0, cycle);
        Some(t.pc)
    }

    /// External interrupt lines (MEI / MTI / MSI / S…), as on the other
    /// cores: level-triggered, seen by every thread (each takes it when its
    /// own mstatus / mode allows).
    pub fn raise_irq(&mut self, mask: u32) {
        for t in self.threads.iter_mut() { t.sys.raise_irq(mask); }
    }
    pub fn clear_irq(&mut self) {
        for t in self.threads.iter_mut() { t.sys.clear_irq(); }
    }

    /// Privilege mode of thread `slot`.
    pub fn priv_mode_of(&self, slot: u8) -> PrivMode {
        self.threads.get(slot as usize).map(|t| t.sys.priv_mode).unwrap_or(PrivMode::M)
    }

    /// Update inject_irq to use the IRQ controller.
    pub fn inject_irq(&mut self, line: u8) {
        self.irq.assert_line(line);
    }

    /// Test helper: pre-load every line of ext_imem into the
    /// I-cache so the first fetch is a guaranteed hit. Lets B1-B3
    /// tests run without paying the 8-cycle miss latency on first
    /// access. NOT something a real CPU would do.
    pub fn prewarm_icache(&mut self) {
        use super::cache::{CACHE_LINE_WORDS, CACHE_LINE_BYTES};
        let n = self.mem.imem.len();
        let lines = (n + CACHE_LINE_WORDS - 1) / CACHE_LINE_WORDS;
        for l in 0..lines {
            let mut data = [0u32; CACHE_LINE_WORDS];
            for w in 0..CACHE_LINE_WORDS {
                let idx = l * CACHE_LINE_WORDS + w;
                if idx < n { data[w] = self.mem.imem[idx]; }
            }
            let line_addr = (l as u32) * CACHE_LINE_BYTES;
            self.icache.install(line_addr, data);
        }
    }
}
