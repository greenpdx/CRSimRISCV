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

impl PipelineState {
    /// Advance the simulator by one clock cycle.
    pub fn step(&mut self) {
        if self.halted { return; }

        // Snapshot CSRs *before* this cycle's stages run, so the
        // visualizer can compare and highlight changed CSRs.
        for (i, &a) in csr::CsrFile::known_addrs().iter().enumerate() {
            self.last_csr_snapshot[i] = self.csrs.read(a);
        }
        // Clear per-cycle activity records (they reflect this cycle).
        self.reg_read_mask = 0;
        self.reg_write_mask = 0;
        self.last_ram_addr = 0;
        self.last_ram_kind = 0;

        // ── 0. Interrupt check (asynchronous trap) ─────────────────
        // Async interrupts redirect PC to mtvec but do NOT squash any
        // in-flight stages. The instructions currently in IF/ID/EX/
        // MA/WB drain to completion behind the handler. The IF stage
        // this very cycle will fetch from mtvec (because PC was just
        // overwritten), so the handler starts immediately while older
        // in-flight instructions retire on subsequent cycles.
        self.last_trap_taken = false;
        if self.check_and_take_interrupt() {
            self.last_trap_taken = true;
        }

        // Snapshot the inputs each stage will use. We're computing the
        // *new* register values from the *current* (previous-cycle)
        // register values, then latching them all atomically.
        let prev_if_id = self.if_id;
        let prev_id_ex = self.id_ex;
        let prev_ex_ma = self.ex_ma;
        let prev_ma_wb = self.ma_wb;

        // Record the instruction WB is consuming this cycle so the
        // visualizer can show it (the value will be gone after WB runs).
        self.last_wb = prev_ma_wb;

        // ── 1. WB ───────────────────────────────────────────────────
        // Write the value in MA/WB into the regfile (if applicable).
        // Doing this first means the ID stage's regfile read in this
        // same cycle will see the new value (write-then-read in one
        // cycle, the standard split-cycle regfile trick).
        stage_wb::compute_wb(self, &prev_ma_wb);

        // ── 2. MA ───────────────────────────────────────────────────
        // Read RAM/IO if loading; write RAM/IO if storing.
        let new_ma_wb = stage_ma::compute_ma(self, &prev_ex_ma);

        // ── 3. EX ───────────────────────────────────────────────────
        // ALU + forwarding + branch resolution. We pass the *current*
        // EX/MA and MA/WB as forwarding sources because that matches
        // what the hardware sees: the values that were latched on the
        // previous edge are still on the wires at the start of this
        // cycle.
        let ex_result = stage_ex::compute_ex(&prev_id_ex, &prev_ex_ma, &prev_ma_wb);
        let new_ex_ma = ex_result.ex_ma;
        self.last_forward_a = ex_result.forward_a;
        self.last_forward_b = ex_result.forward_b;

        // ── 4. ID ───────────────────────────────────────────────────
        // Decode the instruction in IF/ID, read the regfile, generate
        // the immediate. Detect a load-use hazard against the
        // instruction currently in ID/EX (the one EX is processing
        // right now).
        let stall = stage_id::detect_load_use_hazard(&prev_if_id, &prev_id_ex);
        self.last_stall = stall;
        self.last_stall_reg = if stall { prev_id_ex.rd_num } else { 0 };
        let new_id_ex_default = stage_id::compute_id(self, &prev_if_id)
            .unwrap_or_else(IdEx::bubble);
        // Record this cycle's register reads (rs1/rs2 of new ID/EX).
        if new_id_ex_default.valid {
            if new_id_ex_default.rs1_num != 0 {
                self.reg_read_mask |= 1 << new_id_ex_default.rs1_num;
            }
            if new_id_ex_default.rs2_num != 0 {
                self.reg_read_mask |= 1 << new_id_ex_default.rs2_num;
            }
        }

        // ── 5. IF ───────────────────────────────────────────────────
        // Read the next instruction from ROM at PC.
        let new_if_id_default = stage_if::compute_if(self);

        // ── 5b. MA-stage page-fault resolution ──────────────────────
        // If MA detected a load/store page fault, the instruction in
        // MA traps NOW. The instructions currently in EX, ID, and IF
        // are all younger and must be squashed. The MaWb result must
        // not be latched (its rd_num was already cleared, but bubble
        // it for clarity).
        if new_ma_wb.trap_kind != 0 {
            let cause: u32 = if new_ma_wb.trap_kind == 7 { 13 } else { 15 };
            let target = self.take_trap(cause, new_ma_wb.trap_pc, new_ma_wb.trap_value);
            self.last_flush = true;
            self.ma_wb = MaWb::bubble();
            self.ex_ma = ExMa::bubble();
            self.id_ex = IdEx::bubble();
            self.if_id = IfId::bubble();
            self.pc = target;
            self.cycle_count += 1;
            return;
        }

        // ── 6a. Synchronous trap / mret resolution ──────────────────
        // The instruction in EX may be a sync trap (illegal/ECALL/
        // EBREAK) or an MRET. Both behave like a flush + redirect,
        // and update mstatus/mepc/mcause. The trapping/mret
        // instruction itself must NOT retire — squash new_ex_ma.
        let mut new_ex_ma = new_ex_ma;
        let mut trap_redirect: Option<u32> = None;
        if new_ex_ma.valid && new_ex_ma.trap_kind != 0 {
            match new_ex_ma.trap_kind {
                4 => { trap_redirect = Some(self.do_mret()); }
                5 => { trap_redirect = Some(self.do_sret()); }
                kind => { // 1=illegal, 2=ecall, 3=ebreak, 6=ifetch-pf
                    let cause: u32 = match kind {
                        1 => 2,
                        2 => match self.priv_mode { 3 => 11, 1 => 9, _ => 8 },
                        3 => 3,
                        6 => 12, // instruction page fault
                        _ => 0,
                    };
                    let mtval: u32 = match kind {
                        1 => new_ex_ma.instr,
                        6 => new_ex_ma.trap_pc, // faulting vaddr
                        _ => 0,
                    };
                    trap_redirect = Some(self.take_trap(cause, new_ex_ma.trap_pc, mtval));
                }
            }
            // The trapping instruction itself must not retire any
            // architectural side-effects, but we keep the pipeline-
            // register entry alive so the visualizer can show it
            // travel through MA and WB. Suppress reg/mem writes and
            // any CSR side-effect by clearing the relevant control
            // bits and zeroing rd.
            new_ex_ma.ctrl.reg_write = false;
            new_ex_ma.ctrl.mem_read  = false;
            new_ex_ma.ctrl.mem_write = false;
            new_ex_ma.ctrl.csr_op    = 0;
            new_ex_ma.ctrl.is_sfence = false;
            new_ex_ma.rd_num         = 0;
            // Clear trap_kind so the next cycle's MA-trap check
            // doesn't re-fire on this same instruction.
            new_ex_ma.trap_kind = 0;
        }

        // ── 6b. Branch flush logic (mode-aware) ────────────────────
        // Three branch-related events can happen in this cycle:
        //
        //   A. EX resolved a JALR-or-mode-1 branch this cycle
        //      (no ID-side speculation was done for it).
        //      → mode-1 style "squash IF and ID, redirect to ex_ma.branch_target"
        //
        //   B. EX resolved a branch that WAS speculated by ID one
        //      cycle ago (modes 2/3, conditional branch or JAL).
        //      → check if speculation matched; squash either id_ex
        //        (if speculation correct = taken) or if_id (if
        //        speculation wrong = not taken, redirect back).
        //
        //   C. ID just decoded a branch with early_branch set
        //      (modes 2/3, this cycle). Override pc to early target.
        //      The previously-fetched if_id stays put.
        //
        // Events A/B happen on a branch's MA-stage cycle (its EX
        // resolution one cycle ago); event C happens on a different
        // branch's ID-stage cycle. They are normally one cycle apart
        // and don't conflict.

        let mode = self.branch_mode;
        let modes_with_id_redirect = matches!(
            mode,
            BranchMode::AlwaysTarget | BranchMode::EarlyResolve | BranchMode::Predict
        );

        let ex_branch = new_ex_ma.valid
            && (new_ex_ma.ctrl.branch || new_ex_ma.ctrl.jump);
        // Only treat the branch as "ID-speculated" in modes that
        // actually perform the ID-side redirect. In Eager mode the
        // compute_id stage still sets the flag (it's mode-agnostic)
        // but the redirect never happened, so we must not skip the
        // EX-stage flush.
        let ex_was_speculated =
            modes_with_id_redirect && ex_branch && new_ex_ma.early_redirected;
        let need_eager_flush  = ex_branch && new_ex_ma.branch_taken && !ex_was_speculated;
        let need_misspec_fix  = modes_with_id_redirect && ex_was_speculated && !new_ex_ma.branch_taken;
        let need_match_squash = modes_with_id_redirect && ex_was_speculated && new_ex_ma.branch_taken;
        let do_id_redirect    = modes_with_id_redirect
            && new_id_ex_default.early_branch
            && !stall
            && trap_redirect.is_none()
            && !need_eager_flush;

        let flush_visible =
            need_eager_flush || need_misspec_fix || need_match_squash || do_id_redirect;
        self.last_flush = flush_visible || trap_redirect.is_some();

        // ── 7. Latch ────────────────────────────────────────────────
        self.ma_wb = new_ma_wb;
        self.ex_ma = new_ex_ma;

        if let Some(target) = trap_redirect {
            // Sync trap or mret: squash IF/ID and ID/EX, redirect.
            self.if_id = IfId::bubble();
            self.id_ex = IdEx::bubble();
            self.pc = target;
        } else if stall {
            // Freeze IF/ID and PC, insert a stall-tagged bubble.
            let mut bubble = IdEx::bubble();
            bubble.stall_reg = prev_id_ex.rd_num;
            self.id_ex = bubble;
            // self.if_id stays as it was (don't overwrite).
            // self.pc stays as it was.
        } else if need_eager_flush {
            // Mode 1 / JALR style: squash IF and ID, redirect.
            let mut if_b = IfId::bubble();
            if_b.flush_bubble = true;
            let mut id_b = IdEx::bubble();
            id_b.flush_bubble = true;
            self.if_id = if_b;
            self.id_ex = id_b;
            self.pc = new_ex_ma.branch_target;
        } else if need_misspec_fix {
            // Mode 2/3 misprediction: ID speculated taken but EX says
            // not taken. The branch's pc_plus4 (= the address right
            // after the branch) is the correct continuation. Squash
            // if_id (the speculatively-fetched target_instr) and
            // both id_ex (which may itself be a stale bubble from a
            // chained recovery) — let the pipeline refill cleanly.
            let mut id_b = IdEx::bubble();
            id_b.flush_bubble = true;
            self.id_ex = id_b;
            let mut bub = IfId::bubble();
            bub.flush_bubble = true;
            self.if_id = bub;
            self.pc = new_ex_ma.pc_plus4;
        } else if need_match_squash {
            // Mode 2/3 correct speculation: branch was taken as we
            // guessed. The target_instr in new_if_id_default is
            // correct (continue from target). Squash id_ex (the
            // seq1 from the not-taken path).
            let mut id_b = IdEx::bubble();
            id_b.flush_bubble = true;
            self.id_ex = id_b;
            self.if_id = new_if_id_default;
            self.pc = self.pc.wrapping_add(4);
        } else if do_id_redirect {
            // Mode 2/3: ID just decoded a branch this cycle. Latch
            // normally (id_ex = decoded branch, if_id = seq1 we
            // fetched at PC+4) and override pc so the next cycle's
            // IF fetches at the early target. Mark id_ex with
            // `id_redirected` so when this branch reaches EX, the
            // need_match_squash / need_misspec_fix logic recognizes
            // that an ID-side redirect actually occurred (and
            // doesn't fire on every random branch in id_ex).
            let mut idex = new_id_ex_default;
            idex.id_redirected = true;
            self.id_ex = idex;
            self.if_id = new_if_id_default;
            self.pc = new_id_ex_default.early_target;
        } else {
            // Normal advance.
            self.id_ex = new_id_ex_default;
            self.if_id = new_if_id_default;
            self.pc = self.pc.wrapping_add(4);
        }

        self.cycle_count += 1;
    }

