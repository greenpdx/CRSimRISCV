//! Mirrors `src/stores/cpuState.ts::step()` — 9-phase single-cycle execution
//! with an `ActivityTrace` recording which components fire in which phase.

use crate::data::alu_control_data::compute_alu_control;
use crate::data::alu_data::{alu_compute, AluFlags};
use crate::data::control_data::{branch_condition, opcode_by_value, BranchCondition};
use crate::data::imm_data::{extract_immediate, ImmSel};
use crate::data::netlist as nl;
use crate::sim::micro::{next_phase, MicroState};
use crate::sim::phase::Phase;
use crate::sim::trace::ActivityTrace;
use crate::sim::trap::{
    addr as csra, interrupt_destination, mret, sret, take_trap,
    CsrFile, PrivMode, TrapInfo,
    EXC_BREAKPOINT, EXC_ECALL_FROM_M, EXC_ECALL_FROM_S, EXC_ECALL_FROM_U,
    EXC_ILLEGAL_INSTR,
};

const ROM_BASE: u32 = 0x0000_0000;
const RAM_BASE: u32 = 0x0001_0000;
const IO_BASE:  u32 = 0x4002_0000;
const MEM_WORDS: usize = 256;

#[derive(Debug, Clone)]
pub struct CpuState {
    pub pc: u32,
    pub registers: [u32; 32],
    pub rom: Vec<u32>,
    pub ram: Vec<u32>,
    pub io_data: u32,
    pub cycle_count: u64,
    pub halted: bool,
    /// Live phase-by-phase state for the visualizer.
    pub micro: MicroState,
    /// Drivers that fired this micro-step (bright green in the visualizer).
    pub last_active: Vec<&'static str>,
    /// Control signals consumed (sampled) by the phase that just ran.
    pub last_consumed: Vec<&'static str>,
    /// Register indices touched (read or written) by the current phase.
    pub last_reg_access: Vec<u8>,
    /// RAM byte addresses touched by the current phase.
    pub last_ram_access: Vec<u32>,
    /// ROM byte address touched by the current phase (Fetch only).
    pub last_rom_access: Option<u32>,

    // ── Privilege & traps ──────────────────────────────────────────────
    pub priv_mode: PrivMode,
    pub csrs: CsrFile,
    /// External IRQ inputs ORed into mip at the start of each IrqCheck.
    pub pending_irq: u32,
    pub last_trap: Option<TrapInfo>,
}

impl Default for CpuState {
    fn default() -> Self {
        Self {
            pc: 0,
            registers: [0; 32],
            rom: vec![0x0000_0013; MEM_WORDS],
            ram: vec![0; MEM_WORDS],
            io_data: 0,
            cycle_count: 0,
            halted: false,
            micro: MicroState::default(),
            last_active: Vec::new(),
            last_consumed: Vec::new(),
            last_reg_access: Vec::new(),
            last_ram_access: Vec::new(),
            last_rom_access: None,
            priv_mode: PrivMode::M,
            csrs: CsrFile::new(),
            pending_irq: 0,
            last_trap: None,
        }
    }
}

/// Control signals **sampled (consumed)** by each phase. These are the
/// signals whose value gates the phase's behavior — orthogonal to the
/// `active_for` set, which lists drivers that fired this cycle.
fn consumed_for(phase: Phase) -> &'static [&'static str] {
    match phase {
        Phase::IrqCheck => &["mie", "mip", "mideleg", "mstatus.MIE", "mstatus.SIE", "priv_mode"],
        Phase::Fetch    => &[],
        Phase::Decode   => &[],
        Phase::Control  => &[],
        Phase::Imm      => &["imm_sel"],
        Phase::RegRead  => &[],
        Phase::Alu      => &["alu_src", "alu_op"],
        Phase::Mem      => &["mem_read", "mem_write"],
        Phase::Wb       => &["reg_write", "wb_sel"],
        Phase::PcUpdate => &["pc_sel"],
    }
}

