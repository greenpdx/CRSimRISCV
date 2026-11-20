//! 8-thread barrel processor with 6-stage pipeline.
//!
//! Phase B1: skeleton + round-robin scheduler. No caches, no local
//! I-SRAM, no custom-0 instructions, no wasm, no Vue. The only
//! purpose of this phase is to dispatch 8 threads in round-robin
//! order, run their instructions correctly, and write back results
//! to the right per-thread register file.
//!
//! Pipeline stages, in dispatch order:
//!   LT (Load Thread) → IF → DI → EX → MA → WB
//!
//! `step()` runs the stages in REVERSE order (WB first, then MA,
//! then EX, then DI, then IF, then LT) so that earlier stages see
//! the *new* register values written back by later stages this
//! cycle. This is the same split-cycle trick the existing 5-stage
//! `pipeline/` module uses.
//!
//! Hazards: when ≥ 2 threads are runnable, instructions from the
//! same thread are always at least N=8 cycles apart in the
//! pipeline. Our pipeline depth is only 6, so a thread's
//! instruction is fully written back before its next instruction
//! enters DI. **No forwarding logic, no stalls, no flushes are
//! needed.** This is the central magic of barrel processors.
//!
//! The single-thread degenerate case (only one slot active) is
//! known broken in B1 and gets forwarding/load-use stalls in a
//! later phase.

pub mod bus;
pub mod cache;
pub mod dma;
pub mod irq_controller;
pub mod store_buffer;
pub mod thread_mmio;

pub mod state;
pub mod scheduler;
pub mod stage_lt;
pub mod stage_if;
pub mod stage_di;
pub mod stage_ex;
pub mod stage_ma;
pub mod stage_wb;

pub use state::{
    BarrelState, ControlBits, CustomOp, DiEx, ExMa, IfDi, IfSrc, LtIf, MaWb, SchedMode,
    Thread, EXT_DMEM_BASE, ISRAM_BASE, ISRAM_BYTES_PER_SLOT, ISRAM_N_SLOTS,
    ISRAM_TOP, ISRAM_WORDS_PER_SLOT, N_THREADS,
};

/// Same-thread load-use hazard. Returns true if `prev_di_ex` is a
/// load whose rd is read by `prev_if_di`'s instruction.
fn detect_load_use(if_di: &IfDi, di_ex: &DiEx) -> bool {
    if !di_ex.valid || !di_ex.ctrl.mem_read || di_ex.rd_num == 0 { return false; }
    if !if_di.valid || di_ex.thread != if_di.thread { return false; }
    let instr = if_di.instr;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;
    di_ex.rd_num == rs1 || di_ex.rd_num == rs2
}

