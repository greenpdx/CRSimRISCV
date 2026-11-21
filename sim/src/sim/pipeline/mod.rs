//! 5-stage pipelined RV32I simulator.
//!
//! See `sim/docs/pipeline-plan.md` for the full design discussion.
//!
//! The simulator is split across one file per stage:
//!   * `state.rs`     — pipeline-register structs and `PipelineState`
//!   * `stage_if.rs`  — Instruction Fetch
//!   * `stage_id.rs`  — Decode + register read + hazard detection
//!   * `stage_ex.rs`  — Execute + forwarding + branch resolution
//!   * `stage_ma.rs`  — Memory access
//!   * `stage_wb.rs`  — Write back
//!
//! `PipelineState::step()` walks all five stages **in reverse order**
//! (WB first, then MA, then EX, then ID, then IF) so that earlier
//! stages see the *new* register values written back by later stages
//! this cycle. This is exactly how real hardware works: writes happen
//! on the rising clock edge, but the combinational logic for stage N
//! reads its pipeline-register input as latched on the previous edge.

pub mod state;
pub mod csr;
pub mod tlb;
pub mod translate;
pub mod stage_if;
pub mod stage_id;
pub mod stage_ex;
pub mod stage_ma;
pub mod stage_wb;

use crate::sim::system::addr;

pub use state::{
    BranchMode, ControlBits, ExMa, ForwardSrc, IdEx, IfId, MaWb, PipelineState,
};

/// The five pipeline stages, in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage { IF, ID, EX, MA, WB }

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::IF => "IF",
            Stage::ID => "ID",
            Stage::EX => "EX",
            Stage::MA => "MA",
            Stage::WB => "WB",
        }
    }
    pub const ALL: [Stage; 5] = [Stage::IF, Stage::ID, Stage::EX, Stage::MA, Stage::WB];
}

/// Compute the set of active components in a given pipeline stage,
/// based on what's currently in the corresponding pipeline register.
/// Mirrors the matrix in `sim/docs/pipeline-plan.md` §4.
pub fn active_components(state: &PipelineState, stage: Stage) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    match stage {
        Stage::IF => {
            // IF runs every cycle (the PC drives ROM unconditionally).
            v.extend_from_slice(&["pc", "rom", "pc_adder", "if_id_reg"]);
        }
        Stage::ID => {
            v.extend_from_slice(&["decoder", "imm_gen", "regfile_read", "id_ex_reg"]);
            if state.last_stall { v.push("hazard_unit"); }
        }
        Stage::EX => {
            if state.id_ex.valid {
                v.extend_from_slice(&[
                    "forward_unit", "mux_fwd_a", "mux_fwd_b",
                    "alu_ctrl", "alu", "ex_ma_reg",
                ]);
                if state.id_ex.ctrl.alu_src { v.push("mux_alusrc"); }
                if state.id_ex.ctrl.branch || state.id_ex.ctrl.jump {
                    v.push("branch_adder");
                    v.push("branch_decide");
                }
            } else {
                v.push("ex_ma_reg");
            }
        }
        Stage::MA => {
            v.push("ma_wb_reg");
            if state.ex_ma.valid {
                if state.ex_ma.ctrl.mem_read || state.ex_ma.ctrl.mem_write {
                    v.push("loadstore");
                    let addr = state.ex_ma.alu_result;
                    if PipelineState::is_io(addr) { v.push("io"); } else { v.push("ram"); }
                }
            }
        }
        Stage::WB => {
            // The WB stage processed `last_wb` this cycle (now-stale data
            // since the regfile has already been updated).
            if state.last_wb.valid && state.last_wb.ctrl.reg_write && state.last_wb.rd_num != 0 {
                v.push("mux_wb");
                v.push("regfile_write");
            }
        }
    }
    v
}

/// Branch-mode flags computed per cycle, determining which latch
/// path to take. Extracted from step() for readability.
struct BranchFlags {
    need_eager_flush: bool,
    need_misspec_fix: bool,
    need_match_squash: bool,
    do_id_redirect: bool,
    flush_visible: bool,
}

impl PipelineState {
    /// Advance the simulator by one clock cycle.
    pub fn step(&mut self) {
        if self.halted { return; }
        // Memory-panel mode: frozen while an earlier access is still in
        // flight; otherwise run the cycle and freeze for the rest of the
        // slower side's latency.
        let hier = self.hier.clone();
        if let Some(h) = &hier {
            if h.sys.borrow_mut().core_begin(h.core as usize) {
                self.cycle_count += 1;
                return;
            }
        }
        self.step_cycle();
        if let Some(h) = &hier { h.sys.borrow_mut().core_end(h.core as usize); }
    }

