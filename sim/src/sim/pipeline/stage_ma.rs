//! Pipeline stage 4 — Memory Access (MA).
//!
//! For loads, reads RAM (or IO) at `alu_result` and applies the
//! load-width sign/zero extension. For stores, writes RAM (or IO).
//! Produces the new MA/WB register contents.
//!
//! Always-active components: `ma_wb_reg`, plus `ram` or `io` and
//! `loadstore` if memory access.

use crate::data::load_width::sign_extend_load;
use super::csr::CsrOp;
use super::state::{ExMa, MaWb, PipelineState};
use super::translate::{translate, AccessKind};

/// Compute the MA stage outputs and apply any side effects (RAM/IO
/// writes). Returns the new MA/WB register contents.
pub fn compute_ma(state: &mut PipelineState, ex_ma: &ExMa) -> MaWb {
    if !ex_ma.valid {
        let mut bubble = MaWb::bubble();
        bubble.stall_reg = ex_ma.stall_reg;
        bubble.flush_bubble = ex_ma.flush_bubble;
        return bubble;
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
                    stall_reg: 0,
                    flush_bubble: false,
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
            state.priv_ram_read(addr)
        };
        mem_data = sign_extend_load(raw, addr, funct3);
    }

    // Zicsr: CSR read/modify/write happens in MA. The old value goes
    // into `mem_data` so the WB stage can write it to rd via wb_sel=3.
    if ex_ma.ctrl.csr_op != 0 {
        // Shared Zicsr unit: funct3[1:0] = the op; rs1 = 0 marks "no
        // write" for RS / RC (rs1 = x0 or zimm = 0, decided in EX).
        let addr = ex_ma.csr_addr;
        let f3 = CsrOp::from_u8(ex_ma.ctrl.csr_op).to_u8();
        let rs1_marker = if ex_ma.csr_src_is_zero { 0 } else { 1 };
        let r = state.sys.csr(addr, f3, rs1_marker, ex_ma.csr_write_val);
        mem_data = r.old;
        // Phase 4.4: writes to satp invalidate the entire TLB.
        if r.write && addr == super::csr::CSR_SATP {
            state.tlb.flush();
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
            state.priv_ram_write(addr, ex_ma.rs2_data);
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
        stall_reg: 0,
        flush_bubble: false,
    }
}