    /// Take a synchronous trap (illegal/ECALL/EBREAK). Updates mepc,
    /// mcause, mtval, mstatus.{MIE,MPIE,MPP=M}. Returns the redirect
    /// PC (mtvec, direct mode).
    fn take_trap(&mut self, cause: u32, epc: u32, tval: u32) -> u32 {
        // Decide delegation: cause's low bits index medeleg (sync) or
        // mideleg (interrupts). Delegate only if current priv ≤ S.
        let is_int = (cause & 0x8000_0000) != 0;
        let bit = (cause & 0x1f) as u32;
        let deleg = if is_int { self.csrs.mideleg } else { self.csrs.medeleg };
        let to_s = self.priv_mode <= 1 && (deleg & (1 << bit)) != 0;

        if to_s {
            self.csrs.sepc   = epc & !1;
            self.csrs.scause = cause;
            self.csrs.stval  = tval;
            let m = self.csrs.mstatus;
            let sie = (m >> 1) & 1;
            // sstatus.SPIE ← SIE; SIE ← 0; SPP ← current priv (0 or 1)
            let mut nm = m & !((1 << 1) | (1 << 5) | (1 << 8));
            nm |= sie << 5;                       // SPIE
            nm |= ((self.priv_mode & 1) as u32) << 8; // SPP
            self.csrs.mstatus = nm;
            self.priv_mode = 1; // S
            self.csrs.stvec & !0x3
        } else {
            self.csrs.mepc   = epc & !1;
            self.csrs.mcause = cause;
            self.csrs.mtval  = tval;
            let m = self.csrs.mstatus;
            let mie = (m >> 3) & 1;
            let mut nm = m & !((1 << 3) | (1 << 7) | (3 << 11));
            nm |= mie << 7;             // MPIE = old MIE
            nm |= (self.priv_mode as u32) << 11; // MPP = previous priv
            self.csrs.mstatus = nm;
            self.priv_mode = 3; // M
            self.csrs.mtvec & !0x3
        }
    }