    /// One clock of the pipeline (the body of `step`).
    fn step_cycle(&mut self) {
        self.clear_per_cycle_state();
        // cycle counter CSR = clocks completed so far
        self.sys.csrs.set_counter(addr::CYCLE, self.cycle_count as u32);

        // ── 0. Interrupt check ──────────────────────────────────
        self.last_trap_taken = false;
        if self.check_and_take_interrupt() {
            self.last_trap_taken = true;
        }

        // ── Snapshot previous-cycle pipeline registers ──────────
        let prev_if_id = self.if_id;
        let prev_id_ex = self.id_ex;
        let prev_ex_ma = self.ex_ma;
        let prev_ma_wb = self.ma_wb;
        self.last_wb = prev_ma_wb;

        // ── Run stages in reverse order (WB → IF) ──────────────
        stage_wb::compute_wb(self, &prev_ma_wb);
        if prev_ma_wb.valid { self.sys.csrs.bump(addr::INSTRET); }
        let new_ma_wb = stage_ma::compute_ma(self, &prev_ex_ma);
        let ex_result = stage_ex::compute_ex(&prev_id_ex, &prev_ex_ma, &prev_ma_wb);
        let new_ex_ma = ex_result.ex_ma;
        self.last_forward_a = ex_result.forward_a;
        self.last_forward_b = ex_result.forward_b;

        let stall = stage_id::detect_load_use_hazard(&prev_if_id, &prev_id_ex);
        self.last_stall = stall;
        self.last_stall_reg = if stall { prev_id_ex.rd_num } else { 0 };
        let new_id_ex = stage_id::compute_id(self, &prev_if_id)
            .unwrap_or_else(IdEx::bubble);
        self.record_reg_reads(&new_id_ex);
        let new_if_id = stage_if::compute_if(self);

        // ── Resolve traps + compute branch flags + latch ────────
        if self.resolve_ma_trap(&new_ma_wb) { return; }
        let mut new_ex_ma = new_ex_ma;
        let trap_redirect = self.resolve_ex_trap(&mut new_ex_ma);
        let bf = self.compute_branch_flags(
            &new_ex_ma, &new_id_ex, stall, trap_redirect.is_some(),
        );
        self.last_flush = bf.flush_visible || trap_redirect.is_some();
        self.latch(
            new_ma_wb, new_ex_ma, new_id_ex, new_if_id,
            stall, &prev_id_ex, trap_redirect, &bf,
        );
        self.cycle_count += 1;
    }

    // ── Private helpers for step() ──────────────────────────────

    fn clear_per_cycle_state(&mut self) {
        for (i, &a) in csr::known_addrs().iter().enumerate() {
            self.last_csr_snapshot[i] = self.sys.csrs.read(a);
        }
        self.reg_read_mask = 0;
        self.reg_write_mask = 0;
        self.last_ram_addr = 0;
        self.last_ram_kind = 0;
    }

    fn record_reg_reads(&mut self, id_ex: &IdEx) {
        if id_ex.valid {
            if id_ex.rs1_num != 0 { self.reg_read_mask |= 1 << id_ex.rs1_num; }
            if id_ex.rs2_num != 0 { self.reg_read_mask |= 1 << id_ex.rs2_num; }
        }
    }

    /// Handle MA-stage page fault. Returns true if the trap was
    /// taken (caller should return early from step).
    fn resolve_ma_trap(&mut self, new_ma_wb: &MaWb) -> bool {
        if new_ma_wb.trap_kind == 0 { return false; }
        let cause: u32 = if new_ma_wb.trap_kind == 7 { 13 } else { 15 };
        let target = self.take_trap(cause, new_ma_wb.trap_pc, new_ma_wb.trap_value);
        self.last_flush = true;
        self.ma_wb = MaWb::bubble();
        self.ex_ma = ExMa::bubble();
        self.id_ex = IdEx::bubble();
        self.if_id = IfId::bubble();
        self.pc = target;
        self.cycle_count += 1;
        true
    }

    /// Handle EX-stage sync trap or mret. Defangs the trapping
    /// instruction and returns the redirect PC (or None).
    fn resolve_ex_trap(&mut self, ex_ma: &mut ExMa) -> Option<u32> {
        if !ex_ma.valid || ex_ma.trap_kind == 0 { return None; }
        let redirect = match ex_ma.trap_kind {
            4 => self.sys.xret(false),
            5 => self.sys.xret(true),
            kind => {
                let cause: u32 = match kind {
                    1 => 2,
                    2 => self.sys.ecall_cause(),
                    3 => 3,
                    6 => 12,
                    _ => 0,
                };
                let mtval: u32 = match kind {
                    1 => ex_ma.instr, 6 => ex_ma.trap_pc, _ => 0,
                };
                self.take_trap(cause, ex_ma.trap_pc, mtval)
            }
        };
        // Defang: keep entry alive for the visualizer but suppress
        // all side effects so MA/WB treat it as a no-op.
        ex_ma.ctrl.reg_write = false;
        ex_ma.ctrl.mem_read  = false;
        ex_ma.ctrl.mem_write = false;
        ex_ma.ctrl.csr_op    = 0;
        ex_ma.ctrl.is_sfence = false;
        ex_ma.rd_num         = 0;
        ex_ma.trap_kind      = 0;
        Some(redirect)
    }

