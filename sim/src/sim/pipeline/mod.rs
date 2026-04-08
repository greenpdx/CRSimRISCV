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
    ControlBits, ExMa, ForwardSrc, IdEx, IfId, MaWb, PipelineState,
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

        // ── 0. Interrupt check (asynchronous trap) ─────────────────
        // Take pending+enabled interrupts at instruction boundary.
        // mepc is set to the next-to-fetch PC; in-flight IF/ID and
        // ID/EX are squashed (their instructions never retire);
        // EX/MA/WB drain naturally.
        self.last_trap_taken = false;
        if self.check_and_take_interrupt() {
            self.last_trap_taken = true;
            self.if_id = IfId::bubble();
            self.id_ex = IdEx::bubble();
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
        let new_id_ex_default = stage_id::compute_id(self, &prev_if_id)
            .unwrap_or_else(IdEx::bubble);

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
            // Squash the trapping instruction so MA/WB ignore it.
            new_ex_ma = ExMa::bubble();
        }

        // ── 6b. Branch flush ────────────────────────────────────────
        let flush = new_ex_ma.valid && new_ex_ma.branch_taken;
        self.last_flush = flush || trap_redirect.is_some();

        // ── 7. Latch ────────────────────────────────────────────────
        self.ma_wb = new_ma_wb;
        self.ex_ma = new_ex_ma;

        if let Some(target) = trap_redirect {
            // Sync trap or mret: squash IF/ID and ID/EX, redirect.
            self.if_id = IfId::bubble();
            self.id_ex = IdEx::bubble();
            self.pc = target;
        } else if stall {
            // Freeze IF/ID and PC, insert bubble in ID/EX.
            self.id_ex = IdEx::bubble();
            // self.if_id stays as it was (don't overwrite).
            // self.pc stays as it was.
        } else if flush {
            // The branch fired; squash the next two instructions in
            // flight and redirect PC.
            self.if_id = IfId::bubble();
            self.id_ex = IdEx::bubble();
            self.pc = new_ex_ma.branch_target;
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
    /// If an enabled interrupt is pending, takes the trap and returns
    /// true (caller squashes IF/ID and ID/EX so the in-flight younger
    /// instructions never retire). Direct mtvec mode only. Priority:
    /// MEI (11) > MSI (3) > MTI (7).
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
        // mepc = address of next instruction that *would* have been fetched.
        let epc = self.pc;
        self.pc = self.take_trap(int_cause, epc, 0);
        true
    }

    /// Reset the pipeline to its initial state (program preserved).
    pub fn reset(&mut self) {
        self.pc = 0;
        self.registers = [0; 32];
        for word in self.ram.iter_mut() { *word = 0; }
        self.io_data = 0;
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
    }
}