/// Names of the buses, controls, and components driven by each phase.
/// "Active" means the phase fired its driver this cycle, not that the
/// value changed (a real wire toggles its driver regardless of equality).
fn active_for(phase: Phase) -> Vec<&'static str> {
    match phase {
        Phase::IrqCheck => vec!["trap_ctrl"],
        Phase::Fetch    => vec!["pc_out", "instr", "pc_plus4"],
        Phase::Decode   => vec![],
        Phase::Control  => vec![
            "reg_write","alu_src","mem_read","mem_write",
            "alu_op","wb_sel","pc_sel","imm_sel",
        ],
        Phase::Imm      => vec!["immediate", "imm_gen"],
        Phase::RegRead  => vec!["rs1_data", "rs2_data"],
        Phase::Alu      => vec![
            // rs1_data feeds alu_a directly; rs2_data feeds the alu_src mux.
            // Both wires are sampled by the ALU stage this cycle.
            "rs1_data","rs2_data",
            "alu_b","alu_control","alu_result",
            "zero","neg","carry","overflow",
            "alu_src_mux","alu","loadstore",
        ],
        Phase::Mem      => vec!["mem_read_data"],
        Phase::Wb       => vec!["write_data", "wb_sel_mux"],
        Phase::PcUpdate => vec!["pc_offset", "jalr_target", "next_pc"],
    }
}

impl CpuState {
    pub fn new() -> Self { Self::default() }

    pub fn load_word(&mut self, byte_addr: u32, word: u32) {
        let idx = ((byte_addr - ROM_BASE) >> 2) as usize;
        if idx < self.rom.len() { self.rom[idx] = word; }
    }

    pub fn load_program(&mut self, words: &[u32]) {
        for (i, w) in words.iter().enumerate() {
            if i < self.rom.len() { self.rom[i] = *w; }
        }
    }

    pub fn write_register(&mut self, idx: u8, val: u32) {
        if idx != 0 && (idx as usize) < 32 { self.registers[idx as usize] = val; }
    }

    // ── Live debugger pokes — safe to call between or even during steps ──

    /// Force a register value while running. x0 stays pinned at 0.
    pub fn poke_register(&mut self, idx: u8, val: u32) {
        let i = idx as usize & 0x1f;
        if i != 0 { self.registers[i] = val; }
    }

    /// Patch one ROM word in place (byte address).
    pub fn poke_rom(&mut self, byte_addr: u32, val: u32) {
        let idx = ((byte_addr.wrapping_sub(ROM_BASE)) >> 2) as usize;
        if idx < self.rom.len() { self.rom[idx] = val; }
    }

