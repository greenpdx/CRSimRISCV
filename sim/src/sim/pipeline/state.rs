//! Shared types for the 5-stage pipeline simulator.
//!
//! Defines the four pipeline registers (IF/ID, ID/EX, EX/MA, MA/WB) and the
//! `PipelineState` struct that holds them along with the architectural
//! state (PC, registers, RAM, ROM, IO).
//!
//! See `sim/docs/pipeline-plan.md` for the full design discussion.

use super::csr::known_addrs;
use crate::isa::csr::addr;
use crate::sim::system::{PrivMode, SysUnit};
use super::tlb::Tlb;
use crate::sim::mem::Port;
use crate::sim::memsys::Event;
use super::mem::PipeMem;
use super::mem::IO_BASE;
use crate::sim::forwarding::RegWriter;
pub use crate::sim::stage::{CsrOp, Latch, MemKind, PcSel, TrapKind, WbSel};

/// Branch resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BranchMode {
    /// Resolve fully in EX, predict-not-taken. Penalty: 2 if taken, 0 if not.
    /// Default; matches the original behavior.
    Eager       = 0,
    /// Compute target in ID, always speculate taken. Penalty: flat 1 cycle
    /// for branches. JAL gets 0 cycles via in-IF predecode.
    AlwaysTarget= 1,
    /// Resolve direction in ID via a comparator + forwarding. Penalty: 1 if
    /// mispredicted, 0 if predicted correctly. Stalls when a branch reads a
    /// register still in flight.
    EarlyResolve= 2,
    /// Reserved for future BHT/BTB-based prediction.
    Predict     = 3,
}

impl Default for BranchMode {
    fn default() -> Self { BranchMode::Eager }
}

impl BranchMode {
    pub fn from_u8(v: u8) -> BranchMode {
        match v {
            1 => BranchMode::AlwaysTarget,
            2 => BranchMode::EarlyResolve,
            3 => BranchMode::Predict,
            _ => BranchMode::Eager,
        }
    }
    pub fn to_u8(self) -> u8 { self as u8 }
}


/// All control signals decoded from the instruction in ID, propagated
/// forward through the pipeline registers. Each downstream stage only
/// consults the bits it needs. Panel codes: `wb_sel.code()`,
/// `pc_sel.code()`, `csr_op.to_u8()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ControlBits {
    pub reg_write: bool,
    pub alu_src:   bool,   // false=rs2, true=immediate
    pub alu_op:    u8,     // ALU-control class 0|1|2
    pub mem:       MemKind,
    pub wb_sel:    WbSel,
    pub pc_sel:    PcSel,
    pub branch:    bool,   // is this a conditional branch?
    pub jump:      bool,   // is this an unconditional jump (JAL/JALR)?
    pub csr_op:    CsrOp,  // Zicsr
    pub is_sfence: bool,   // SFENCE.VMA — flush TLB in MA stage
}

/// IF/ID payload: an instruction fetched but not yet decoded.
#[derive(Debug, Clone, Copy, Default)]
pub struct IfIdData {
    pub pc_plus4: u32,
    /// None, or InstrPageFault (set by IF translation).
    pub trap_kind: TrapKind,
    pub trap_value: u32,
    /// True if this bubble was inserted by a branch flush.
    pub flush_bubble: bool,
}
/// IF/ID pipeline register.
pub type IfId = Latch<IfIdData>;

