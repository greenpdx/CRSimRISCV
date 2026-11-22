//! Example programs for the memory panel. Each one loads physical memory
//! (code, data, page tables) and sets the cores' start state. Core 1 is
//! parked at 0x40 unless the example uses both cores.

use super::system::MemPanelSystem;
use super::ROM_BASE;
use crate::isa::csr::addr;
use crate::sim::system::PrivMode;

pub struct Example {
    pub name: &'static str,
    /// What to watch for.
    pub about: &'static str,
    pub setup: fn(&mut MemPanelSystem),
}

use crate::isa::encode::*;

/// Code at 0: hart 1 jumps to 0x40, hart 0 falls into `core0` at 0x08.
fn split(core0: &[u32], core1: &[u32]) -> Vec<u32> { split_at(core0, core1, 0x40) }

/// Same, with hart 1's code at `at` (room for a longer core 0 program).
fn split_at(core0: &[u32], core1: &[u32], at: u32) -> Vec<u32> {
    let n = (at / 4) as usize;
    let mut p = vec![csrr(5, 0xf14), bne(5, 0, at as i32 - 4)];
    p.extend_from_slice(core0);
    assert!(p.len() <= n, "core 0 code overlaps core 1's");
    p.resize(n, NOP);
    p.extend_from_slice(core1);
    p
}

// PTE bits
const V: u32 = 1; const R: u32 = 2; const W: u32 = 4; const X: u32 = 8; const A: u32 = 64; const D: u32 = 128;
const G: u32 = 32;
const fn pte(pa: u32, flags: u32) -> u32 { ((pa >> 12) << 10) | flags }
/// satp value: Sv32, `asid`, root table at `root`.
const fn satp(asid: u32, root: u32) -> u32 { (1 << 31) | (asid << 22) | (root >> 12) }

/// Load `v` into `rd` (lui + addi).
fn li(rd: u32, v: u32) -> [u32; 2] {
    let lo = ((v & 0xfff) as i32) << 20 >> 20;
    [lui(rd, v.wrapping_sub(lo as u32) >> 12), addi(rd, rd, lo)]
}

/// Like `paging`, but the root table at 0x8000 belongs to ASID `asid`,
/// the code page is global (G) and the data leaf table 0xa000 maps
/// VA 0x0040_n000 → `pages[n-1]` (n = 1, 2, …; flags RW AD, + G if asked).
fn paging_asid(sys: &mut MemPanelSystem, asid: u32, pages: &[(u32, bool)]) {
    paging(sys);
    sys.load(0x9000, &[pte(0x0000, V | R | X | A | G)]);
    for (i, &(pa, g)) in pages.iter().enumerate() {
        sys.load(0xa004 + 4 * i as u32, &[pte(pa, V | R | W | A | D | if g { G } else { 0 })]);
    }
    for c in sys.cores.iter_mut() { c.sys.csrs.write(addr::SATP, satp(asid, 0x8000)); }
}

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