    /// Patch one RAM word in place (byte address).
    pub fn poke_ram(&mut self, byte_addr: u32, val: u32) {
        let idx = ((byte_addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        if idx < self.ram.len() { self.ram[idx] = val; }
    }

    pub fn registers_snapshot(&self) -> [u32; 32] { self.registers }
    pub fn rom_snapshot(&self) -> &[u32] { &self.rom }
    pub fn ram_snapshot(&self) -> &[u32] { &self.ram }

    // ── Trap / privilege API ───────────────────────────────────────────
    pub fn priv_mode(&self) -> PrivMode { self.priv_mode }
    pub fn last_trap(&self) -> Option<TrapInfo> { self.last_trap }
    pub fn csr_read(&self, addr: u16) -> u32 { self.csrs.read(addr) }
    pub fn csr_write(&mut self, addr: u16, val: u32) { self.csrs.write(addr, val); }
    pub fn raise_irq(&mut self, mask: u32) { self.pending_irq |= mask; }
    pub fn clear_irq(&mut self) {
        self.pending_irq = 0;
        self.csrs.write(csra::MIP, 0);
    }
    pub fn csrs_snapshot(&self) -> Vec<(u16, u32)> {
        self.csrs.raw().iter().map(|(&a, &v)| (a, v)).collect()
    }

    fn rom_read(&self, addr: u32) -> u32 {
        let idx = ((addr - ROM_BASE) >> 2) as usize;
        self.rom.get(idx).copied().unwrap_or(0)
    }
    fn ram_read(&self, addr: u32) -> u32 {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        self.ram.get(idx).copied().unwrap_or(0)
    }
    fn ram_write(&mut self, addr: u32, val: u32) {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        if idx < self.ram.len() { self.ram[idx] = val; }
    }
    fn is_io(addr: u32) -> bool { addr >= IO_BASE }

    /// Execute one instruction and return the (phase → active components) trace.
    /// Faithful port of cpuState.ts::step() lines 212–390.
    pub fn step_traced(&mut self) -> ActivityTrace {
        let mut trace = ActivityTrace::new();
        if self.halted { return trace; }
        self.cycle_count += 1;

        // ── 1. Fetch ─────────────────────────────────────────────────────────
        let instr = self.rom_read(self.pc);
        trace.mark(Phase::Fetch, nl::PC);
        trace.mark(Phase::Fetch, nl::ROM);

        let pc_plus4 = self.pc.wrapping_add(4);

        // ── 2. Decode ───────────────────────────────────────────────────────
        let opcode  = (instr & 0x7f) as u8;
        let rd      = ((instr >>  7) & 0x1f) as u8;
        let funct3  = ((instr >> 12) & 0x07) as u8;
        let rs1_num = ((instr >> 15) & 0x1f) as u8;
        let rs2_num = ((instr >> 20) & 0x1f) as u8;
        let funct7b30 = ((instr >> 30) & 0x1) as u8;
        trace.mark(Phase::Decode, nl::DECODER);

        let entry = match opcode_by_value(opcode) {
            Some(e) => e,
            None => {
                // Unknown opcode → NOP, advance PC.
                trace.mark(Phase::PcUpdate, nl::PC_ADDER);
                trace.mark(Phase::PcUpdate, nl::MUX_PC_OFFSET);
                trace.mark(Phase::PcUpdate, nl::MUX_PC);
                trace.mark(Phase::PcUpdate, nl::PC);
                self.pc = pc_plus4;
                return trace;
            }
        };
        let sig = entry.signals;
        let is_lui = opcode == 0b0110111;

        // ── 3. Control signals ──────────────────────────────────────────────
        trace.mark(Phase::Control, nl::DECODER);

        // ── 4. Immediate generation ─────────────────────────────────────────
        let imm = extract_immediate(instr, ImmSel::from_u8(sig.imm_sel));
        if sig.imm_sel != 5 {
            trace.mark(Phase::Imm, nl::IMM_GEN);
        }

        // ── 5. Register read ────────────────────────────────────────────────
        let rs1_val = self.registers[rs1_num as usize];
        let rs2_val = self.registers[rs2_num as usize];
        trace.mark(Phase::RegRead, nl::REGFILE);

        // ── 6. ALU src mux + ALU control + compute ─────────────────────────
        let alu_b = if sig.alu_src { imm as u32 } else { rs2_val };

        let is_rtype    = opcode == 0b0110011;
        let is_shift_imm = opcode == 0b0010011 && (funct3 == 0b101 || funct3 == 0b001);
        let eff_f7b30 = if is_rtype || is_shift_imm { funct7b30 } else { 0 };
        let alu_ctrl_code = compute_alu_control(sig.alu_op, funct3, eff_f7b30, is_lui);

        let AluResultLocal { result: alu_result, flags } =
            { let r = alu_compute(alu_ctrl_code, rs1_val, alu_b);
              AluResultLocal { result: r.result, flags: r.flags } };

        trace.mark(Phase::Alu, nl::ALU_CTRL);
        trace.mark(Phase::Alu, nl::MUX_ALUSRC);
        trace.mark(Phase::Alu, nl::ALU);

        // ── 7. Memory access ────────────────────────────────────────────────
        let mut mem_read_data = 0u32;
        if sig.mem_read {
            trace.mark(Phase::Mem, nl::LOADSTORE);
            if Self::is_io(alu_result) {
                trace.mark(Phase::Mem, nl::IO);
                mem_read_data = self.io_data;
            } else {
                trace.mark(Phase::Mem, nl::RAM);
                mem_read_data = self.ram_read(alu_result);
            }
        }
        if sig.mem_write {
            trace.mark(Phase::Mem, nl::LOADSTORE);
            if Self::is_io(alu_result) {
                trace.mark(Phase::Mem, nl::IO);
                self.io_data = rs2_val;
            } else {
                trace.mark(Phase::Mem, nl::RAM);
                self.ram_write(alu_result, rs2_val);
            }
        }

        // ── 8/9. Write-back mux + register write (same clock edge) ─────────
        let wb_data = match sig.wb_sel {
            0 => alu_result,
            1 => mem_read_data,
            2 => pc_plus4,
            _ => 0,
        };
        if sig.reg_write {
            trace.mark(Phase::Wb, nl::MUX_WB);
            if rd != 0 {
                trace.mark(Phase::Wb, nl::REGFILE);
                self.registers[rd as usize] = wb_data;
            }
        }

        // ── 10. PC update: mux_pc_offset → pc_adder → mux_pc ────────────────
        let branch_taken = if sig.pc_sel != 1 {
            false
        } else if opcode == 0b1101111 {
            true // JAL unconditional
        } else {
            match branch_condition(funct3) {
                Some(BranchCondition::Zero)     =>  flags.zero,
                Some(BranchCondition::NotZero)  => !flags.zero,
                Some(BranchCondition::Neg)      =>  flags.negative,
                Some(BranchCondition::NotNeg)   => !flags.negative,
                Some(BranchCondition::Carry)    =>  flags.carry,
                Some(BranchCondition::NotCarry) => !flags.carry,
                None => false,
            }
        };
        let is_jalr = sig.pc_sel == 2;

        let pc_offset = if branch_taken { imm as u32 } else { 4 };
        let adder_result = self.pc.wrapping_add(pc_offset);
        let jalr_target  = alu_result & 0xffff_fffe;
        let next_pc      = if is_jalr { jalr_target } else { adder_result };

        trace.mark(Phase::PcUpdate, nl::MUX_PC_OFFSET);
        trace.mark(Phase::PcUpdate, nl::PC_ADDER);
        trace.mark(Phase::PcUpdate, nl::MUX_PC);
        trace.mark(Phase::PcUpdate, nl::PC);

        self.pc = next_pc;
        trace
    }
}

// Local alias to avoid naming clash with the crate-level AluResult struct.
struct AluResultLocal { result: u32, flags: AluFlags }

// ─────────────────────────────────────────────────────────────────────────
// Micro-step API: advance one phase at a time, populating MicroState.
// ─────────────────────────────────────────────────────────────────────────
impl CpuState {
    /// Advance exactly one phase. Returns the phase that just executed.
    /// Bus/control values persist across instructions (just like real
    /// hardware) — they only update when a phase actually drives them.
    /// `last_active()` reports which fields changed during this call.
    pub fn micro_step(&mut self) -> Phase {
        let next = next_phase(self.micro.current);
        // Clear per-phase trackers (regs/ram only). last_rom_access is sticky:
        // it shows the instruction currently being executed across the entire
        // cycle, and updates at IrqCheck/Fetch when the PC actually moves.
        self.last_reg_access.clear();
        self.last_ram_access.clear();

        self.run_phase(next);
        self.micro.current = Some(next);
        self.last_active = self.compute_active(next);
        self.last_consumed = consumed_for(next).iter().copied().collect();
        self.compute_access(next);
        next
    }

    pub fn last_consumed(&self) -> &[&'static str] { &self.last_consumed }
    pub fn last_reg_access(&self) -> &[u8]  { &self.last_reg_access }
    pub fn last_ram_access(&self) -> &[u32] { &self.last_ram_access }
    pub fn last_rom_access(&self) -> Option<u32> { self.last_rom_access }

    /// Populate the per-phase storage access lists from the just-updated `MicroState`.
    fn compute_access(&mut self, phase: Phase) {
        match phase {
            // Update the "currently executing" ROM highlight at the boundary
            // of each cycle. IrqCheck runs first; if it took a trap, self.pc
            // already points at mtvec. Otherwise pc is the next instruction's
            // address. Fetch keeps it in sync in case it differs.
            Phase::IrqCheck => {
                self.last_rom_access = Some(self.pc);
            }
            Phase::Fetch => {
                self.last_rom_access = self.micro.pc_out;
            }
            Phase::RegRead => {
                if let Some(rs1) = self.micro.rs1_num { self.last_reg_access.push(rs1); }
                if let Some(rs2) = self.micro.rs2_num {
                    if !self.last_reg_access.contains(&rs2) { self.last_reg_access.push(rs2); }
                }
            }
            Phase::Mem => {
                let mr = self.micro.ctrl_mem_read.unwrap_or(false);
                let mw = self.micro.ctrl_mem_write.unwrap_or(false);
                if (mr || mw) && !Self::is_io(self.micro.alu_result.unwrap_or(0)) {
                    if let Some(addr) = self.micro.alu_result {
                        self.last_ram_access.push(addr & !3);
                    }
                }
            }
            Phase::Wb => {
                if self.micro.ctrl_reg_write.unwrap_or(false) {
                    if let Some(rd) = self.micro.rd {
                        if rd != 0 { self.last_reg_access.push(rd); }
                    }
                }
                // CSR ops also write rd (with the *old* CSR value).
                let opcode = self.micro.opcode.unwrap_or(0);
                let funct3 = self.micro.funct3.unwrap_or(0);
                if opcode == 0b1110011 && funct3 != 0 {
                    if let Some(rd) = self.micro.rd { if rd != 0 { self.last_reg_access.push(rd); } }
                }
            }
            _ => {}
        }
    }

    /// Returns the decoded opcode class name (e.g. "I-ALU", "Load") if known.
    pub fn opcode_name(&self) -> Option<&'static str> {
        self.micro.opcode.and_then(opcode_by_value).map(|e| e.name)
    }

    fn compute_active(&self, phase: Phase) -> Vec<&'static str> {
        let mut v = active_for(phase);
        if phase == Phase::Alu {
            // rs2_data only feeds the ALU when alu_src = 0 (rs2 path).
            // For imm-type instructions (alu_src = 1) rs2 is not consumed.
            if self.micro.ctrl_alu_src.unwrap_or(false) {
                v.retain(|n| *n != "rs2_data");
            }
        }
        v
    }

    pub fn last_active(&self) -> &[&'static str] { &self.last_active }

    fn run_phase(&mut self, p: Phase) {
        match p {
            Phase::IrqCheck => self.phase_irq_check(),
            Phase::Fetch    => self.phase_fetch(),
            Phase::Decode   => self.phase_decode(),
            Phase::Control  => self.phase_control(),
            Phase::Imm      => self.phase_imm(),
            Phase::RegRead  => self.phase_regread(),
            Phase::Alu      => self.phase_alu(),
            Phase::Mem      => self.phase_mem(),
            Phase::Wb       => self.phase_wb(),
            Phase::PcUpdate => self.phase_pc_update(),
        }
    }

    /// IrqCheck — sample mip & mie. If a trap should fire, redirect PC to the
    /// trap vector and switch privilege mode. The next Fetch will see the
    /// handler entry. We OR `pending_irq` into mip live so externally
    /// asserted IRQs are visible to the standard mip/mie path.
    fn phase_irq_check(&mut self) {
        // Merge external pending bits into mip (level-triggered).
        let mip = self.csrs.read(csra::MIP) | self.pending_irq;
        self.csrs.write(csra::MIP, mip);

        if let Some((cause, _dest)) = interrupt_destination(&self.csrs, self.priv_mode, 0) {
            let (new_pc, new_mode, mut info) =
                take_trap(&mut self.csrs, self.priv_mode, cause, true, self.pc, 0);
            info.cycle = self.cycle_count;
            self.last_trap = Some(info);
            self.pc = new_pc;
            self.priv_mode = new_mode;
            self.micro.trap_taken = true;
        } else {
            self.micro.trap_taken = false;
        }
    }

    fn phase_fetch(&mut self) {
        let pc = self.pc;
        let instr = self.rom_read(pc);
        self.micro.pc_out = Some(pc);
        self.micro.instr = Some(instr);
        self.micro.pc_plus4 = Some(pc.wrapping_add(4));
    }

    fn phase_decode(&mut self) {
        let instr = self.micro.instr.unwrap_or(0);
        self.micro.opcode  = Some((instr & 0x7f) as u8);
        self.micro.rd      = Some(((instr >>  7) & 0x1f) as u8);
        self.micro.funct3  = Some(((instr >> 12) & 0x07) as u8);
        self.micro.rs1_num = Some(((instr >> 15) & 0x1f) as u8);
        self.micro.rs2_num = Some(((instr >> 20) & 0x1f) as u8);
    }

    fn phase_control(&mut self) {
        let opcode = self.micro.opcode.unwrap_or(0);
        if let Some(e) = opcode_by_value(opcode) {
            let s = e.signals;
            self.micro.ctrl_reg_write = Some(s.reg_write);
            self.micro.ctrl_alu_src   = Some(s.alu_src);
            self.micro.ctrl_mem_read  = Some(s.mem_read);
            self.micro.ctrl_mem_write = Some(s.mem_write);
            self.micro.ctrl_alu_op    = Some(s.alu_op);
            self.micro.ctrl_wb_sel    = Some(s.wb_sel);
            self.micro.ctrl_pc_sel    = Some(s.pc_sel);
            self.micro.ctrl_imm_sel   = Some(s.imm_sel);
        }
    }

    fn phase_imm(&mut self) {
        let instr = self.micro.instr.unwrap_or(0);
        let sel = self.micro.ctrl_imm_sel.unwrap_or(5);
        let imm = extract_immediate(instr, ImmSel::from_u8(sel));
        self.micro.immediate = Some(imm as u32);
    }

    fn phase_regread(&mut self) {
        let rs1 = self.micro.rs1_num.unwrap_or(0) as usize;
        let rs2 = self.micro.rs2_num.unwrap_or(0) as usize;
        self.micro.rs1_data = Some(self.registers[rs1]);
        self.micro.rs2_data = Some(self.registers[rs2]);
    }

    fn phase_alu(&mut self) {
        let opcode = self.micro.opcode.unwrap_or(0);
        let funct3 = self.micro.funct3.unwrap_or(0);
        let alu_src = self.micro.ctrl_alu_src.unwrap_or(false);
        let alu_op  = self.micro.ctrl_alu_op.unwrap_or(0);
        let imm     = self.micro.immediate.unwrap_or(0);
        let rs1_val = self.micro.rs1_data.unwrap_or(0);
        let rs2_val = self.micro.rs2_data.unwrap_or(0);

        let alu_b = if alu_src { imm } else { rs2_val };
        self.micro.alu_b = Some(alu_b);

        let is_lui  = opcode == 0b0110111;
        let is_rt   = opcode == 0b0110011;
        let is_simm = opcode == 0b0010011 && (funct3 == 0b101 || funct3 == 0b001);
        let f7 = if is_rt || is_simm {
            ((self.micro.instr.unwrap_or(0) >> 30) & 0x1) as u8
        } else { 0 };

        let code = compute_alu_control(alu_op, funct3, f7, is_lui);
        self.micro.alu_control = Some(code);

        let r = alu_compute(code, rs1_val, alu_b);
        self.micro.alu_result    = Some(r.result);
        self.micro.zero_flag     = Some(r.flags.zero);
        self.micro.neg_flag      = Some(r.flags.negative);
        self.micro.carry_flag    = Some(r.flags.carry);
        self.micro.overflow_flag = Some(r.flags.overflow);
    }

    fn phase_mem(&mut self) {
        let mem_read  = self.micro.ctrl_mem_read.unwrap_or(false);
        let mem_write = self.micro.ctrl_mem_write.unwrap_or(false);
        let addr      = self.micro.alu_result.unwrap_or(0);
        let rs2_val   = self.micro.rs2_data.unwrap_or(0);

        if mem_read {
            let data = if Self::is_io(addr) { self.io_data } else { self.ram_read(addr) };
            self.micro.mem_read_data = Some(data);
        }
        if mem_write {
            if Self::is_io(addr) { self.io_data = rs2_val; }
            else { self.ram_write(addr, rs2_val); }
        }
    }

    fn phase_wb(&mut self) {
        let opcode = self.micro.opcode.unwrap_or(0);
        let funct3 = self.micro.funct3.unwrap_or(0);

        // ── CSR ops (Zicsr): opcode SYSTEM (1110011) with funct3 != 0 ───
        if opcode == 0b1110011 && funct3 != 0 {
            let instr   = self.micro.instr.unwrap_or(0);
            let csr_addr = ((instr >> 20) & 0xfff) as u16;
            let rs1     = self.micro.rs1_num.unwrap_or(0);
            let rs1_val = self.micro.rs1_data.unwrap_or(0);
            let uimm    = rs1 as u32; // immediate forms zero-extend rs1 field
            let src     = if funct3 & 0b100 != 0 { uimm } else { rs1_val };
            let old     = self.csrs.read(csr_addr);
            let new = match funct3 & 0b011 {
                0b01 => src,                  // CSRRW / CSRRWI
                0b10 => old | src,            // CSRRS / CSRRSI
                0b11 => old & !src,           // CSRRC / CSRRCI
                _    => old,
            };
            // CSRRS/CSRRC with rs1==x0 should not write (avoid clearing W1C bits).
            let do_write = match funct3 & 0b011 {
                0b01 => true,
                _    => rs1 != 0,
            };
            if do_write { self.csrs.write(csr_addr, new); }
            // rd ← old (always; rd=0 is the no-write case for the regfile)
            let rd = self.micro.rd.unwrap_or(0);
            if rd != 0 { self.registers[rd as usize] = old; }
            self.micro.write_data = Some(old);
            return;
        }

        // ── Ordinary write-back ─────────────────────────────────────────
        let wb_sel = self.micro.ctrl_wb_sel.unwrap_or(0);
        let wb_data = match wb_sel {
            0 => self.micro.alu_result.unwrap_or(0),
            1 => self.micro.mem_read_data.unwrap_or(0),
            2 => self.micro.pc_plus4.unwrap_or(0),
            _ => 0,
        };
        self.micro.write_data = Some(wb_data);

        if self.micro.ctrl_reg_write.unwrap_or(false) {
            let rd = self.micro.rd.unwrap_or(0);
            if rd != 0 { self.registers[rd as usize] = wb_data; }
        }
    }

    fn phase_pc_update(&mut self) {
        let opcode = self.micro.opcode.unwrap_or(0);
        let funct3 = self.micro.funct3.unwrap_or(0);
        let pc_sel = self.micro.ctrl_pc_sel.unwrap_or(0);
        let imm    = self.micro.immediate.unwrap_or(0);
        let zero   = self.micro.zero_flag.unwrap_or(false);
        let neg    = self.micro.neg_flag.unwrap_or(false);
        let carry  = self.micro.carry_flag.unwrap_or(false);

        // ── SYSTEM funct3=0: ECALL/EBREAK/MRET/SRET ─────────────────────
        if opcode == 0b1110011 && funct3 == 0 {
            let instr  = self.micro.instr.unwrap_or(0);
            let imm12  = (instr >> 20) & 0xfff;
            let funct7 = (instr >> 25) & 0x7f;
            let rs2    = (instr >> 20) & 0x1f;

            // MRET: imm = 0x302 (funct7 0011000, rs2 00010)
            if funct7 == 0b0011000 && rs2 == 0b00010 {
                let (epc, new_mode) = mret(&mut self.csrs);
                self.priv_mode = new_mode;
                self.pc = epc;
                self.micro.next_pc = Some(epc);
                self.cycle_count += 1;
                return;
            }
            // SRET: imm = 0x102 (funct7 0001000, rs2 00010)
            if funct7 == 0b0001000 && rs2 == 0b00010 {
                let (epc, new_mode) = sret(&mut self.csrs);
                self.priv_mode = new_mode;
                self.pc = epc;
                self.micro.next_pc = Some(epc);
                self.cycle_count += 1;
                return;
            }
            // ECALL / EBREAK take an exception trap.
            let cause = match imm12 {
                0x000 => Some(match self.priv_mode {
                    PrivMode::U => EXC_ECALL_FROM_U,
                    PrivMode::S => EXC_ECALL_FROM_S,
                    PrivMode::M => EXC_ECALL_FROM_M,
                }),
                0x001 => Some(EXC_BREAKPOINT),
                _ => None,
            };
            if let Some(c) = cause {
                let epc = self.pc; // ecall: epc = pc of the ecall instruction
                let (new_pc, new_mode, mut info) =
                    take_trap(&mut self.csrs, self.priv_mode, c, false, epc, 0);
                info.cycle = self.cycle_count;
                self.last_trap = Some(info);
                self.pc = new_pc;
                self.priv_mode = new_mode;
                self.micro.next_pc = Some(new_pc);
                self.micro.trap_taken = true;
                self.cycle_count += 1;
                return;
            }
            // Unknown SYSTEM imm → illegal instruction.
            self.take_exception(EXC_ILLEGAL_INSTR, instr);
            return;
        }

        let branch_taken = if pc_sel != 1 { false }
        else if opcode == 0b1101111 { true }
        else {
            match branch_condition(funct3) {
                Some(BranchCondition::Zero)     =>  zero,
                Some(BranchCondition::NotZero)  => !zero,
                Some(BranchCondition::Neg)      =>  neg,
                Some(BranchCondition::NotNeg)   => !neg,
                Some(BranchCondition::Carry)    =>  carry,
                Some(BranchCondition::NotCarry) => !carry,
                None => false,
            }
        };
        let is_jalr = pc_sel == 2;
        let pc_offset = if branch_taken { imm } else { 4 };
        self.micro.pc_offset = Some(pc_offset);
        let adder = self.pc.wrapping_add(pc_offset);
        let jalr_target = self.micro.alu_result.unwrap_or(0) & 0xffff_fffe;
        self.micro.jalr_target = Some(jalr_target);
        let next = if is_jalr { jalr_target } else { adder };
        self.micro.next_pc = Some(next);

        self.pc = next;
        self.cycle_count += 1;
    }

    fn take_exception(&mut self, cause: u32, tval: u32) {
        let epc = self.pc;
        let (new_pc, new_mode, mut info) =
            take_trap(&mut self.csrs, self.priv_mode, cause, false, epc, tval);
        info.cycle = self.cycle_count;
        self.last_trap = Some(info);
        self.pc = new_pc;
        self.priv_mode = new_mode;
        self.micro.next_pc = Some(new_pc);
        self.micro.trap_taken = true;
        self.cycle_count += 1;
    }
}