    /// MRET: restore mstatus.MIE from MPIE; MPIE←1; MPP←M. Returns mepc.
    fn do_mret(&mut self) -> u32 {
        let m = self.csrs.mstatus;
        let mpie = (m >> 7) & 1;
        let mpp  = (m >> 11) & 0b11;
        let mut nm = m & !((1 << 3) | (1 << 7) | (3 << 11));
        nm |= mpie << 3;      // MIE = MPIE
        nm |= 1 << 7;         // MPIE = 1
        nm |= 0b11 << 11;     // MPP ← M (lowest supported is M for now)
        self.csrs.mstatus = nm;
        self.priv_mode = mpp as u8;
        self.csrs.mepc
    }

    /// SRET: restore sstatus.{SIE←SPIE, SPIE←1, SPP←0}, priv ← SPP.
    fn do_sret(&mut self) -> u32 {
        let m = self.csrs.mstatus;
        let spie = (m >> 5) & 1;
        let spp  = (m >> 8) & 1;
        let mut nm = m & !((1 << 1) | (1 << 5) | (1 << 8));
        nm |= spie << 1;     // SIE = SPIE
        nm |= 1 << 5;        // SPIE = 1
        // SPP cleared above (=U); spec says ←lowest supported (we have U).
        self.csrs.mstatus = nm;
        self.priv_mode = spp as u8; // 0=U, 1=S
        self.csrs.sepc
    }

