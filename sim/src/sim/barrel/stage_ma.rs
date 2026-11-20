//! Stage 5 — Memory Access.
//!
//! Phase B1: direct external data memory access (no D-L1 cache, no
//! store buffer). Loads do sign/zero extension based on funct3.

use crate::data::load_width::sign_extend_load;
use super::cache::{Cache, PendingMiss};
use super::state::{BarrelState, CustomOp, ExMa, MaWb, N_THREADS, Thread};
use super::thread_mmio;

/// Build a MaWb from an ExMa with a given mem_data (helper to
/// avoid repeating the 8-field construction everywhere).
fn mawb_from(ex_ma: &ExMa, mem_data: u32) -> MaWb {
    MaWb {
        valid: true, thread: ex_ma.thread, instr: ex_ma.instr,
        alu_result: ex_ma.alu_result, mem_data,
        pc_plus4: ex_ma.pc_plus4, rd_num: ex_ma.rd_num,
        ctrl: ex_ma.ctrl,
    }
}

fn execute_csr(state: &mut BarrelState, ex_ma: &ExMa) -> MaWb {
    let slot = ex_ma.thread;
    // Shared Zicsr unit (the shared CSRs are mirrored to every thread)
    let old = state.csr_rmw(slot, ex_ma.csr_addr, ex_ma.csr_op, ex_ma.csr_src_is_zero, ex_ma.csr_write_val);
    let mut r = mawb_from(ex_ma, 0);
    r.alu_result = old; // old CSR value → WB writes to rd
    r
}

fn execute_load(state: &mut BarrelState, ex_ma: &ExMa) -> MaWb {
    let addr = ex_ma.alu_result;
    let funct3 = ex_ma.funct3;
    let thread = ex_ma.thread;

    // MMIO: thread state, IRQ controller, DMA
    if thread_mmio::is_tmmio(addr) {
        let t = &state.threads[thread as usize];
        if !thread_mmio::check_access(thread, t.priv_bits()) {
            state.take_sync_trap(thread, 5, ex_ma.trap_pc, addr);
            return MaWb::bubble();
        }
        return mawb_from(ex_ma, thread_mmio::read(state, addr));
    }
    if let Some(v) = state.irq.read_reg(addr) { return mawb_from(ex_ma, v); }
    if let Some(v) = state.dma.read_reg(addr)  { return mawb_from(ex_ma, v); }

    // Store-buffer forwarding
    if let Some(v) = state.store_buffer.forward(addr) {
        return mawb_from(ex_ma, sign_extend_load(v, addr, funct3));
    }
    // D-L1 cache
    if let Some(raw) = state.dcache.lookup(addr) {
        return mawb_from(ex_ma, sign_extend_load(raw, addr, funct3));
    }
    // Cache miss — stall + rollback
    let line_addr = Cache::line_addr(addr);
    if state.dcache.pending.is_none() {
        state.dcache.pending = Some(PendingMiss { line_addr, thread });
    }
    state.dcache.waiters[thread as usize] = true;
    state.threads[thread as usize].miss_pending = true;
    state.threads[thread as usize].pc =
        state.threads[thread as usize].pc.wrapping_sub(4);
    MaWb::bubble()
}

fn execute_store(state: &mut BarrelState, ex_ma: &ExMa) -> MaWb {
    let addr = ex_ma.alu_result;
    let thread = ex_ma.thread;
    let val = ex_ma.rs2_data;

    // Thread MMIO
    if thread_mmio::is_tmmio(addr) {
        let priv_m = state.threads[thread as usize].priv_bits();
        if !thread_mmio::check_access(thread, priv_m) {
            state.take_sync_trap(thread, 7, ex_ma.trap_pc, addr);
            return MaWb::bubble();
        }
        thread_mmio::write(state, addr, val);
        return mawb_from(ex_ma, 0);
    }
    // IRQ controller
    if state.irq.write_reg(addr, val) { return mawb_from(ex_ma, 0); }
    // DMA
    let is_dma = state.dma.write_reg(addr, val, &state.ext_imem, &mut state.local_isram);
    if !is_dma {
        // D-L1 write-through
        if let Some(line) = state.dcache.peek_line_mut(addr) {
            line.data[Cache::word_offset(addr)] = val;
        }
        // Store buffer
        if !state.store_buffer.push(addr, val, thread) {
            state.threads[thread as usize].pc =
                state.threads[thread as usize].pc.wrapping_sub(4);
            return MaWb::bubble();
        }
    }
    mawb_from(ex_ma, 0)
}

