//! Example programs for the memory panel. Each one loads physical memory
//! (code, data, page tables) and sets the cores' start state. Core 1 is
//! parked at 0x40 unless the example uses both cores.

use super::system::MemPanelSystem;
use crate::sim::system::{addr, PrivMode};

pub struct Example {
    pub name: &'static str,
    /// What to watch for.
    pub about: &'static str,
    pub setup: fn(&mut MemPanelSystem),
}

// ── RV32I encoders ──────────────────────────────────────────────────────
pub mod enc {
    pub fn addi(rd: u32, rs1: u32, imm: i32) -> u32 { ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13 }
    pub fn lui(rd: u32, imm20: u32) -> u32 { (imm20 << 12) | (rd << 7) | 0x37 }
    pub fn lw(rd: u32, rs1: u32, off: i32) -> u32 { ((off as u32 & 0xfff) << 20) | (rs1 << 15) | (2 << 12) | (rd << 7) | 0x03 }
    pub fn sw(rs2: u32, rs1: u32, off: i32) -> u32 {
        let o = off as u32 & 0xfff;
        ((o >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (2 << 12) | ((o & 0x1f) << 7) | 0x23
    }
    pub fn add(rd: u32, rs1: u32, rs2: u32) -> u32 { (rs2 << 20) | (rs1 << 15) | (rd << 7) | 0x33 }
    fn branch(f3: u32, rs1: u32, rs2: u32, off: i32) -> u32 {
        let o = off as u32;
        ((o >> 12) & 1) << 31 | ((o >> 5) & 0x3f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | ((o >> 1) & 0xf) << 8 | ((o >> 11) & 1) << 7 | 0x63
    }
    pub fn beq(rs1: u32, rs2: u32, off: i32) -> u32 { branch(0, rs1, rs2, off) }
    pub fn bne(rs1: u32, rs2: u32, off: i32) -> u32 { branch(1, rs1, rs2, off) }
    pub fn csrr(rd: u32, csr: u32) -> u32 { (csr << 20) | (2 << 12) | (rd << 7) | 0x73 }
    pub const SPIN: u32 = 0x0000_006f;   // j .
    pub const NOP: u32 = 0x0000_0013;
    pub const MRET: u32 = 0x3020_0073;
    pub const SFENCE_VMA: u32 = 0x1200_0073;
}
use enc::*;

/// Code at 0: hart 1 jumps to 0x40, hart 0 falls into `core0` at 0x08.
fn split(core0: &[u32], core1: &[u32]) -> Vec<u32> {
    let mut p = vec![csrr(5, 0xf14), bne(5, 0, 0x40 - 4)];
    p.extend_from_slice(core0);
    assert!(p.len() <= 16, "core 0 code overlaps 0x40");
    p.resize(16, NOP);
    p.extend_from_slice(core1);
    p
}

// PTE bits
const V: u32 = 1; const R: u32 = 2; const W: u32 = 4; const X: u32 = 8; const A: u32 = 64; const D: u32 = 128;
const fn pte(pa: u32, flags: u32) -> u32 { ((pa >> 12) << 10) | flags }

/// Sv32 for both cores: root table 0x8000. VA page 0 → PA 0 (code, RX);
/// VA 0x0040_1000 → PA 0x2000 (data, RW). Cores run in S-mode, traps go
/// to the M-mode handler at 0x80.
fn paging(sys: &mut MemPanelSystem) {
    sys.load(0x8000, &[pte(0x9000, V), pte(0xa000, V), pte(0xb000, V)]); // root[0..2] → tables
    sys.load(0x9000, &[pte(0x0000, V | R | X | A)]);                       // VA 0x0000_0000 → code
    sys.load(0xa004, &[pte(0x2000, V | R | W | A | D)]);                   // VA 0x0040_1000 → data
    for c in sys.cores.iter_mut() {
        c.sys.priv_mode = PrivMode::S;
        c.sys.csrs.write(addr::SATP, (1 << 31) | (0x8000 >> 12));
        c.sys.csrs.write(addr::MTVEC, 0x80);
        c.sys.csrs.write(addr::MSTATUS, 1 << 11);   // MPP = S (so mret returns to S)
    }
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            name: "L1 hits — warm loop",
            about: "Core 0 sums a 4-word array (one line) 8 times. First pass: total misses on fetch and data; after that every fetch and load is an L1 hit.",
            setup: |s| {
                s.load(0x1000, &[1, 2, 3, 4]);
                s.load(0, &split(&[lui(10, 1), addi(6, 0, 8),
                    lw(7, 10, 0), lw(8, 10, 4), lw(9, 10, 8), lw(12, 10, 12),
                    add(11, 11, 7), add(11, 11, 8), add(11, 11, 9), add(11, 11, 12),
                    addi(6, 6, -1), bne(6, 0, -36), SPIN], &[SPIN]));
            },
        },
        Example {
            name: "L1 miss, L2 hit — L1 conflict",
            about: "Three lines (0x1000, 0x1040, 0x1080) all map to L1D set 0, which has 2 ways: each load evicts the oldest, so after the first round every load misses L1 and hits L2.",
            setup: |s| {
                s.load(0, &split(&[lui(10, 1), addi(6, 0, 6), lw(7, 10, 0), lw(8, 10, 0x40), lw(9, 10, 0x80),
                    addi(6, 6, -1), bne(6, 0, -16), SPIN], &[SPIN]));
            },
        },
        Example {
            name: "Total miss — L2 conflict",
            about: "Five lines 0x80 apart all map to L2 set 0 (4 ways) and L1D set 0: every round evicts from L2 too, so loads go all the way to memory (L1 miss, L2 miss).",
            setup: |s| {
                s.load(0, &split(&[lui(10, 1), addi(6, 0, 4), lw(7, 10, 0), lw(8, 10, 0x80), lw(9, 10, 0x100),
                    lw(12, 10, 0x180), lw(13, 10, 0x200), addi(6, 6, -1), bne(6, 0, -24), SPIN], &[SPIN]));
            },
        },
        Example {
            name: "MMU — TLB miss, walk, then hits",
            about: "S-mode with Sv32 paging. The first fetch and the first load each miss the TLB and walk the two-level page table (the PTE reads go through L1D / L2); after that the TLB hits. VA 0x0040_1000 → PA 0x2000.",
            setup: |s| {
                paging(s);
                s.load(0x2000, &[0x55, 0x66]);
                s.load(0, &split(&[lui(10, 0x401), addi(6, 0, 4), lw(7, 10, 0), lw(8, 10, 4),
                    addi(6, 6, -1), bne(6, 0, -12), SPIN], &[SPIN]));
            },
        },
        Example {
            name: "MMU — page fault, handler maps the page",
            about: "S-mode load from VA 0x0080_0000, which has no valid PTE: page fault (cause 13) into the M-mode handler at 0x80. It writes the PTE (→ PA 0x3000), runs SFENCE.VMA and MRETs; the load is retried, walks and succeeds (x7 = 0x77).",
            setup: |s| {
                paging(s);
                s.load(0x3000, &[0x77]);
                s.load(0, &split(&[lui(10, 0x800), lw(7, 10, 0), SPIN], &[SPIN]));
                // handler (M-mode, bare): leaf PTE at 0xb000 ← PA 0x3000 RW
                let leaf = pte(0x3000, V | R | W | A | D);          // 0xcc7
                s.load(0x80, &[lui(28, 0xb), lui(29, 1), addi(29, 29, leaf as i32 - 0x1000), sw(29, 28, 0), SFENCE_VMA, MRET]);
            },
        },
        Example {
            name: "Two cores — flag passing (MESI)",
            about: "Core 0 writes data (0x1000) then a flag (0x1010, another line). Core 1 spins on the flag: its reads snoop core 0's Modified lines (flush, M → S), and it reads 42.",
            setup: |s| {
                s.load(0, &split(
                    &[lui(10, 1), addi(6, 0, 42), sw(6, 10, 0), addi(7, 0, 1), sw(7, 10, 16), SPIN],
                    &[lui(10, 1), lw(7, 10, 16), beq(7, 0, -4), lw(8, 10, 0), SPIN]));
            },
        },
        Example {
            name: "Two cores — false sharing",
            about: "Each core increments its own counter, but both counters are in one line (0x1000 and 0x1004): every write needs the line in M, so it bounces between the L1Ds (BusRdX / BusUpgr, invalidations, flushes).",
            setup: |s| {
                s.load(0, &split(
                    &[lui(10, 1), addi(6, 0, 8), lw(7, 10, 0), addi(7, 7, 1), sw(7, 10, 0), addi(6, 6, -1), bne(6, 0, -16), SPIN],
                    &[lui(10, 1), addi(6, 0, 8), lw(7, 10, 4), addi(7, 7, 1), sw(7, 10, 4), addi(6, 6, -1), bne(6, 0, -16), SPIN]));
            },
        },
    ]
}
