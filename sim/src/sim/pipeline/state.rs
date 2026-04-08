//! Shared types for the 5-stage pipeline simulator.
//!
//! Defines the four pipeline registers (IF/ID, ID/EX, EX/MA, MA/WB) and the
//! `PipelineState` struct that holds them along with the architectural
//! state (PC, registers, RAM, ROM, IO).
//!
//! See `sim/docs/pipeline-plan.md` for the full design discussion.

use super::csr::CsrFile;
use super::tlb::Tlb;

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
}

impl MaWb {
    pub fn bubble() -> Self { Self::default() }
}

/// Forwarding source for an ALU input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForwardSrc {
    #[default]
    Regfile,
    ExMa,
    MaWb,
}

/// The full state of the 5-stage pipeline simulator.
#[derive(Debug, Clone)]
pub struct PipelineState {
    pub pc: u32,
    pub registers: [u32; 32],
    pub rom: Vec<u32>,
    pub ram: Vec<u32>,
    pub io_data: u32,
    pub csrs: CsrFile,
    pub tlb: Tlb,
    /// Current privilege mode: 3=M, 1=S, 0=U. Reset = M.
    pub priv_mode: u8,
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
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            pc: 0,
            registers: [0; 32],
            rom: vec![0x0000_0013; MEM_WORDS],  // NOPs
            ram: vec![0; MEM_WORDS],
            io_data: 0,
            csrs: CsrFile::default(),
            tlb: Tlb::default(),
            priv_mode: 3,
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

    pub fn ram_read(&self, addr: u32) -> u32 {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        self.ram.get(idx).copied().unwrap_or(0)
    }

    pub fn ram_write(&mut self, addr: u32, val: u32) {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        if idx < self.ram.len() { self.ram[idx] = val; }
    }

    pub fn is_io(addr: u32) -> bool { addr >= IO_BASE }

    pub fn write_register(&mut self, idx: u8, val: u32) {
        if idx != 0 && (idx as usize) < 32 { self.registers[idx as usize] = val; }
    }

    pub fn read_register(&self, idx: u8) -> u32 {
        self.registers[(idx as usize) & 0x1f]
    }
}
