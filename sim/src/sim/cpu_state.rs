//! Mirrors `src/stores/cpuState.ts::step()` — 9-phase single-cycle execution
//! with an `ActivityTrace` recording which components fire in which phase.

use crate::data::alu_data::{alu_compute, AluFlags};
use crate::data::control_data::opcode_by_value;
use crate::isa::instr::{decode, Instr};
use crate::sim::execute::{alu_control_code, decide_branch};
use crate::isa::imm::{extract_immediate, ImmSel};
use crate::data::netlist as nl;
use crate::sim::micro::{next_phase, MicroState};
use crate::sim::mem::{Bus, Port};
use crate::sim::phase::Phase;
use crate::sim::trace::ActivityTrace;
use crate::sim::system::{PrivMode, SysUnit, TrapInfo, EXC_BREAKPOINT, EXC_ILLEGAL_INSTR,
    EXC_LOAD_ADDR_MISALIGNED, EXC_STORE_ADDR_MISALIGNED};
use crate::isa::lsu;
use crate::isa::mul::mul;

const ROM_BASE: u32 = 0x0000_0000;
const RAM_BASE: u32 = 0x0001_0000;
pub const IO_BASE:  u32 = 0x4002_0000;
const MEM_WORDS: usize = 256;

/// Single-cycle memory: ROM at 0 (instruction fetch), RAM at `RAM_BASE`
/// (words past it kept sparse in `ram_extra`), and one I/O register that
/// answers at every address from `io_base` up.
#[cfg_attr(feature = "wasm", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", serde(default))]
#[derive(Debug, Clone)]
pub struct SingleMem {
    pub rom: Vec<u32>,
    pub ram: Vec<u32>,
    pub io_data: u32,
    /// I/O window starts here (everything at or above is I/O).
    pub io_base: u32,
    /// RAM words written outside the RAM window (below the I/O base): sparse.
    pub ram_extra: std::collections::BTreeMap<u32, u32>,
}

impl Default for SingleMem {
    fn default() -> Self {
        Self { rom: vec![0x0000_0013; MEM_WORDS], ram: vec![0; MEM_WORDS], io_data: 0, io_base: IO_BASE, ram_extra: Default::default() }
    }
}

impl SingleMem {
    pub fn is_io(&self, addr: u32) -> bool { addr >= self.io_base }
    fn rom_index(&self, addr: u32) -> Option<usize> {
        let i = (addr.wrapping_sub(ROM_BASE) >> 2) as usize;
        (i < self.rom.len()).then_some(i)
    }
    fn ram_read(&self, addr: u32) -> u32 {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        match self.ram.get(idx) {
            Some(&v) => v,
            None => self.ram_extra.get(&(addr & !3)).copied().unwrap_or(0),
        }
    }
    fn ram_write(&mut self, addr: u32, val: u32) {
        let idx = ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        if idx < self.ram.len() { self.ram[idx] = val; }
        else { self.ram_extra.insert(addr & !3, val); }
    }
    /// The data side: the I/O register or RAM.
    fn data_read(&self, addr: u32) -> u32 {
        if self.is_io(addr) { self.io_data } else { self.ram_read(addr) }
    }
}

/// Fetch reads ROM; loads / stores reach RAM or the I/O register. `peek` /
/// `poke` see ROM below the end of ROM, then the data side.
impl Bus for SingleMem {
    fn read(&mut self, addr: u32, port: Port) -> u32 {
        match port {
            Port::Fetch => self.rom_index(addr).map_or(0, |i| self.rom[i]),
            _ => self.data_read(addr),
        }
    }
    fn write(&mut self, addr: u32, word: u32, _port: Port) {
        if self.is_io(addr) { self.io_data = word; } else { self.ram_write(addr, word); }
    }
    fn peek(&self, addr: u32) -> u32 {
        match self.rom_index(addr) { Some(i) => self.rom[i], None => self.data_read(addr) }
    }
    fn poke(&mut self, addr: u32, word: u32) {
        match self.rom_index(addr) { Some(i) => self.rom[i] = word, None => self.write(addr, word, Port::Store) }
    }
}