pub fn compute_ma(state: &mut BarrelState, ex_ma: &ExMa) -> MaWb {
    if !ex_ma.valid { return MaWb::bubble(); }
    if ex_ma.ctrl.custom_op != CustomOp::None { return execute_custom0(state, ex_ma); }
    if ex_ma.csr_op != 0 { return execute_csr(state, ex_ma); }
    if ex_ma.ctrl.mem_read { return execute_load(state, ex_ma); }
    if ex_ma.ctrl.mem_write { return execute_store(state, ex_ma); }
    mawb_from(ex_ma, 0)
}

/// Execute one custom-0 instruction in the MA stage. The issuer is
/// `ex_ma.thread`. Returns a MaWb that, for FORK and TID, contains
/// the result the WB stage should write to rd; for the others
/// `reg_write` is false so WB is a no-op.
fn execute_custom0(state: &mut BarrelState, ex_ma: &ExMa) -> MaWb {
    let issuer = ex_ma.thread as usize;
    let mut result_for_rd: u32 = 0;

    match ex_ma.ctrl.custom_op {
        CustomOp::Fork => {
            // FORK rd, rs1, rs2 — rs1 = start_pc, rs2 = arg.
            // Find lowest free slot != issuer; set up its context;
            // return slot ID (or u32::MAX = -1 if no free slot).
            let start_pc = ex_ma.rs1_data;
            let arg      = ex_ma.rs2_data;
            let parent_sp = state.threads[issuer].regs[2];
            let mut allocated: i32 = -1;
            for s in 0..N_THREADS {
                if s == issuer { continue; }
                let t = &state.threads[s];
                if !t.active && t.waiting < 0 {
                    // fresh hart (mhartid = s) with the shared CSRs
                    let mut child = Thread::idle_hart(s as u8);
                    for a in super::state::SHARED_CSRS {
                        child.sys.csrs.write(a, state.threads[issuer].sys.csrs.read(a));
                    }
                    child.pc = start_pc;
                    child.active = true;
                    child.priority = 1;
                    child.regs[10] = arg;       // a0
                    child.regs[2]  = parent_sp; // sp inherited from parent
                    state.threads[s] = child;
                    allocated = s as i32;
                    break;
                }
            }
            result_for_rd = allocated as u32;
        }

        CustomOp::Join => {
            // JOIN rs1 — block self until thread rs1 is no longer active.
            // If target is already inactive (and not itself joined),
            // JOIN is a no-op (returns immediately, 1 cycle).
            let target = ex_ma.rs1_data as usize;
            if target < N_THREADS {
                let target_state = &state.threads[target];
                let already_done = !target_state.active && target_state.waiting < 0;
                if !already_done {
                    state.threads[issuer].waiting = target as i8;
                    state.threads[issuer].active  = false;
                }
            }
        }

        CustomOp::Done => {
            // DONE — wake any thread JOIN-blocked on issuer; deactivate self.
            for s in 0..N_THREADS {
                if state.threads[s].waiting == issuer as i8 {
                    state.threads[s].waiting = -1;
                    state.threads[s].active  = true;
                }
            }
            state.threads[issuer].active = false;
        }

        CustomOp::Yield => {
            // YIELD — set the one-shot pulse; scheduler will skip
            // this slot for one round, then the pulse is cleared
            // by step() at end of cycle.
            state.threads[issuer].yield_pulse = true;
        }

        CustomOp::Tid => {
            // TID rd — write current TID to rd.
            result_for_rd = issuer as u32;
        }

        CustomOp::Forkret => {
            // FORKRET rd — like DONE but copy regs[issuer][rd] into
            // each waker's a0 (x10) at wakeup.
            let return_value = ex_ma.rs1_data; // pre-loaded by stage_di
            for s in 0..N_THREADS {
                if state.threads[s].waiting == issuer as i8 {
                    state.threads[s].regs[10] = return_value;
                    state.threads[s].waiting  = -1;
                    state.threads[s].active   = true;
                }
            }
            state.threads[issuer].active = false;
        }

        CustomOp::None => unreachable!(),
    }

    MaWb {
        valid: true,
        thread: ex_ma.thread,
        instr: ex_ma.instr,
        alu_result: result_for_rd,
        mem_data: 0,
        pc_plus4: ex_ma.pc_plus4,
        rd_num: ex_ma.rd_num,
        ctrl: ex_ma.ctrl,
    }
}
