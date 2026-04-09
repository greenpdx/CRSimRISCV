//! Shared types for the 5-stage pipeline simulator.
//!
//! Defines the four pipeline registers (IF/ID, ID/EX, EX/MA, MA/WB) and the
//! `PipelineState` struct that holds them along with the architectural
//! state (PC, registers, RAM, ROM, IO).
//!
//! See `sim/docs/pipeline-plan.md` for the full design discussion.

use super::csr::CsrFile;
use super::tlb::Tlb;

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

const ROM_BASE: u32 = 0x0000_0000;
const RAM_BASE: u32 = 0x0001_0000;
const IO_BASE:  u32 = 0x4002_0000;
const MEM_WORDS: usize = 256;

/// All control signals decoded from the instruction in ID, propagated
/// forward through the pipeline registers. Each downstream stage only
/// consults the bits it needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct ControlBits {
    pub reg_write: bool,
    pub alu_src:   bool,   // false=rs2, true=immediate
    pub alu_op:    u8,     // 0|1|2
    pub mem_read:  bool,
    pub mem_write: bool,
    pub wb_sel:    u8,     // 0=ALU, 1=Mem, 2=PC+4, 3=CSR
    pub pc_sel:    u8,     // 0=PC+4, 1=PC+imm, 2=(rs1+imm)&~1
    pub branch:    bool,   // is this a conditional branch?
    pub jump:      bool,   // is this an unconditional jump (JAL/JALR)?
    pub csr_op:    u8,     // 0=none, 1=RW, 2=RS, 3=RC (Zicsr Phase 1)
    pub is_sfence: bool,   // SFENCE.VMA — flush TLB in MA stage (Phase 4.4)
}

/// IF/ID pipeline register. Holds an instruction that has been fetched
/// but not yet decoded.
#[derive(Debug, Clone, Copy, Default)]
pub struct IfId {
    pub valid: bool,
    pub pc: u32,
    pub instr: u32,
    pub pc_plus4: u32,
    /// 0 = none, 6 = instruction page fault (set by IF translation)
    pub trap_kind: u8,
    pub trap_value: u32,
    /// True if this bubble was inserted by a branch flush.
    pub flush_bubble: bool,
}

impl IfId {
    pub fn bubble() -> Self { Self::default() }
}

/// ID/EX pipeline register. Holds a decoded instruction with its operand
/// values, immediate, and the full control vector.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdEx {
    pub valid: bool,
    pub pc: u32,
    pub pc_plus4: u32,
    pub instr: u32,           // original instruction word (for display)
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
    /// CSR address (instr[31:20]) when ctrl.csr_op != 0.
    pub csr_addr: u16,
    /// CSR immediate (rs1 field as zero-extended uimm) for CSRR*I.
    pub csr_uimm: u8,
    /// True if this is the immediate-form CSR op (uimm replaces rs1).
    pub csr_imm_form: bool,
    /// Synchronous trap / mret kind decoded in ID:
    /// 0 = none, 1 = illegal_instr, 2 = ecall_m, 3 = ebreak, 4 = mret.
    pub trap_kind: u8,
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

impl IdEx {
    pub fn bubble() -> Self { Self::default() }
}

/// EX/MA pipeline register. Holds the ALU result and the control bits
/// the MA and WB stages still need.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExMa {
    pub valid: bool,
    pub instr: u32,            // original instruction word (for display)
    pub alu_result: u32,
    pub rs2_data: u32,         // for stores
    pub pc_plus4: u32,         // for JAL/JALR link
    pub rd_num: u8,
    pub funct3: u8,            // for load width logic
    pub branch_taken: bool,
    pub branch_target: u32,
    pub ctrl: ControlBits,
    /// CSR address forwarded to MA when ctrl.csr_op != 0.
    pub csr_addr: u16,
    /// Value to write into the CSR (rs1_data forwarded, or zero-extended uimm).
    pub csr_write_val: u32,
    /// Was rs1 == 0 / uimm == 0? (Determines whether CSR write occurs for RS/RC.)
    pub csr_src_is_zero: bool,
    /// Trap/mret kind, propagated from ID. Resolved by `step()` after EX.
    pub trap_kind: u8,
    /// PC of the trapping instruction (saved into mepc on sync trap).
    pub trap_pc: u32,
    /// Propagated stall-bubble tag (0 = not a stall bubble).
    pub stall_reg: u8,
    /// Propagated branch-flush bubble tag.
    pub flush_bubble: bool,
    /// True if the ID stage redirected IF for this branch (modes 2/3).
    /// Used so the EX-stage flush logic doesn't double-flush.
    pub early_redirected: bool,
}

