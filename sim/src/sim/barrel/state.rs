//! Per-thread state and pipeline registers for the 8-thread barrel
//! processor.

use super::bus::Bus;
use super::cache::Cache;
use super::store_buffer::StoreBuffer;

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
#[derive(Debug, Clone, Copy)]
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

    // ── Per-thread privileged state (B7) ──
    // Each thread has its own CSR file. Storage only for now —
    // trap delivery (synchronous + asynchronous) lands in a
    // future phase. The IRQ "controller" in B7 is just a wasm
    // entry point that sets `active=true` on a slot.
    pub priv_mode: u8,    // 3=M, 1=S, 0=U; reset = M
    pub mstatus:  u32,
    pub mtvec:    u32,
    pub mepc:     u32,
    pub mcause:   u32,
    pub mtval:    u32,
    pub mscratch: u32,
    pub mip:      u32,
    pub mie:      u32,
    pub satp:     u32,
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
            priv_mode: 3,
            mstatus: 0, mtvec: 0, mepc: 0, mcause: 0, mtval: 0,
            mscratch: 0, mip: 0, mie: 0, satp: 0,
        }
    }

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
    pub mem_read: bool,
    pub mem_write: bool,
    pub wb_sel: u8,       // 0 = ALU, 1 = Mem, 2 = PC+4
    pub branch: bool,
    pub jump: bool,
    pub custom_op: CustomOp,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LtIf {
    pub valid: bool,
    pub thread: u8,
    pub pc: u32,
}
impl LtIf { pub fn bubble() -> Self { Self::default() } }

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

#[derive(Debug, Clone, Copy, Default)]
pub struct IfDi {
    pub valid: bool,
    pub thread: u8,
    pub pc: u32,
    pub instr: u32,
    pub src: IfSrc,
}
impl IfDi { pub fn bubble() -> Self { Self::default() } }

#[derive(Debug, Clone, Copy, Default)]
pub struct DiEx {
    pub valid: bool,
    pub thread: u8,
    pub pc: u32,
    pub pc_plus4: u32,
    pub instr: u32,
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
}
impl DiEx { pub fn bubble() -> Self { Self::default() } }

#[derive(Debug, Clone, Copy, Default)]
pub struct ExMa {
    pub valid: bool,
    pub thread: u8,
    pub instr: u32,
    pub alu_result: u32,
    pub rs1_data: u32,        // carried for custom-0 (FORK start_pc, FORKRET retval)
    pub rs2_data: u32,
    pub pc_plus4: u32,
    pub rd_num: u8,
    pub funct3: u8,
    pub branch_taken: bool,
    pub branch_target: u32,
    pub ctrl: ControlBits,
}
impl ExMa { pub fn bubble() -> Self { Self::default() } }