impl BarrelState {
    pub fn step(&mut self) {
        if self.halted { return; }

        // ── 0a. IRQ controller tick ───────────────────────────────
        // Activate handler slots for any pending+routed IRQ lines.
        self.irq.tick(&mut self.threads);

        // ── 0b. Bus tick ─────────────────────────────────────────
        self.bus_tick();

        // Snapshot inputs each stage will use.
        let prev_lt_if = self.lt_if;
        let prev_if_di = self.if_di;
        let prev_di_ex = self.di_ex;
        let prev_ex_ma = self.ex_ma;
        let prev_ma_wb = self.ma_wb;

        self.last_wb = prev_ma_wb;

        // ── Counters: cycle (clocks done) in every thread; instret for
        // the thread whose instruction leaves WB.
        let cyc = self.cycle_count as u32;
        for t in self.threads.iter_mut() { t.sys.csrs.set_counter(crate::sim::system::addr::CYCLE, cyc); }

        // ── 1. WB ────────────────────────────────────────────────
        stage_wb::compute_wb(self, &prev_ma_wb);
        if prev_ma_wb.valid {
            if let Some(t) = self.threads.get_mut(prev_ma_wb.thread as usize) {
                t.sys.csrs.bump(crate::sim::system::addr::INSTRET);
            }
        }

        // ── 2. MA ────────────────────────────────────────────────
        let new_ma_wb = stage_ma::compute_ma(self, &prev_ex_ma);

        // ── 3. EX ────────────────────────────────────────────────
        // Pass the in-flight pipeline registers so EX can forward
        // from them. In multi-thread barrel mode the forwarding
        // paths never fire (different threads); in single-thread
        // fallback they cover RAW dependencies.
        let mut new_ex_ma = stage_ex::compute_ex(&prev_di_ex, &prev_ex_ma, &prev_ma_wb);

        // ── 4. DI ────────────────────────────────────────────────
        let new_di_ex = stage_di::compute_di(self, &prev_if_di);

        // ── Load-use hazard detection ───────────────────────────
        // A load that is about to enter EX (in prev_di_ex) whose
        // rd is read by the instruction about to enter DI (in
        // prev_if_di) — same thread only — needs a 1-cycle stall.
        // The load result isn't ready until end of MA, so the
        // consumer must wait one cycle so MA→EX forwarding works
        // next cycle.
        let stall = detect_load_use(&prev_if_di, &prev_di_ex);

        // ── 5. IF ────────────────────────────────────────────────
        // stage_if needs &mut self in B3+ because an instruction
        // access fault on a mis-targeted I-SRAM fetch deactivates
        // the offending thread, and B4+ may set miss_pending.
        let new_if_di = stage_if::compute_if(self, &prev_lt_if);

        // ── PC advance for the thread whose IF just succeeded ───
        // Only advance pc for a real fetch. If IF returned a bubble
        // (cache miss, instr access fault) the pc must NOT advance,
        // so the thread will re-fetch the same instruction once
        // its miss is satisfied or once it's re-activated.
        // ALSO: if a load-use stall fires, we need to roll back the
        // pc advance for the consumer's thread so the freshly-
        // fetched (but unwanted) instruction is re-fetched.
        if new_if_di.valid {
            let s = new_if_di.thread as usize;
            self.threads[s].pc = self.threads[s].pc.wrapping_add(4);
        }
        if stall && new_if_di.valid {
            let s = new_if_di.thread as usize;
            self.threads[s].pc = self.threads[s].pc.wrapping_sub(4);
        }

        // ── 6. LT ────────────────────────────────────────────────
        let (mut new_lt_if, picked) = stage_lt::compute_lt(self);

        if let Some(slot) = picked {
            self.threads[slot as usize].age = 0;
            self.current_thread = slot;
            let idx = (self.sched_history_idx as usize) % self.sched_history.len();
            self.sched_history[idx] = slot;
            self.sched_history_idx = self.sched_history_idx.wrapping_add(1);
        } else {
            let idx = (self.sched_history_idx as usize) % self.sched_history.len();
            self.sched_history[idx] = 0xff;
            self.sched_history_idx = self.sched_history_idx.wrapping_add(1);
        }

        // Age all non-picked threads (saturating).
        for (i, t) in self.threads.iter_mut().enumerate() {
            if Some(i as u8) != picked {
                t.age = t.age.saturating_add(1);
            }
        }

        // ── Sync trap / MRET resolution ──
        //
        // If the instruction in new_ex_ma carries a trap_kind, the
        // trapping thread's PC gets redirected to mtvec (for traps)
        // or mepc (for MRET). In multi-thread barrel mode, no
        // other in-flight instructions need squashing because
        // they belong to different threads.
        if new_ex_ma.valid && new_ex_ma.trap_kind != 0 {
            let slot = new_ex_ma.thread;
            match new_ex_ma.trap_kind {
                4 => {
                    // MRET: restore pc from mepc, MIE from MPIE.
                    self.do_mret(slot);
                }
                kind => {
                    // Sync trap: illegal(1), ecall(2), ebreak(3).
                    let cause: u32 = match kind {
                        1 => 2,  // illegal instruction
                        2 => self.threads[slot as usize].sys.ecall_cause(),   // ECALL (by mode)
                        3 => 3,  // EBREAK
                        5 => 1,  // instruction access fault
                        _ => 0,
                    };
                    let tval = if kind == 1 { new_ex_ma.instr } else { 0 };
                    self.take_sync_trap(slot, cause, new_ex_ma.trap_pc, tval);
                }
            }
            // Defang the trapping instruction so MA/WB don't
            // produce side effects (same pattern as 5-stage).
            new_ex_ma.ctrl.reg_write = false;
            new_ex_ma.ctrl.mem_read  = false;
            new_ex_ma.ctrl.mem_write = false;
            new_ex_ma.ctrl.custom_op = CustomOp::None;
            new_ex_ma.rd_num = 0;
            new_ex_ma.trap_kind = 0;
        }

        // ── EX-stage branch resolution ──
        if new_ex_ma.valid && new_ex_ma.branch_taken {
            let s = new_ex_ma.thread as usize;
            self.threads[s].pc = new_ex_ma.branch_target;
        }

        // ── Interrupt delivery at the picked thread's fetch slot ──
        // (shared system unit: mip & mie, mstatus / mode, mideleg). Taken
        // only when none of the thread's instructions are in flight, so
        // mepc (= its next fetch pc) is exact and no older branch / trap
        // can overwrite the handler pc. While some are, the thread's fetch
        // slot becomes a bubble so it drains (a lone thread would
        // otherwise never be idle).
        if let Some(slot) = picked {
            if !stall && new_lt_if.valid && self.threads[slot as usize].sys.irq_would_take().is_some() {
                let mine = |v: bool, t: u8| v && t == slot;
                let in_flight = mine(new_if_di.valid, new_if_di.thread)
                    || mine(new_di_ex.valid, new_di_ex.thread)
                    || mine(new_ex_ma.valid, new_ex_ma.thread)
                    || mine(new_ma_wb.valid, new_ma_wb.thread);
                if in_flight {
                    new_lt_if = LtIf::bubble();
                } else if let Some(pc) = self.take_interrupt(slot) {
                    new_lt_if.pc = pc;
                }
            }
        }

        // ── Latch ────────────────────────────────────────────────
        self.lt_if = new_lt_if;
        if stall {
            // Freeze if_di (don't take new_if_di), bubble di_ex.
            // The consumer instruction stays in if_di and gets
            // re-decoded next cycle, by which time the load value
            // is in ma_wb and forwarding will deliver it.
            self.di_ex = DiEx::bubble();
            // self.if_di remains as prev_if_di (we just don't
            // overwrite it).
        } else {
            self.if_di = new_if_di;
            self.di_ex = new_di_ex;
        }
        self.ex_ma = new_ex_ma;
        self.ma_wb = new_ma_wb;

        // YIELD pulse is one-shot: any thread that had its
        // yield_pulse set entering this cycle has now had its
        // chance skipped (LT either picked someone else or there
        // was no other runnable thread). Clear the flag so the
        // thread is runnable next cycle.
        for t in self.threads.iter_mut() {
            t.yield_pulse = false;
        }

        self.cycle_count += 1;
    }