/// Boot ROM image (at ROM_BASE): both harts start here in M-mode, bare.
/// Hart 1 writes a `j .` into RAM at 0x40 and parks there. Hart 0 copies
/// the program (stored in ROM at +0x100) to RAM 0x1000, writes the page
/// tables with stores, turns on Sv32 (ASID 1) and `mret`s into S-mode at
/// VA 0x0040_0000. Returns (rom words, program words).
pub fn boot_rom() -> (Vec<u32>, Vec<u32>) {
    // User program, linked at VA 0x0040_0000 (IMEM); data at VA 0x1001_0000 (DMEM)
    let prog = vec![
        lui(10, 0x10010),                   // x10 = DMEM
        addi(6, 0, 5), addi(7, 0, 0),       // i = 5, sum = 0
        add(7, 7, 6), addi(6, 6, -1), bne(6, 0, -8),   // sum = 5+4+3+2+1
        sw(7, 10, 0),                       // DTLB miss → walk, sets D
        lw(8, 10, 0),                       // DTLB hit: x8 = 15
        SPIN,
    ];
    let mut h0 = vec![
        // copy the program: ROM+0x100 → RAM 0x1000
        lui(10, ROM_BASE >> 12), addi(10, 10, 0x100), lui(11, 1), addi(12, 0, prog.len() as i32),
        lw(13, 10, 0), sw(13, 11, 0), addi(10, 10, 4), addi(11, 11, 4), addi(12, 12, -1), bne(12, 0, -20),
    ];
    // page tables, written with stores: root 0x8000, leaf tables 0x9000 (IMEM) and 0xa000 (DMEM)
    for (pte_addr, pte_val) in [
        (0x8000 + 4 * 0x001, pte(0x9000, V)),               // root[VPN1 = 0x001] → 0x9000
        (0x8000 + 4 * 0x040, pte(0xa000, V)),               // root[VPN1 = 0x040] → 0xa000
        (0x9000 + 4 * 0x000, pte(0x1000, V | R | X | A)),    // VA 0x0040_0000 → PA 0x1000 (code)
        (0xa000 + 4 * 0x010, pte(0x2000, V | R | W | A)),    // VA 0x1001_0000 → PA 0x2000 (data, D clear)
    ] {
        h0.extend(li(5, pte_val));
        h0.extend([lui(6, pte_addr >> 12), sw(5, 6, (pte_addr & 0xfff) as i32)]);
    }
    // paging on: satp, MPP = S, mepc = entry, flush, mret
    h0.extend(li(5, satp(1, 0x8000)));
    h0.push(csrw(0x180, 5));
    h0.extend(li(5, 0x800));
    h0.push(csrw(0x300, 5));                                       // mstatus: MPP = S
    h0.extend([lui(5, 0x400), csrw(0x341, 5)]);                    // mepc = 0x0040_0000
    h0.extend([sfence(0, 0), FENCE_I, MRET]);
    // hart 1: park in RAM
    let h1 = [addi(6, 0, SPIN as i32), sw(6, 0, 0x40), FENCE_I, jalr(0, 0, 0x40)];
    let mut rom = vec![csrr(5, 0xf14), bne(5, 0, 4 * (h0.len() as i32 + 1))];
    rom.extend(h0);
    rom.extend(h1);
    assert!(rom.len() <= 0x40, "boot code overlaps the program image");
    rom.resize(0x40, NOP);
    rom.extend(&prog);
    (rom, prog)
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            name: "Boot — ROM, copy, page tables, paging on",
            about: "Reset: both harts start in M-mode at the boot ROM (0xF000_0000, uncached — every fetch is a bus read, 8 cycles). Hart 1 parks in RAM. Hart 0 copies the program from ROM to RAM 0x1000 (the stores fill L1D / L2; memory stays stale), writes the page tables with stores, sets satp (Sv32, ASID 1), MPP = S, mepc = 0x0040_0000, runs SFENCE.VMA + FENCE.I and MRETs. The first fetch in S-mode misses the ITLB; the walker finds the PTEs in the caches (just written by the boot code — L2 after L1D set 0 overflowed), not in memory; the code line also comes from L2, while memory itself is still empty. The program sums 5..1 and stores 15 to VA 0x1001_0000 (DTLB miss, walk, D bit set).",
            setup: |s| {
                let (rom, _) = boot_rom();
                s.load(ROM_BASE, &rom);
                for c in s.cores.iter_mut() { c.pc = ROM_BASE; }
            },
        },
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
            name: "TLB — ASIDs and a global page",
            about: "Two address spaces map VA 0x0040_1000 differently: ASID 1 → PA 0x2000 (0x11), ASID 2 → PA 0x5000 (0x22). Switching satp does not flush the TLB: the ASID 2 load misses (ASID compare fails) and walks; switching back, the ASID 1 entry still hits. The code page is global (G), so the ITLB keeps hitting across both ASIDs.",
            setup: |s| {
                paging_asid(s, 1, &[(0x2000, false)]);
                // ASID 2: root 0xc000 shares the code table, own data table 0xd000
                s.load(0xc000, &[pte(0x9000, V), pte(0xd000, V)]);
                s.load(0xd004, &[pte(0x5000, V | R | W | A | D)]);
                s.load(0x2000, &[0x11]);
                s.load(0x5000, &[0x22]);
                let mut c0 = vec![lui(10, 0x401), lw(7, 10, 0)];
                c0.extend(li(5, satp(2, 0xc000))); c0.extend([csrw(0x180, 5), lw(8, 10, 0)]);
                c0.extend(li(5, satp(1, 0x8000))); c0.extend([csrw(0x180, 5), lw(9, 10, 0), SPIN]);
                s.load(0, &split_at(&c0, &[SPIN], 0x100));
            },
        },
        Example {
            name: "TLB — SFENCE.VMA, the four cases",
            about: "ASID 1 fills the DTLB with pages 0x401, 0x402, 0x403 and a global page 0x404. Then: sfence.vma x10, x0 (that page, any ASID) · sfence.vma x0, x14 (ASID 1, the global page survives — and the ITLB code entry goes too) · sfence.vma x11, x14 (that page in ASID 1) · sfence.vma x0, x0 (everything, global too). Each load after a flush shows hit or walk.",
            setup: |s| {
                paging_asid(s, 1, &[(0x2000, false), (0x3000, false), (0x4000, false), (0x6000, true)]);
                s.load(0x9000, &[pte(0x0000, V | R | X | A)]);   // code not global here: an ASID flush drops it
                s.load(0, &split_at(&[
                    lui(10, 0x401), lui(11, 0x402), lui(12, 0x403), lui(13, 0x404), addi(14, 0, 1),
                    lw(5, 10, 0), lw(5, 11, 0), lw(5, 12, 0), lw(5, 13, 0),
                    sfence(10, 0), lw(5, 10, 0),
                    sfence(0, 14), lw(5, 13, 0), lw(5, 11, 0),
                    sfence(11, 14), lw(5, 11, 0),
                    sfence(0, 0), lw(5, 13, 0), SPIN], &[SPIN], 0x100));
            },
        },
        Example {
            name: "TLB — full, PLRU replacement",
            about: "Loads from pages 0x401–0x404 fill all 4 DTLB entries; a load from 0x401 hits and moves the PLRU tree away from entry 0. The load from 0x405 needs a victim: the tree points at entry 2 (page 0x403) — not entry 1 (0x402), which true LRU would pick. Then 0x403 misses again and the tree picks the next victim.",
            setup: |s| {
                paging_asid(s, 1, &[(0x2000, false), (0x3000, false), (0x4000, false), (0x5000, false), (0x6000, false)]);
                s.load(0, &split_at(&[
                    lui(10, 0x401), lui(11, 0x402), lui(12, 0x403), lui(13, 0x404), lui(14, 0x405),
                    lw(5, 10, 0), lw(5, 11, 0), lw(5, 12, 0), lw(5, 13, 0),
                    lw(5, 10, 0), lw(5, 14, 0), lw(5, 12, 0), lw(5, 11, 0), SPIN], &[SPIN], 0x100));
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
