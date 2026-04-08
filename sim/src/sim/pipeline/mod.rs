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

        // ── 6. Branch flush ─────────────────────────────────────────
        // If EX detected a taken branch this cycle, the two
        // instructions in IF and ID are wrong (they were fetched
        // speculatively from the not-taken path). Squash them.
        let flush = new_ex_ma.valid && new_ex_ma.branch_taken;
        self.last_flush = flush;

        // ── 7. Latch ────────────────────────────────────────────────
        // Update each pipeline register based on stall/flush flags.
        self.ma_wb = new_ma_wb;
        self.ex_ma = new_ex_ma;

        if stall {
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

    /// Reset the pipeline to its initial state (program preserved).
    pub fn reset(&mut self) {
        self.pc = 0;
        self.registers = [0; 32];
        for word in self.ram.iter_mut() { *word = 0; }
        self.io_data = 0;
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
    }
}