/// ID/EX payload: a decoded instruction with its operand values,
/// immediate, and the full control vector.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdExData {
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
    /// CSR address (instr bits 31:20) for a CSR instruction.
    pub csr_addr: u16,
    /// CSR immediate (rs1 field as zero-extended uimm) for CSRR*I.
    pub csr_uimm: u8,
    /// True if this is the immediate-form CSR op (uimm replaces rs1).
    pub csr_imm_form: bool,
    /// Synchronous trap / xRET decoded in ID (or a page fault from IF).
    pub trap_kind: TrapKind,
    /// If this entry is a stall-induced bubble, the register the
    /// younger instruction is waiting on. 0 = not a stall bubble.
    pub stall_reg: u8,
    /// True if this bubble was inserted by a branch flush.
    pub flush_bubble: bool,
    /// If this is a branch/JAL, the branch target computed by the ID-stage
    /// adder (PC + imm). Used by AlwaysTarget and EarlyResolve modes to
    /// redirect IF without waiting for EX.
    pub early_target: u32,
    /// True if compute_id detected a branch/JAL/JALR opcode that needs an
    /// early redirect by the ID-resolve modes. Set independently of
    /// `branch_taken` (which is decided in EX).
    pub early_branch: bool,
    /// True if `mod.rs` actually performed an ID-side redirect for this
    /// instruction. Only set in modes 2/3, only when `do_id_redirect`
    /// fired. Propagated through stage_ex into `ExMa.early_redirected`.
    pub id_redirected: bool,
}
/// ID/EX pipeline register.
pub type IdEx = Latch<IdExData>;

/// EX/MA payload: the ALU result and the control bits MA and WB still need.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExMaData {
    pub alu_result: u32,
    pub rs2_data: u32,         // for stores
    pub pc_plus4: u32,         // for JAL/JALR link
    pub rd_num: u8,
    pub funct3: u8,            // for load width logic
    pub branch_taken: bool,
    pub branch_target: u32,
    pub ctrl: ControlBits,
    /// CSR address forwarded to MA for a CSR instruction.
    pub csr_addr: u16,
    /// Value to write into the CSR (rs1_data forwarded, or zero-extended uimm).
    pub csr_write_val: u32,
    /// Was rs1 == 0 / uimm == 0? (Determines whether CSR write occurs for RS/RC.)
    pub csr_src_is_zero: bool,
    /// Trap/xRET kind, propagated from ID. Resolved by `step()` after EX
    /// (mepc = the latch's `pc`).
    pub trap_kind: TrapKind,
    /// Propagated stall-bubble tag (0 = not a stall bubble).
    pub stall_reg: u8,
    /// Propagated branch-flush bubble tag.
    pub flush_bubble: bool,
    /// True if the ID stage redirected IF for this branch (modes 2/3).
    /// Used so the EX-stage flush logic doesn't double-flush.
    pub early_redirected: bool,
}
/// EX/MA pipeline register.
pub type ExMa = Latch<ExMaData>;

/// MA/WB payload: the value about to be written back (or the alternatives
/// the WB mux chooses among).
#[derive(Debug, Clone, Copy, Default)]
pub struct MaWbData {
    pub alu_result: u32,
    pub mem_data: u32,
    pub pc_plus4: u32,
    pub rd_num: u8,
    pub ctrl: ControlBits,
    /// LoadPageFault / StorePageFault from MA translation (mepc = `pc`).
    pub trap_kind: TrapKind,
    pub trap_value: u32,
    /// Propagated stall-bubble tag (0 = not a stall bubble).
    pub stall_reg: u8,
    /// Propagated branch-flush bubble tag.
    pub flush_bubble: bool,
}
/// MA/WB pipeline register.
pub type MaWb = Latch<MaWbData>;

impl MaWbData {
    /// The value WB writes to rd (and MA/WB forwards).
    pub fn wb_value(&self) -> u32 { self.ctrl.wb_sel.select(self.alu_result, self.mem_data, self.pc_plus4) }
}

impl RegWriter for ExMa {
    fn writes(&self) -> Option<u8> { (self.valid && self.ctrl.reg_write && self.rd_num != 0).then_some(self.rd_num) }
    fn fwd_value(&self) -> u32 { self.alu_result }
}
impl RegWriter for MaWb {
    fn writes(&self) -> Option<u8> { (self.valid && self.ctrl.reg_write && self.rd_num != 0).then_some(self.rd_num) }
    fn fwd_value(&self) -> u32 { self.wb_value() }
}

pub use super::super::forwarding::ForwardSrc;

