//! Pipeline stage 4 — Memory Access (MA).
//!
//! For loads, reads RAM (or IO) at `alu_result` and applies the
//! load-width sign/zero extension. For stores, writes RAM (or IO).
//! Produces the new MA/WB register contents.
//!
//! Always-active components: `ma_wb_reg`, plus `ram` or `io` and
//! `loadstore` if memory access.

use super::csr::CsrOp;
use super::state::{ExMa, MaWb, PipelineState};
use super::translate::{translate, AccessKind};

/// Compute the MA stage outputs and apply any side effects (RAM/IO
/// writes). Returns the new MA/WB register contents.
pub fn compute_ma(state: &mut PipelineState, ex_ma: &ExMa) -> MaWb {
    if !ex_ma.valid {
        return MaWb::bubble();
    }

    let mut mem_data = 0u32;
    let vaddr = ex_ma.alu_result;
    let funct3 = ex_ma.funct3;

    // Sv32 translation for loads/stores. CSR ops and IO accesses skip
    // translation (IO is M-mode-only in our model). On fault, return
    // a MaWb carrying the trap kind so step() delivers the trap.
    let needs_xlat = ex_ma.ctrl.mem_read || ex_ma.ctrl.mem_write;
    let phys_addr = if needs_xlat && !PipelineState::is_io(vaddr) {
        let access = if ex_ma.ctrl.mem_write { AccessKind::Store } else { AccessKind::Load };
        match translate(state, vaddr, access) {
            Ok(p) => p,
            Err(cause) => {
                let trap_kind = if cause == 13 { 7 } else { 8 };
                return MaWb {
                    valid: true,
                    instr: ex_ma.instr,
                    alu_result: 0, mem_data: 0,
                    pc_plus4: ex_ma.pc_plus4,
                    rd_num: 0,
                    ctrl: super::state::ControlBits::default(),
                    trap_kind,
                    trap_pc: ex_ma.trap_pc,
                    trap_value: vaddr,
                };
            }
        }
    } else {
        vaddr
    };
    let addr = phys_addr;

    if ex_ma.ctrl.mem_read {
        let raw = if PipelineState::is_io(vaddr) {
            state.io_data
        } else {
            state.ram_read(addr)
        };
        // Apply sign/zero extension based on funct3 (LB/LH/LW/LBU/LHU)
        let byte_off = (addr & 0x3) * 8;
        mem_data = match funct3 {
            0b000 => {  // LB
                let v = (raw >> byte_off) & 0xff;
                if v & 0x80 != 0 { v | 0xffff_ff00 } else { v }
            }
            0b001 => {  // LH
                let v = (raw >> byte_off) & 0xffff;
                if v & 0x8000 != 0 { v | 0xffff_0000 } else { v }
            }
            0b010 => raw,  // LW
            0b100 => (raw >> byte_off) & 0xff,    // LBU
            0b101 => (raw >> byte_off) & 0xffff,  // LHU
            _     => raw,
        };
    }

    // Zicsr: CSR read/modify/write happens in MA. The old value goes
    // into `mem_data` so the WB stage can write it to rd via wb_sel=3.
    if ex_ma.ctrl.csr_op != 0 {
        let addr = ex_ma.csr_addr;
        let old = state.csrs.read(addr);
        mem_data = old;
        let new_val = match CsrOp::from_u8(ex_ma.ctrl.csr_op) {
            CsrOp::Rw => Some(ex_ma.csr_write_val),
            CsrOp::Rs => if ex_ma.csr_src_is_zero { None } else { Some(old |  ex_ma.csr_write_val) },
            CsrOp::Rc => if ex_ma.csr_src_is_zero { None } else { Some(old & !ex_ma.csr_write_val) },
            CsrOp::None => None,
        };
        if let Some(v) = new_val {
            state.csrs.write(addr, v);
            // Phase 4.4: writes to satp invalidate the entire TLB.
            if addr == super::csr::CSR_SATP {
                state.tlb.flush();
            }
        }
    }

    // Phase 4.4: SFENCE.VMA flushes the entire TLB (no ASID support).
    if ex_ma.ctrl.is_sfence {
        state.tlb.flush();
    }

    if ex_ma.ctrl.mem_write {
        if PipelineState::is_io(vaddr) {
            state.io_data = ex_ma.rs2_data;
        } else {
            // Sub-word writes are not modeled in the visualizer; treat
            // SB/SH/SW as full word writes for v1.
            state.ram_write(addr, ex_ma.rs2_data);
        }
    }

    MaWb {
        valid: true,
        instr: ex_ma.instr,
        alu_result: ex_ma.alu_result,
        mem_data,
        pc_plus4: ex_ma.pc_plus4,
        rd_num: ex_ma.rd_num,
        ctrl: ex_ma.ctrl,
        trap_kind: 0, trap_pc: 0, trap_value: 0,
    }
}
