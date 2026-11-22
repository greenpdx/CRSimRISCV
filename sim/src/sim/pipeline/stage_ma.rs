//! Pipeline stage 4 — Memory Access (MA).
//!
//! For loads, reads RAM (or IO) at `alu_result` and applies the
//! load-width sign/zero extension. For stores, writes RAM (or IO).
//! Produces the new MA/WB register contents.
//!
//! Always-active components: `ma_wb_reg`, plus `ram` or `io` and
//! `loadstore` if memory access.

use crate::isa::lsu;
use super::state::{ControlBits, CsrOp, ExMa, MaWb, MaWbData, PipelineState, TrapKind};
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
    let needs_xlat = ex_ma.ctrl.mem.is_access();
    let phys_addr = if needs_xlat && !state.io_at(vaddr) {
        let access = if ex_ma.ctrl.mem.is_store() { AccessKind::Store } else { AccessKind::Load };
        match translate(state, vaddr, access) {
            Ok(p) => p,
            Err(cause) => {
                let trap_kind = if cause == 13 { TrapKind::LoadPageFault } else { TrapKind::StorePageFault };
                return MaWb::new(ex_ma.pc, ex_ma.instr, MaWbData {
                    pc_plus4: ex_ma.pc_plus4,
                    ctrl: ControlBits::default(),
                    trap_kind,
                    trap_value: vaddr,
                    ..MaWbData::default()
                });
            }
        }
    } else {
        vaddr
    };
    let addr = phys_addr;

    if ex_ma.ctrl.mem.is_load() {
        let raw = if state.io_at(vaddr) {
            state.mem.io_data()
        } else {
            state.priv_ram_read(addr)
        };
        mem_data = lsu::load(raw, addr, funct3);
    }

    // Zicsr: CSR read/modify/write happens in MA. The old value goes
    // into `mem_data` so the WB stage can write it to rd via wb_sel=3.
    if ex_ma.ctrl.csr_op != CsrOp::None {
        // Shared Zicsr unit: funct3[1:0] = the op; rs1 = 0 marks "no
        // write" for RS / RC (rs1 = x0 or zimm = 0, decided in EX).
        let addr = ex_ma.csr_addr;
        let f3 = ex_ma.ctrl.csr_op.to_u8();
        let rs1_marker = if ex_ma.csr_src_is_zero { 0 } else { 1 };
        let r = state.sys.csr(addr, f3, rs1_marker, ex_ma.csr_write_val);
        mem_data = r.old;
        // A satp write does not flush the TLBs (as in the spec and CVA6):
        // entries are tagged with their ASID; software runs SFENCE.VMA.
    }

    // SFENCE.VMA rs1, rs2: rs1 ≠ x0 → only that page; rs2 ≠ x0 → only
    // that ASID (global entries kept). rs1 / rs2 = x0 → all.
    if ex_ma.ctrl.is_sfence {
        let rs1 = (ex_ma.instr >> 15) & 0x1f;
        let rs2 = (ex_ma.instr >> 20) & 0x1f;
        let va = (rs1 != 0).then_some(ex_ma.csr_write_val);
        let asid = (rs2 != 0).then_some((ex_ma.rs2_data & 0x1ff) as u16);
        state.sfence(va, asid);
    }

    if ex_ma.ctrl.mem.is_store() {
        // SW writes the word; SB / SH merge their byte lanes into the word
        // already there (read without a second memory access).
        let sub_word = ex_ma.funct3 & 3 != 2;
        if state.io_at(vaddr) {
            let io = state.mem.io_data();
            state.mem.set_io_data(if sub_word { lsu::store_merge(io, ex_ma.rs2_data, vaddr, ex_ma.funct3) } else { ex_ma.rs2_data });
        } else {
            let word = if sub_word { lsu::store_merge(state.word_peek(addr), ex_ma.rs2_data, addr, ex_ma.funct3) } else { ex_ma.rs2_data };
            state.priv_ram_write(addr, word);
        }
    }

    MaWb::new(ex_ma.pc, ex_ma.instr, MaWbData {
        alu_result: ex_ma.alu_result,
        mem_data,
        pc_plus4: ex_ma.pc_plus4,
        rd_num: ex_ma.rd_num,
        ctrl: ex_ma.ctrl,
        ..MaWbData::default()
    })
}