    pub fn reset(&mut self) {
        let was_imem = std::mem::take(&mut self.ext_imem);
        *self = Self::default();
        self.ext_imem = was_imem;
        // Re-zero ext_dmem in default(); ext_imem is preserved.
    }

    /// Advance the bus arbiter by one cycle. If the bus is busy,
    /// decrement its timer (and complete the operation when the
    /// timer hits zero). If the bus is idle, scan for the highest-
    /// priority pending request and start it.
    fn bus_tick(&mut self) {
        use bus::{Bus, BusState, MISS_LATENCY};

        // ── If bus is busy, advance the timer ──
        match self.bus.state {
            BusState::Idle => {}
            BusState::FillingI { line_addr, thread, cycles_remaining } => {
                if cycles_remaining > 1 {
                    self.bus.state = BusState::FillingI {
                        line_addr, thread,
                        cycles_remaining: cycles_remaining - 1,
                    };
                    return;
                }
                // Fill completes this cycle.
                let mut data = [0u32; cache::CACHE_LINE_WORDS];
                for w in 0..cache::CACHE_LINE_WORDS {
                    data[w] = self.imem_read(line_addr + (w as u32) * 4);
                }
                self.icache.install(line_addr, data);
                self.icache.pending = None;
                // Wake all waiters (the primary issuer + any
                // secondary stallers waiting on the I-cache).
                for s in 0..N_THREADS {
                    if self.icache.waiters[s] {
                        self.threads[s].miss_pending = false;
                        self.icache.waiters[s] = false;
                    }
                }
                let _ = thread; // (already woken via waiters)
                self.bus.state = BusState::Idle;
            }
            BusState::FillingD { line_addr, thread, cycles_remaining } => {
                if cycles_remaining > 1 {
                    self.bus.state = BusState::FillingD {
                        line_addr, thread,
                        cycles_remaining: cycles_remaining - 1,
                    };
                    return;
                }
                let mut data = [0u32; cache::CACHE_LINE_WORDS];
                for w in 0..cache::CACHE_LINE_WORDS {
                    data[w] = self.dmem_read(line_addr + (w as u32) * 4);
                }
                self.dcache.install(line_addr, data);
                self.dcache.pending = None;
                for s in 0..N_THREADS {
                    if self.dcache.waiters[s] {
                        self.threads[s].miss_pending = false;
                        self.dcache.waiters[s] = false;
                    }
                }
                let _ = thread;
                self.bus.state = BusState::Idle;
            }
            BusState::Storing { addr, value } => {
                self.dmem_write(addr, value);
                self.bus.state = BusState::Idle;
            }
        }

        // ── If bus is now idle, pick next op (priority order) ──
        if !matches!(self.bus.state, BusState::Idle) { return; }

        // 1. D-L1 read miss
        if let Some(p) = self.dcache.pending {
            self.bus.state = BusState::FillingD {
                line_addr: p.line_addr,
                thread: p.thread,
                cycles_remaining: MISS_LATENCY,
            };
            return;
        }
        // 2. I-L1 read miss
        if let Some(p) = self.icache.pending {
            self.bus.state = BusState::FillingI {
                line_addr: p.line_addr,
                thread: p.thread,
                cycles_remaining: MISS_LATENCY,
            };
            return;
        }
        // 3. Store buffer drain
        if !self.store_buffer.is_empty() {
            if let Some(e) = self.store_buffer.pop_front() {
                self.bus.state = BusState::Storing {
                    addr: e.addr,
                    value: e.value,
                };
            }
        }
        // (The Bus and unused params are kept for later wasm export.)
        let _ = Bus::new;
    }
}