    fn compute_branch_flags(
        &self, ex_ma: &ExMa, id_ex: &IdEx, stall: bool, has_trap: bool,
    ) -> BranchFlags {
        let mode = self.branch_mode;
        let adv = matches!(mode, BranchMode::AlwaysTarget | BranchMode::EarlyResolve | BranchMode::Predict);
        let ex_br = ex_ma.valid && (ex_ma.ctrl.branch || ex_ma.ctrl.jump);
        let spec = adv && ex_br && ex_ma.early_redirected;
        let eager = ex_br && ex_ma.branch_taken && !spec;
        let misspec = adv && spec && !ex_ma.branch_taken;
        let matched = adv && spec && ex_ma.branch_taken;
        let id_redir = adv && id_ex.early_branch && !stall && !has_trap && !eager;
        BranchFlags {
            need_eager_flush: eager,
            need_misspec_fix: misspec,
            need_match_squash: matched,
            do_id_redirect: id_redir,
            flush_visible: eager || misspec || matched || id_redir,
        }
    }

    /// Latch new pipeline register values based on the resolved
    /// trap, stall, branch, or normal-advance path.
    #[allow(clippy::too_many_arguments)]
    fn latch(
        &mut self,
        new_ma_wb: MaWb, new_ex_ma: ExMa,
        new_id_ex: IdEx, new_if_id: IfId,
        stall: bool, prev_id_ex: &IdEx,
        trap_redirect: Option<u32>, bf: &BranchFlags,
    ) {
        self.ma_wb = new_ma_wb;
        self.ex_ma = new_ex_ma;

        if let Some(target) = trap_redirect {
            self.if_id = IfId::bubble();
            self.id_ex = IdEx::bubble();
            self.pc = target;
        } else if stall {
            let mut bubble = IdEx::bubble();
            bubble.stall_reg = prev_id_ex.rd_num;
            self.id_ex = bubble;
        } else if bf.need_eager_flush {
            let mut if_b = IfId::bubble(); if_b.flush_bubble = true;
            let mut id_b = IdEx::bubble(); id_b.flush_bubble = true;
            self.if_id = if_b;
            self.id_ex = id_b;
            self.pc = new_ex_ma.branch_target;
        } else if bf.need_misspec_fix {
            let mut id_b = IdEx::bubble(); id_b.flush_bubble = true;
            let mut if_b = IfId::bubble(); if_b.flush_bubble = true;
            self.id_ex = id_b;
            self.if_id = if_b;
            self.pc = new_ex_ma.pc_plus4;
        } else if bf.need_match_squash {
            let mut id_b = IdEx::bubble(); id_b.flush_bubble = true;
            self.id_ex = id_b;
            self.if_id = new_if_id;
            self.pc = self.pc.wrapping_add(4);
        } else if bf.do_id_redirect {
            let mut idex = new_id_ex;
            idex.id_redirected = true;
            self.id_ex = idex;
            self.if_id = new_if_id;
            self.pc = new_id_ex.early_target;
        } else {
            self.id_ex = new_id_ex;
            self.if_id = new_if_id;
            self.pc = self.pc.wrapping_add(4);
        }
    }

    /// Take a synchronous trap through the shared system unit (delegation,
    /// mstatus stacking, xcause / xepc / xtval). Returns the handler PC.
    fn take_trap(&mut self, cause: u32, epc: u32, tval: u32) -> u32 {
        self.sys.trap(cause, false, epc, tval, self.cycle_count)
    }

    /// Asynchronous interrupt check, called at the top of `step()`: the
    /// shared system unit merges the external lines into mip and picks the
    /// highest-priority enabled interrupt (M and S, mideleg). If one is
    /// taken, PC is redirected to the handler. In-flight instructions are
    /// not squashed — they drain behind it; `mepc` is the address that was
    /// about to be fetched, so `mret` resumes exactly there.
    fn check_and_take_interrupt(&mut self) -> bool {
        let Some(cause) = self.sys.sample_irq() else { return false };
        let epc = self.pc;
        self.pc = self.sys.trap(cause, true, epc, 0, self.cycle_count);
        true
    }

    /// Reset the pipeline to its initial state (program preserved).
    pub fn reset(&mut self) {
        self.pc = 0;
        self.registers = [0; 32];
        for word in self.machine_ram.iter_mut() { *word = 0; }
        for word in self.user_ram.iter_mut() { *word = 0; }
        self.io_data = 0;
        self.reg_read_mask = 0;
        self.reg_write_mask = 0;
        self.last_ram_addr = 0;
        self.last_ram_kind = 0;
        self.last_ram_side = 0;
        self.sys = state::pipeline_sys();
        self.flush_tlbs();
        self.cycle_count = 0;
        self.halted = false;
        self.if_id = IfId::default();
        self.id_ex = IdEx::default();
        self.ex_ma = ExMa::default();
        self.ma_wb = MaWb::default();
        self.last_wb = MaWb::default();
        self.last_stall = false;
        self.last_flush = false;
        self.last_forward_a = ForwardSrc::Regfile;
        self.last_forward_b = ForwardSrc::Regfile;
        self.last_trap_taken = false;
        self.last_stall_reg = 0;
    }
}