impl ExMa {
    pub fn bubble() -> Self { Self::default() }
}

/// MA/WB pipeline register. Holds the value that's about to be written
/// back to the register file (or the alternatives the WB mux can choose
/// among).
#[derive(Debug, Clone, Copy, Default)]
pub struct MaWb {
    pub valid: bool,
    pub instr: u32,            // original instruction word (for display)
    pub alu_result: u32,
    pub mem_data: u32,
    pub pc_plus4: u32,
    pub rd_num: u8,
    pub ctrl: ControlBits,
    /// Trap kind for an MA-stage page fault (7=load, 8=store).
    pub trap_kind: u8,
    pub trap_pc: u32,
    pub trap_value: u32,
    /// Propagated stall-bubble tag (0 = not a stall bubble).
    pub stall_reg: u8,
    /// Propagated branch-flush bubble tag.
    pub flush_bubble: bool,
}

impl MaWb {
    pub fn bubble() -> Self { Self::default() }
}

pub use super::super::forwarding::ForwardSrc;

/// The full state of the 5-stage pipeline simulator.
#[derive(Debug, Clone)]
pub struct PipelineState {
    pub pc: u32,
    pub registers: [u32; 32],
    pub rom: Vec<u32>,
    pub machine_ram: Vec<u32>,
    pub user_ram: Vec<u32>,
    pub io_data: u32,
    pub csrs: CsrFile,
    pub tlb: Tlb,
    /// Current privilege mode: 3=M, 1=S, 0=U. Reset = M.
    pub priv_mode: u8,
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
            rom: vec![0x0000_0013; MEM_WORDS],  // NOPs
            machine_ram: vec![0; MEM_WORDS],
            user_ram: vec![0; MEM_WORDS],
            io_data: 0,
            csrs: CsrFile::default(),
            tlb: Tlb::default(),
            priv_mode: 3,
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
            last_csr_snapshot: vec![0; CsrFile::known_addrs().len()],
        }
    }
}

impl PipelineState {
    pub fn new() -> Self { Self::default() }

    pub fn load_program(&mut self, words: &[u32]) {
        for (i, w) in words.iter().enumerate() {
            if i < self.rom.len() { self.rom[i] = *w; }
        }
    }

    pub fn rom_read(&self, addr: u32) -> u32 {
        let idx = ((addr.wrapping_sub(ROM_BASE)) >> 2) as usize;
        self.rom.get(idx).copied().unwrap_or(0)
    }

    /// Machine-side RAM read. Used by the page-table walker (PTEs
    /// always live in machine RAM) and by M-mode loads. Existing
    /// tests that pre-date the user/machine split also call this.
    pub fn ram_read(&self, addr: u32) -> u32 {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        self.machine_ram.get(idx).copied().unwrap_or(0)
    }

    pub fn ram_write(&mut self, addr: u32, val: u32) {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        if idx < self.machine_ram.len() { self.machine_ram[idx] = val; }
    }

    pub fn user_ram_read(&self, addr: u32) -> u32 {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        self.user_ram.get(idx).copied().unwrap_or(0)
    }

    pub fn user_ram_write(&mut self, addr: u32, val: u32) {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        if idx < self.user_ram.len() { self.user_ram[idx] = val; }
    }

    /// Privilege-aware RAM read for stage_ma loads. M-mode hits
    /// machine RAM; U/S-mode hits user RAM. Records `last_ram_*`
    /// for the visualizer.
    pub fn priv_ram_read(&mut self, addr: u32) -> u32 {
        let v;
        if self.priv_mode == 3 {
            v = self.ram_read(addr);
            self.last_ram_side = 0;
        } else {
            v = self.user_ram_read(addr);
            self.last_ram_side = 1;
        }
        self.last_ram_addr = addr & !3;
        self.last_ram_kind = 1;
        v
    }

    pub fn priv_ram_write(&mut self, addr: u32, val: u32) {
        if self.priv_mode == 3 {
            self.ram_write(addr, val);
            self.last_ram_side = 0;
        } else {
            self.user_ram_write(addr, val);
            self.last_ram_side = 1;
        }
        self.last_ram_addr = addr & !3;
        self.last_ram_kind = 2;
    }

    pub fn is_io(addr: u32) -> bool { addr >= IO_BASE }

    pub fn write_register(&mut self, idx: u8, val: u32) {
        if idx != 0 && (idx as usize) < 32 { self.registers[idx as usize] = val; }
    }

    pub fn read_register(&self, idx: u8) -> u32 {
        self.registers[(idx as usize) & 0x1f]
    }
}