#[cfg_attr(feature = "wasm", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", serde(default))]
#[derive(Debug, Clone)]
pub struct CpuState {
    pub pc: u32,
    pub registers: [u32; 32],
    /// ROM, RAM and the I/O register.
    pub mem: SingleMem,
    /// Traps on: IrqCheck may take interrupts; ECALL / EBREAK / MRET / SRET /
    /// illegal SYSTEM trap. Off: they are no-ops (PC + 4), IRQs are ignored.
    pub traps_enabled: bool,
    /// Zmmul on: MUL / MULH / MULHSU / MULHU run on the MUL block.
    /// Off: they are illegal (trap with traps on, NOP with traps off).
    pub zmmul: bool,
    pub cycle_count: u64,
    pub halted: bool,
    /// Live phase-by-phase state for the visualizer.
    pub micro: MicroState,
    /// Drivers that fired this micro-step (bright green in the visualizer).
    #[cfg_attr(feature = "wasm", serde(skip))]
    pub last_active: Vec<&'static str>,
    /// Control signals consumed (sampled) by the phase that just ran.
    #[cfg_attr(feature = "wasm", serde(skip))]
    pub last_consumed: Vec<&'static str>,
    /// Register indices touched (read or written) by the current phase.
    pub last_reg_access: Vec<u8>,
    /// RAM byte addresses touched by the current phase.
    pub last_ram_access: Vec<u32>,
    /// ROM byte address touched by the current phase (Fetch only).
    pub last_rom_access: Option<u32>,

    // ── Privilege, CSRs, traps, IRQs (shared system unit) ──────────────
    pub sys: SysUnit,
}