/// The full state of the 5-stage pipeline simulator.
#[derive(Debug, Clone)]
pub struct PipelineState {
    pub pc: u32,
    pub registers: [u32; 32],
    /// ROM / RAM / I/O, or this core's port into the memory panel's hierarchy.
    pub mem: PipeMem,
    /// Privilege mode, CSRs, trap entry / return, IRQ sampling — the
    /// system unit shared with the other cores (sim/system).
    pub sys: SysUnit,
    /// Fetch-side TLB.
    pub itlb: Tlb,
    /// Load / store TLB.
    pub dtlb: Tlb,
    /// Branch resolution strategy.
    pub branch_mode: BranchMode,
    pub cycle_count: u64,
    pub halted: bool,

    // Pipeline registers
    pub if_id: IfId,
    pub id_ex: IdEx,
    pub ex_ma: ExMa,
    pub ma_wb: MaWb,
    /// Snapshot of `ma_wb` *before* this cycle's WB stage consumed it.
    /// The visualizer's WB card uses this so MA and WB show distinct
    /// instructions (one cycle apart), instead of both showing the
    /// same `ma_wb` value.
    pub last_wb: MaWb,

    // Per-cycle pipeline-control diagnostics (so the visualizer can
    // show what the hazard / forwarding logic decided this cycle).
    pub last_stall: bool,
    pub last_flush: bool,
    pub last_forward_a: ForwardSrc,
    pub last_forward_b: ForwardSrc,
    /// True if an asynchronous interrupt was taken at the start of this cycle.
    pub last_trap_taken: bool,
    /// Register being awaited during a load-use stall (the load's rd).
    /// 0 when no stall.
    pub last_stall_reg: u8,

    // ── Per-cycle activity (for the visualizer) ────────────────────
    /// Bitmask of registers read this cycle (rs1/rs2 of ID).
    pub reg_read_mask: u32,
    /// Bitmask of registers written this cycle (rd of WB).
    pub reg_write_mask: u32,
    /// Address of the most recent load/store; 0 if none.
    pub last_ram_addr: u32,
    /// 0 = none, 1 = read, 2 = write.
    pub last_ram_kind: u8,
    /// 0 = machine, 1 = user. Tracks which RAM array was hit.
    pub last_ram_side: u8,
    /// Snapshot of all known CSRs at the *previous* cycle, so the
    /// visualizer can show which CSRs changed this cycle.
    pub last_csr_snapshot: Vec<u32>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            pc: 0,
            registers: [0; 32],
            mem: PipeMem::default(),
            sys: pipeline_sys(),
            itlb: Tlb::default(),
            dtlb: Tlb::default(),
            branch_mode: BranchMode::default(),
            cycle_count: 0,
            halted: false,
            if_id: IfId::default(),
            id_ex: IdEx::default(),
            ex_ma: ExMa::default(),
            ma_wb: MaWb::default(),
            last_wb: MaWb::default(),
            last_stall: false,
            last_flush: false,
            last_forward_a: ForwardSrc::Regfile,
            last_forward_b: ForwardSrc::Regfile,
            last_trap_taken: false,
            last_stall_reg: 0,
            reg_read_mask: 0,
            reg_write_mask: 0,
            last_ram_addr: 0,
            last_ram_kind: 0,
            last_ram_side: 0,
            last_csr_snapshot: vec![0; known_addrs().len()],
        }
    }
}

impl PipelineState {
    pub fn new() -> Self { Self::default() }

    /// Load a program into ROM from address 0 (flat model).
    pub fn load_program(&mut self, words: &[u32]) {
        if let Some(f) = self.mem.flat_mut() {
            for (slot, w) in f.rom.iter_mut().zip(words) { *slot = *w; }
        }
    }

    pub fn rom_read(&self, addr: u32) -> u32 { self.mem.fetch(addr) }

    /// Machine-side RAM (flat model; page tables live here).
    pub fn ram_read(&self, addr: u32) -> u32 { self.mem.flat().map_or(0, |f| f.ram_read(addr, false)) }
    pub fn ram_write(&mut self, addr: u32, val: u32) { if let Some(f) = self.mem.flat_mut() { f.ram_write(addr, val, false) } }
    /// User-side RAM (flat model).
    pub fn user_ram_read(&self, addr: u32) -> u32 { self.mem.flat().map_or(0, |f| f.ram_read(addr, true)) }
    pub fn user_ram_write(&mut self, addr: u32, val: u32) { if let Some(f) = self.mem.flat_mut() { f.ram_write(addr, val, true) } }