    /// Asynchronous interrupt check, called at the top of `step()`.
    /// If an enabled interrupt is pending, redirects PC to mtvec and
    /// records the trap state. We do NOT squash any in-flight stages
    /// — async interrupts have no urgency, so the instructions
    /// already in IF/ID/EX/MA/WB drain naturally. The handler simply
    /// starts fetching at mtvec on this same cycle (the IF stage
    /// will see the new PC), and the older in-flight instructions
    /// complete behind it. `mepc` is set to the address that was
    /// about to be fetched (`self.pc` at trap time) so `mret` resumes
    /// exactly where the program would have continued.
    /// Direct mtvec mode only. Priority: MEI (11) > MSI (3) > MTI (7).
    fn check_and_take_interrupt(&mut self) -> bool {
        let mie_global = (self.csrs.mstatus >> 3) & 1 != 0;
        if !mie_global { return false; }
        let pending = self.csrs.mip & self.csrs.mie;
        if pending == 0 { return false; }
        let cause = if pending & (1 << 11) != 0 { 11 }
                    else if pending & (1 << 3) != 0 { 3 }
                    else if pending & (1 << 7) != 0 { 7 }
                    else { return false; };
        let int_cause = 0x8000_0000u32 | cause;
        let epc = self.pc;
        self.pc = self.take_trap(int_cause, epc, 0);
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
        self.csrs = csr::CsrFile::default();
        self.tlb.flush();
        self.priv_mode = 3;
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
