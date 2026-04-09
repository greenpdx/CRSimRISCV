//! Stage 5 — Memory Access.
//!
//! Phase B1: direct external data memory access (no D-L1 cache, no
//! store buffer). Loads do sign/zero extension based on funct3.

use super::cache::{Cache, PendingMiss};
use super::state::{BarrelState, CustomOp, ExMa, MaWb, N_THREADS, Thread};

pub fn compute_ma(state: &mut BarrelState, ex_ma: &ExMa) -> MaWb {
    if !ex_ma.valid {
        return MaWb::bubble();
    }

    // ── Custom-0 dispatch ─────────────────────────────────────
    if ex_ma.ctrl.custom_op != CustomOp::None {
        return execute_custom0(state, ex_ma);
    }

    let addr = ex_ma.alu_result;
    let funct3 = ex_ma.funct3;
    let thread = ex_ma.thread;

    // ── Loads ─────────────────────────────────────────────────
    if ex_ma.ctrl.mem_read {
        // 1. Read-after-write forwarding from store buffer.
        if let Some(v) = state.store_buffer.forward(addr) {
            let mem_data = sign_extend_load(v, addr, funct3);
            return MaWb {
                valid: true, thread, instr: ex_ma.instr,
                alu_result: ex_ma.alu_result, mem_data,
                pc_plus4: ex_ma.pc_plus4, rd_num: ex_ma.rd_num,
                ctrl: ex_ma.ctrl,
            };
        }
        // 2. D-L1 lookup.
        if let Some(raw) = state.dcache.lookup(addr) {
            let mem_data = sign_extend_load(raw, addr, funct3);
            return MaWb {
                valid: true, thread, instr: ex_ma.instr,
                alu_result: ex_ma.alu_result, mem_data,
                pc_plus4: ex_ma.pc_plus4, rd_num: ex_ma.rd_num,
                ctrl: ex_ma.ctrl,
            };
        }
        // 3. Miss. Issue or queue, stall this thread, return bubble.
        // Roll back the thread's pc by 4 because IF already
        // advanced it after fetching this instruction; without the
        // rollback, when the miss completes and the thread is
        // re-dispatched, it would skip the missing load entirely.
        let line_addr = Cache::line_addr(addr);
        if state.dcache.pending.is_none() {
            state.dcache.pending = Some(PendingMiss { line_addr, thread });
        }
        state.dcache.waiters[thread as usize] = true;
        state.threads[thread as usize].miss_pending = true;
        state.threads[thread as usize].pc =
            state.threads[thread as usize].pc.wrapping_sub(4);
        return MaWb::bubble();
    }

    // ── Stores ────────────────────────────────────────────────
    if ex_ma.ctrl.mem_write {
        // Update D-L1 if the line is currently cached (write
        // through, no write-allocate on miss).
        if let Some(line) = state.dcache.peek_line_mut(addr) {
            line.data[Cache::word_offset(addr)] = ex_ma.rs2_data;
        }
        // Push to store buffer; if full, stall this thread by
        // rolling back its pc so it re-issues the store later.
        if !state.store_buffer.push(addr, ex_ma.rs2_data, thread) {
            state.threads[thread as usize].pc =
                state.threads[thread as usize].pc.wrapping_sub(4);
            return MaWb::bubble();
        }
    }

    MaWb {
        valid: true,
        thread,
        instr: ex_ma.instr,
        alu_result: ex_ma.alu_result,
        mem_data: 0,
        pc_plus4: ex_ma.pc_plus4,
        rd_num: ex_ma.rd_num,
        ctrl: ex_ma.ctrl,
    }
}

fn sign_extend_load(raw: u32, addr: u32, funct3: u8) -> u32 {
    let byte_off = (addr & 0x3) * 8;
    match funct3 {
        0b000 => {
            let v = (raw >> byte_off) & 0xff;
            if v & 0x80 != 0 { v | 0xffff_ff00 } else { v }
        }
        0b001 => {
            let v = (raw >> byte_off) & 0xffff;
            if v & 0x8000 != 0 { v | 0xffff_0000 } else { v }
        }
        0b010 => raw,
        0b100 => (raw >> byte_off) & 0xff,
        0b101 => (raw >> byte_off) & 0xffff,
        _     => raw,
    }
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
                    let mut child = Thread::idle();
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
                let target_state = state.threads[target];
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