    /// U / S mode: the flat model's user RAM side.
    fn user_side(&self) -> bool { self.sys.priv_mode != PrivMode::M }

    /// Privilege-aware RAM read for stage_ma loads. M-mode hits
    /// machine RAM; U/S-mode hits user RAM. Records `last_ram_*`
    /// for the visualizer.
    pub fn priv_ram_read(&mut self, addr: u32) -> u32 {
        let user = self.user_side();
        let v = self.mem.load(addr, user);
        if !self.mem.is_hier() { self.last_ram_side = user as u8; }
        self.last_ram_addr = addr & !3;
        self.last_ram_kind = 1;
        v
    }

    pub fn priv_ram_write(&mut self, addr: u32, val: u32) {
        let user = self.user_side();
        self.mem.store(addr, val, user);
        if !self.mem.is_hier() { self.last_ram_side = user as u8; }
        self.last_ram_addr = addr & !3;
        self.last_ram_kind = 2;
    }

    /// The data word at `addr` as a store by the current mode would find it —
    /// no access is recorded and no cache state changes (SB / SH merge).
    pub fn word_peek(&self, addr: u32) -> u32 { self.mem.peek_data(addr, self.user_side()) }

    pub fn is_io(addr: u32) -> bool { addr >= IO_BASE }

    /// I/O at `addr` for this core? (None in memory-panel mode.)
    pub fn io_at(&self, addr: u32) -> bool { self.mem.io_at(addr) }

    pub fn write_register(&mut self, idx: u8, val: u32) {
        if idx != 0 && (idx as usize) < 32 { self.registers[idx as usize] = val; }
    }

    pub fn read_register(&self, idx: u8) -> u32 {
        self.registers[(idx as usize) & 0x1f]
    }
}

/// The pipeline's system unit at reset: M-mode, misa = RV32I.
pub fn pipeline_sys() -> SysUnit {
    let mut s = SysUnit::new();
    s.csrs.set_counter(addr::MISA, 0x4000_0100);   // MXL = 1 (RV32), I
    s
}

impl PipelineState {
    /// Current privilege mode as bits: 3 = M, 1 = S, 0 = U.
    pub fn priv_bits(&self) -> u8 { self.sys.priv_mode as u8 }
}

// ── Memory-panel mode helpers (no-ops / flat RAM without `hier`) ──────────
impl PipelineState {
    /// Page-table read at `level` (1 = root table, 0 = leaf table).
    pub fn pte_read(&self, addr: u32, level: u8) -> u32 { self.mem.pte_read(addr, level) }

    /// Page-table write (A / D bits).
    pub fn pte_write(&mut self, addr: u32, val: u32) { self.mem.pte_write(addr, val) }

    /// Report an MMU event to the memory system (memory-panel mode only):
    /// `e(core, port)` with the access being translated.
    pub fn mem_event(&self, e: impl FnOnce(u8, Port) -> Event) { self.mem.event(e) }

    /// The access about to be translated (Fetch / Load / Store).
    pub fn set_xlat_port(&self, port: Port) { self.mem.set_xlat_port(port) }

    /// Reset: invalidate the ITLB and the DTLB.
    pub fn flush_tlbs(&mut self) {
        self.itlb.flush();
        self.dtlb.flush();
    }

    /// SFENCE.VMA on both TLBs (`va` / `asid` = None for rs1 / rs2 = x0).
    pub fn sfence(&mut self, va: Option<u32>, asid: Option<u16>) {
        let itlb = self.itlb.sfence(va, asid);
        let dtlb = self.dtlb.sfence(va, asid);
        self.mem.event(|core, _| Event::Sfence { core, va, asid, itlb, dtlb });
    }
}