impl Default for CpuState {
    fn default() -> Self {
        Self {
            pc: 0,
            registers: [0; 32],
            mem: SingleMem::default(),
            traps_enabled: true,
            zmmul: true,
            cycle_count: 0,
            halted: false,
            micro: MicroState::default(),
            last_active: Vec::new(),
            last_consumed: Vec::new(),
            last_reg_access: Vec::new(),
            last_ram_access: Vec::new(),
            last_rom_access: None,
            sys: SysUnit::new(),
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

#[cfg(feature = "wasm")]
impl CpuState {
    /// Restore a saved state (JSON). Also reads saves from before `mem`
    /// existed (`rom`, `ram`, `io_data`, `io_base`, `ram_extra` at the top).
    pub fn from_json(json: &str) -> Option<CpuState> {
        let mut v: serde_json::Value = serde_json::from_str(json).ok()?;
        if let Some(o) = v.as_object_mut() {
            if !o.contains_key("mem") {
                let mut m = serde_json::Map::new();
                for k in ["rom", "ram", "io_data", "io_base", "ram_extra"] {
                    if let Some(x) = o.remove(k) { m.insert(k.into(), x); }
                }
                o.insert("mem".into(), m.into());
            }
        }
        serde_json::from_value(v).ok()
    }
}

impl CpuState {
    pub fn new() -> Self { Self::default() }

    /// Core with the given ROM / RAM sizes in words (the panel uses 64 / 64).
    pub fn with_sizes(rom_words: usize, ram_words: usize) -> Self {
        Self { mem: SingleMem { rom: vec![0x0000_0013; rom_words], ram: vec![0; ram_words], ..SingleMem::default() }, ..Self::default() }
    }

    /// RAM words outside the window, (address, value).
    pub fn ram_extra_snapshot(&self) -> Vec<(u32, u32)> {
        self.mem.ram_extra.iter().map(|(&a, &v)| (a, v)).collect()
    }

    pub fn load_word(&mut self, byte_addr: u32, word: u32) {
        let idx = ((byte_addr - ROM_BASE) >> 2) as usize;
        if idx < self.mem.rom.len() { self.mem.rom[idx] = word; }
    }

    pub fn load_program(&mut self, words: &[u32]) {
        for (i, w) in words.iter().enumerate() {
            if i < self.mem.rom.len() { self.mem.rom[i] = *w; }
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
        if idx < self.mem.rom.len() { self.mem.rom[idx] = val; }
    }

    /// Patch one RAM word in place (byte address).
    pub fn poke_ram(&mut self, byte_addr: u32, val: u32) {
        let idx = ((byte_addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize;
        if idx < self.mem.ram.len() { self.mem.ram[idx] = val; }
    }

    pub fn registers_snapshot(&self) -> [u32; 32] { self.registers }
    pub fn rom_snapshot(&self) -> &[u32] { &self.mem.rom }
    pub fn ram_snapshot(&self) -> &[u32] { &self.mem.ram }

    // ── Trap / privilege API ───────────────────────────────────────────
    pub fn priv_mode(&self) -> PrivMode { self.sys.priv_mode }
    pub fn last_trap(&self) -> Option<TrapInfo> { self.sys.last_trap }
    pub fn csr_read(&self, addr: u16) -> u32 { self.sys.csrs.read(addr) }
    pub fn csr_write(&mut self, addr: u16, val: u32) { self.sys.csrs.write(addr, val); }
    pub fn raise_irq(&mut self, mask: u32) { self.sys.raise_irq(mask); }
    pub fn clear_irq(&mut self) { self.sys.clear_irq(); }
    pub fn csrs_snapshot(&self) -> Vec<(u16, u32)> { self.sys.snapshot() }

    /// Sized load / store through the load-store unit. funct3 gives the
    /// width (B / H / W, U = zero-extend). A misaligned address is an
    /// exception with `trap_misaligned`, otherwise its low bits are ignored.
    fn mem_access(&mut self, read: bool, write: bool, addr: u32, rs2: u32, funct3: u8,
                  trap_misaligned: bool) -> Result<MemResult, (u32, u32)> {
        let bytes = 1u32 << (funct3 & 3).min(2);
        let mut addr = addr;
        if addr & (bytes - 1) != 0 {
            if trap_misaligned {
                let cause = if write { EXC_STORE_ADDR_MISALIGNED } else { EXC_LOAD_ADDR_MISALIGNED };
                return Err((cause, addr));
            }
            addr &= !(bytes - 1);
        }
        let mut r = MemResult::default();
        if read {
            let word = self.mem.read(addr, Port::Load);
            r.word = word;
            r.data = lsu::load(word, addr, funct3);
        }
        if write {
            let (wdata, bw) = lsu::store(rs2, addr, funct3);
            self.mem.write_lanes(addr, wdata, bw, Port::Store);
            r.bw = bw;
        }
        Ok(r)
    }

    /// R-type with funct7 = 0000001 and funct3 0..3 (MUL / MULH / MULHSU /
    /// MULHU). DIV / REM (funct3 4..7) are not in Zmmul: illegal.
    fn is_mul_instr(instr: u32) -> bool { matches!(decode(instr), Instr::Mul { .. }) }

    /// Not an instruction of this core: RV32I + Zicsr + M-mode / S-mode
    /// returns + Zmmul (when on). FENCE / FENCE.I / WFI are legal no-ops.
    fn is_illegal(&self, instr: u32) -> bool {
        match decode(instr) {
            Instr::Mul { .. } => !self.zmmul,
            i if i.is_base_rv32i() => false,
            Instr::Fence | Instr::FenceI | Instr::Wfi | Instr::Csr { .. }
                | Instr::Ecall | Instr::Ebreak | Instr::Mret | Instr::Sret => false,
            _ => true,
        }
    }

    /// Execute one instruction and return the (phase → active components) trace.
    /// Faithful port of cpuState.ts::step() lines 212–390.
    pub fn step_traced(&mut self) -> ActivityTrace {
        let mut trace = ActivityTrace::new();
        if self.halted { return trace; }
        self.cycle_count += 1;

        // ── 1. Fetch ─────────────────────────────────────────────────────────
        let instr = self.mem.read(self.pc, Port::Fetch);
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

        let alu_ctrl_code = alu_control_code(sig.alu_op, funct3, funct7b30, opcode);

        let AluResultLocal { result: alu_result, flags } =
            { let alu_a = if crate::sim::decode::is_auipc(opcode) { self.pc } else { rs1_val };
              let r = alu_compute(alu_ctrl_code, alu_a, alu_b);
              AluResultLocal { result: r.result, flags: r.flags } };

        trace.mark(Phase::Alu, nl::ALU_CTRL);
        trace.mark(Phase::Alu, nl::MUX_ALUSRC);
        trace.mark(Phase::Alu, nl::ALU);

        // ── 7. Memory access ────────────────────────────────────────────────
        let mut mem_read_data = 0u32;
        if sig.mem_read || sig.mem_write {
            trace.mark(Phase::Mem, nl::LOADSTORE);
            trace.mark(Phase::Mem, if self.mem.is_io(alu_result) { nl::IO } else { nl::RAM });
            if let Ok(r) = self.mem_access(sig.mem_read, sig.mem_write, alu_result, rs2_val, funct3, false) {
                mem_read_data = r.data;
            }
        }
        let is_mul = self.zmmul && Self::is_mul_instr(instr);

        // ── 8/9. Write-back mux + register write (same clock edge) ─────────
        let wb_data = if is_mul { mul(funct3 & 3, rs1_val, rs2_val) } else {
            match sig.wb_sel {
                0 => alu_result,
                1 => mem_read_data,
                2 => pc_plus4,
                _ => 0,
            }
        };
        if sig.reg_write && (self.zmmul || !Self::is_mul_instr(instr)) {
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
            decide_branch(funct3, &flags)
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

/// Result of a load / store through the load-store unit.
#[derive(Default)]
struct MemResult { data: u32, word: u32, bw: u8 }

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
                if (mr || mw) && self.micro.exc.is_none() && !self.mem.is_io(self.micro.alu_result.unwrap_or(0)) {
                    if let Some(addr) = self.micro.alu_result {
                        self.last_ram_access.push(addr & !3);
                    }
                }
            }
            Phase::Wb => {
                if self.micro.ctrl_reg_write.unwrap_or(false) && self.micro.exc.is_none() {
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
        if !self.traps_enabled { self.micro.trap_taken = false; return; }
        if let Some(cause) = self.sys.sample_irq() {
            self.pc = self.sys.trap(cause, true, self.pc, 0, self.cycle_count);
            self.micro.trap_taken = true;
        } else {
            self.micro.trap_taken = false;
        }
    }

    fn phase_fetch(&mut self) {
        // cycle / instret counters advance at fetch (as the visualizer's engine)
        self.sys.csrs.bump(crate::isa::csr::addr::CYCLE);
        self.sys.csrs.bump(crate::isa::csr::addr::INSTRET);
        let pc = self.pc;
        let instr = self.mem.read(pc, Port::Fetch);
        self.micro.pc_out = Some(pc);
        self.micro.instr = Some(instr);
        self.micro.pc_plus4 = Some(pc.wrapping_add(4));
        self.micro.exc = None;
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
        } else {
            // Unknown opcode: all controls deasserted → NOP (PC + 4)
            self.micro.ctrl_reg_write = Some(false);
            self.micro.ctrl_alu_src   = Some(false);
            self.micro.ctrl_mem_read  = Some(false);
            self.micro.ctrl_mem_write = Some(false);
            self.micro.ctrl_alu_op    = Some(0);
            self.micro.ctrl_wb_sel    = Some(0);
            self.micro.ctrl_pc_sel    = Some(0);
            self.micro.ctrl_imm_sel   = Some(5);
        }
        let instr = self.micro.instr.unwrap_or(0);
        let mul = Self::is_mul_instr(instr);
        self.micro.is_mul = Some(mul && self.zmmul);
        self.micro.ctrl_alu_a_pc = Some(crate::sim::decode::is_auipc(opcode));
        if mul && self.zmmul {
            self.micro.ctrl_wb_sel = Some(4);            // WB Sel ← MUL block
        } else if self.is_illegal(instr) {
            // Illegal instruction (unknown opcode, reserved encoding, DIV /
            // REM, MUL with Zmmul off): all controls deasserted (NOP);
            // with traps on, PcUpdate takes the exception.
            self.micro.ctrl_mem_read  = Some(false);
            self.micro.ctrl_mem_write = Some(false);
            self.micro.ctrl_reg_write = Some(false);
            self.micro.ctrl_alu_src   = Some(false);
            self.micro.ctrl_alu_op    = Some(0);
            self.micro.ctrl_wb_sel    = Some(0);
            self.micro.ctrl_pc_sel    = Some(0);
            self.micro.ctrl_imm_sel   = Some(5);
            if self.traps_enabled { self.micro.exc = Some((EXC_ILLEGAL_INSTR, instr)); }
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
        // ALU Src mux is combinational: its output is valid once the operands are
        let imm = self.micro.immediate.unwrap_or(0);
        self.micro.alu_b = Some(if self.micro.ctrl_alu_src.unwrap_or(false) { imm } else { self.registers[rs2] });
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

        let f7b30 = ((self.micro.instr.unwrap_or(0) >> 30) & 0x1) as u8;
        let code = alu_control_code(alu_op, funct3, f7b30, opcode);
        self.micro.alu_control = Some(code);

        // ALU A: rs1, or the PC for AUIPC
        let alu_a = if self.micro.ctrl_alu_a_pc.unwrap_or(false) { self.micro.pc_out.unwrap_or(self.pc) } else { rs1_val };
        self.micro.alu_a = Some(alu_a);
        let r = alu_compute(code, alu_a, alu_b);
        self.micro.alu_result    = Some(r.result);
        self.micro.zero_flag     = Some(r.flags.zero);
        self.micro.neg_flag      = Some(r.flags.negative);
        self.micro.carry_flag    = Some(r.flags.carry);
        self.micro.overflow_flag = Some(r.flags.overflow);

        // ── MUL block (combinational, beside the ALU) ──
        if self.micro.is_mul.unwrap_or(false) {
            self.micro.mul_result = Some(mul(funct3 & 3, rs1_val, rs2_val));
        }

        // ── CSR ops (Zicsr): read old, write new in execute; rd ← old in WB ──
        // (funct3 = 100 is reserved: no CSR access, rd ← 0)
        if opcode == 0b1110011 && funct3 & 0b011 != 0 {
            let instr    = self.micro.instr.unwrap_or(0);
            let csr_addr = ((instr >> 20) & 0xfff) as u16;
            let rs1      = self.micro.rs1_num.unwrap_or(0);
            let old = self.sys.csr(csr_addr, funct3, rs1, rs1_val).old;
            self.micro.csr_read_data = Some(old);
        }
    }

    fn phase_mem(&mut self) {
        let mem_read  = self.micro.ctrl_mem_read.unwrap_or(false);
        let mem_write = self.micro.ctrl_mem_write.unwrap_or(false);
        let addr      = self.micro.alu_result.unwrap_or(0);
        let rs2_val   = self.micro.rs2_data.unwrap_or(0);

        let funct3    = self.micro.funct3.unwrap_or(2);
        if !(mem_read || mem_write) { return; }

        match self.mem_access(mem_read, mem_write, addr, rs2_val, funct3, self.traps_enabled) {
            Ok(r) => {
                if mem_read {
                    self.micro.mem_word = Some(r.word);
                    self.micro.mem_read_data = Some(r.data);
                }
                if mem_write { self.micro.mem_bw = Some(r.bw); }
            }
            Err(e) => self.micro.exc = Some(e),    // misaligned: no access
        }
    }

    fn phase_wb(&mut self) {
        let opcode = self.micro.opcode.unwrap_or(0);
        let funct3 = self.micro.funct3.unwrap_or(0);

        // ── CSR ops (Zicsr): opcode SYSTEM (1110011) with funct3 != 0 ───
        if opcode == 0b1110011 && funct3 != 0 {
            let old = self.micro.csr_read_data.unwrap_or(0);
            // rd ← old (rd=0 is the no-write case for the regfile); an
            // illegal SYSTEM encoding (funct3 100) writes nothing
            let rd = self.micro.rd.unwrap_or(0);
            if rd != 0 && self.micro.exc.is_none() { self.registers[rd as usize] = old; }
            self.micro.write_data = Some(old);
            return;
        }

        // ── Ordinary write-back ─────────────────────────────────────────
        let wb_sel = self.micro.ctrl_wb_sel.unwrap_or(0);
        let wb_data = match wb_sel {
            0 => self.micro.alu_result.unwrap_or(0),
            1 => self.micro.mem_read_data.unwrap_or(0),
            2 => self.micro.pc_plus4.unwrap_or(0),
            4 => self.micro.mul_result.unwrap_or(0),
            _ => 0,
        };
        self.micro.write_data = Some(wb_data);

        // An exception (misaligned load) suppresses the register write
        if self.micro.ctrl_reg_write.unwrap_or(false) && self.micro.exc.is_none() {
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
        let overflow = self.micro.overflow_flag.unwrap_or(false);

        // ── Exception raised earlier in this instruction ──
        if let Some((cause, tval)) = self.micro.exc {
            self.take_exception(cause, tval);
            return;
        }

        // ── SYSTEM funct3=0: ECALL/EBREAK/MRET/SRET (no-ops with traps off) ──
        if self.traps_enabled && opcode == 0b1110011 && funct3 == 0 {
            let d = decode(self.micro.instr.unwrap_or(0));

            if d == Instr::Mret {
                let epc = self.sys.xret(false);
                self.pc = epc;
                self.micro.next_pc = Some(epc);
                self.cycle_count += 1;
                return;
            }
            if d == Instr::Sret {
                let epc = self.sys.xret(true);
                self.pc = epc;
                self.micro.next_pc = Some(epc);
                self.cycle_count += 1;
                return;
            }
            // ECALL / EBREAK take an exception trap.
            let cause = match d {
                Instr::Ecall => Some(self.sys.ecall_cause()),
                Instr::Ebreak => Some(EXC_BREAKPOINT),
                _ => None,
            };
            if let Some(c) = cause {
                let epc = self.pc; // ecall: epc = pc of the ecall instruction
                let new_pc = self.sys.trap(c, false, epc, 0, self.cycle_count);
                self.pc = new_pc;
                self.micro.next_pc = Some(new_pc);
                self.micro.trap_taken = true;
                self.cycle_count += 1;
                return;
            }
            // WFI: no-op (other SYSTEM encodings were illegal at decode)
        }

        let branch_taken = if pc_sel != 1 { false }
        else if opcode == 0b1101111 { true }
        else {
            decide_branch(funct3, &AluFlags { zero, negative: neg, carry, overflow })
        };
        let is_jalr = pc_sel == 2;
        let pc_offset = if branch_taken { imm } else { 4 };
        self.micro.pc_offset = Some(pc_offset);
        let adder = self.pc.wrapping_add(pc_offset);
        self.micro.pc_adder_result = Some(adder);
        self.micro.offset_sel = Some(branch_taken);
        self.micro.is_jalr = Some(is_jalr);
        let jalr_target = self.micro.alu_result.unwrap_or(0) & 0xffff_fffe;
        self.micro.jalr_target = Some(jalr_target);
        let next = if is_jalr { jalr_target } else { adder };
        self.micro.next_pc = Some(next);

        self.pc = next;
        self.cycle_count += 1;
    }

    fn take_exception(&mut self, cause: u32, tval: u32) {
        let epc = self.pc;
        let new_pc = self.sys.trap(cause, false, epc, tval, self.cycle_count);
        self.pc = new_pc;
        self.micro.next_pc = Some(new_pc);
        self.micro.trap_taken = true;
        self.cycle_count += 1;
    }

    pub fn in_trap(&self) -> bool { self.sys.in_trap() }
}
