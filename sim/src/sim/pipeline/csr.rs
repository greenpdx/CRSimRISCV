//! The CSRs the pipeline view lists. Addresses, CSR ops and Zicsr
//! arithmetic are `isa::csr`; storage, trap entry / return and IRQ sampling
//! are the shared system unit (`crate::sim::system::SysUnit`). PMP CSRs are
//! storage only (no enforcement).

use crate::isa::csr::addr::*;

/// The CSRs the pipeline view lists (wasm snapshot), in order.
pub fn known_addrs() -> &'static [u16] {
    &[
        MSTATUS, MISA, MIE, MTVEC, MSCRATCH,
        MEPC, MCAUSE, MTVAL, MIP,
        MVENDORID, MARCHID, MIMPID, MHARTID,
        PMPCFG0, PMPCFG1,
        PMPADDR0, PMPADDR0 + 1, PMPADDR0 + 2, PMPADDR0 + 3,
        PMPADDR0 + 4, PMPADDR0 + 5, PMPADDR0 + 6, PMPADDR0 + 7,
        SSTATUS, SIE, STVEC, SSCRATCH, SEPC,
        SCAUSE, STVAL, SIP, SATP,
        MEDELEG, MIDELEG,
    ]
}