#[derive(Debug, Clone, Copy, Default)]
pub struct MaWb {
    pub valid: bool,
    pub thread: u8,
    pub instr: u32,
    pub alu_result: u32,
    pub mem_data: u32,
    pub pc_plus4: u32,
    pub rd_num: u8,
    pub ctrl: ControlBits,
}
impl MaWb { pub fn bubble() -> Self { Self::default() } }

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
    pub ext_imem: Vec<u32>,
    pub ext_dmem: Vec<u32>,
    pub io_data: u32,

    /// Local 16 KiB instruction scratchpads, one per slot 0/1/2.
    /// Single-cycle, never misses, only fetchable by the owning
    /// slot. Mapped at `ISRAM_BASE + ISRAM_BYTES_PER_SLOT * slot`.
    pub local_isram: [Box<[u32; ISRAM_WORDS_PER_SLOT]>; ISRAM_N_SLOTS],

    /// Shared 4 KiB I-L1 (slots 3..7 always; slots 0..2 when their
    /// PC is outside their local I-SRAM range).
    pub icache: Cache,
    /// Shared 4 KiB D-L1 (all slots).
    pub dcache: Cache,
    /// Write-through store buffer drained by the bus arbiter.
    pub store_buffer: StoreBuffer,
    /// Single shared external memory bus, fixed-priority.
    pub bus: Bus,

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
            threads: [Thread::idle(); N_THREADS],
            ext_imem: vec![0x0000_0013; EXT_IMEM_WORDS], // NOPs
            ext_dmem: vec![0; EXT_DMEM_WORDS],
            io_data: 0,
            local_isram: [
                Box::new([0x0000_0013; ISRAM_WORDS_PER_SLOT]),
                Box::new([0x0000_0013; ISRAM_WORDS_PER_SLOT]),
                Box::new([0x0000_0013; ISRAM_WORDS_PER_SLOT]),
            ],
            icache: Cache::new("I-L1"),
            dcache: Cache::new("D-L1"),
            store_buffer: StoreBuffer::new(),
            bus: Bus::new(),
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
        self.threads = [Thread::idle(); N_THREADS];
        self.threads[0] = Thread::boot(0);
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
        for (i, w) in words.iter().enumerate() {
            if i < self.ext_imem.len() { self.ext_imem[i] = *w; }
        }
    }

    pub fn imem_read(&self, addr: u32) -> u32 {
        let idx = (addr >> 2) as usize;
        self.ext_imem.get(idx).copied().unwrap_or(0)
    }

    pub fn dmem_read(&self, addr: u32) -> u32 {
        let idx = ((addr & !3).wrapping_sub(EXT_DMEM_BASE) >> 2) as usize;
        self.ext_dmem.get(idx).copied().unwrap_or(0)
    }

    pub fn dmem_write(&mut self, addr: u32, val: u32) {
        let idx = ((addr & !3).wrapping_sub(EXT_DMEM_BASE) >> 2) as usize;
        if idx < self.ext_dmem.len() { self.ext_dmem[idx] = val; }
    }

    /// Load a flat instruction image into a slot's local I-SRAM
    /// (slot must be 0..2). Words past the SRAM size are ignored.
    pub fn load_isram(&mut self, slot: u8, words: &[u32]) {
        if (slot as usize) >= ISRAM_N_SLOTS { return; }
        let dst = &mut self.local_isram[slot as usize];
        for (i, w) in words.iter().enumerate() {
            if i < dst.len() { dst[i] = *w; }
        }
    }

    /// Read a single instruction word from a slot's I-SRAM at the
    /// given local byte offset. Out-of-range returns NOP.
    pub fn isram_read(&self, slot: u8, byte_off: u32) -> u32 {
        let s = slot as usize;
        if s >= ISRAM_N_SLOTS { return 0x0000_0013; }
        let idx = (byte_off >> 2) as usize;
        if idx < ISRAM_WORDS_PER_SLOT {
            self.local_isram[s][idx]
        } else {
            0x0000_0013
        }
    }

    /// Returns `Some(slot)` if `pc` falls in slot N's I-SRAM range
    /// (N = 0, 1, or 2). Used by stage_if to decide whether to
    /// route the fetch to local SRAM or external memory.
    pub fn isram_slot_of(pc: u32) -> Option<u8> {
        if pc >= ISRAM_BASE && pc < ISRAM_TOP {
            let off = pc - ISRAM_BASE;
            Some((off / ISRAM_BYTES_PER_SLOT) as u8)
        } else {
            None
        }
    }

    /// Inject an IRQ on `slot`. Models the hardware "interrupt
    /// line asserted" event. In the barrel design IRQs are
    /// pre-loaded handler threads — the controller just sets
    /// `active=true` and the slot starts running its handler on
    /// the next barrel rotation. Also sets MEIP in the slot's mip
    /// CSR for visualization.
    pub fn inject_irq(&mut self, slot: u8) {
        if (slot as usize) >= N_THREADS { return; }
        let t = &mut self.threads[slot as usize];
        t.active = true;
        t.mip |= 1 << 11; // MEIP
    }

    /// Read one CSR for a given thread, by 12-bit address. Returns
    /// 0 for unknown CSRs. Used by the visualizer's CSR strip.
    pub fn read_csr(&self, slot: u8, addr: u16) -> u32 {
        if (slot as usize) >= N_THREADS { return 0; }
        let t = &self.threads[slot as usize];
        match addr {
            0x300 => t.mstatus,
            0x305 => t.mtvec,
            0x340 => t.mscratch,
            0x341 => t.mepc,
            0x342 => t.mcause,
            0x343 => t.mtval,
            0x344 => t.mip,
            0x304 => t.mie,
            0x180 => t.satp,
            _     => 0,
        }
    }

    pub fn write_csr(&mut self, slot: u8, addr: u16, val: u32) {
        if (slot as usize) >= N_THREADS { return; }
        let t = &mut self.threads[slot as usize];
        match addr {
            0x300 => t.mstatus  = val,
            0x305 => t.mtvec    = val,
            0x340 => t.mscratch = val,
            0x341 => t.mepc     = val,
            0x342 => t.mcause   = val,
            0x343 => t.mtval    = val,
            0x344 => t.mip      = val,
            0x304 => t.mie      = val,
            0x180 => t.satp     = val,
            _     => {}
        }
    }

    /// Test helper: pre-load every line of ext_imem into the
    /// I-cache so the first fetch is a guaranteed hit. Lets B1-B3
    /// tests run without paying the 8-cycle miss latency on first
    /// access. NOT something a real CPU would do.
    pub fn prewarm_icache(&mut self) {
        use super::cache::{CACHE_LINE_WORDS, CACHE_LINE_BYTES};
        let n = self.ext_imem.len();
        let lines = (n + CACHE_LINE_WORDS - 1) / CACHE_LINE_WORDS;
        for l in 0..lines {
            let mut data = [0u32; CACHE_LINE_WORDS];
            for w in 0..CACHE_LINE_WORDS {
                let idx = l * CACHE_LINE_WORDS + w;
                if idx < n { data[w] = self.ext_imem[idx]; }
            }
            let line_addr = (l as u32) * CACHE_LINE_BYTES;
            self.icache.install(line_addr, data);
        }
    }
}
