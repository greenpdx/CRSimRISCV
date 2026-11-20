//! Memory panel machine: two pipeline cores on the shared cache hierarchy.
//! Programs run from physical memory; their fetches, loads / stores and
//! page walks go through L1I / L1D / L2 and report events.

use cpu_sim::sim::memsys::system::MemPanelSystem;
use cpu_sim::sim::memsys::{BusReq, Event, Mesi, Outcome, Port};
use cpu_sim::sim::system::{addr, PrivMode};

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 { ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13 }
fn lui(rd: u32, imm20: u32) -> u32 { (imm20 << 12) | (rd << 7) | 0x37 }
fn lw(rd: u32, rs1: u32, off: i32) -> u32 { ((off as u32 & 0xfff) << 20) | (rs1 << 15) | (2 << 12) | (rd << 7) | 0x03 }
fn sw(rs2: u32, rs1: u32, off: i32) -> u32 {
    let o = off as u32 & 0xfff;
    ((o >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (2 << 12) | ((o & 0x1f) << 7) | 0x23
}
fn branch(f3: u32, rs1: u32, rs2: u32, off: i32) -> u32 {
    let o = off as u32;
    ((o >> 12) & 1) << 31 | ((o >> 5) & 0x3f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | ((o >> 1) & 0xf) << 8 | ((o >> 11) & 1) << 7 | 0x63
}
fn bne(rs1: u32, rs2: u32, off: i32) -> u32 { branch(1, rs1, rs2, off) }
fn beq(rs1: u32, rs2: u32, off: i32) -> u32 { branch(0, rs1, rs2, off) }
fn csrr(rd: u32, csr: u32) -> u32 { (csr << 20) | (2 << 12) | (rd << 7) | 0x73 }
const SPIN: u32 = 0x0000_006f;
const NOP: u32 = 0x13;

/// Run, collecting every event.
fn run(sys: &mut MemPanelSystem, cycles: usize) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..cycles {
        sys.step();
        all.extend(sys.mem.borrow().events.iter().cloned());
    }
    all
}

/// Core 1 parks at 0x40; core 0 runs from 0x08.
fn split(core0: &[u32], core1: &[u32]) -> Vec<u32> {
    let mut p = vec![csrr(5, 0xf14), bne(5, 0, 0x40 - 4)];
    p.extend_from_slice(core0);
    p.resize(16, NOP);
    p.extend_from_slice(core1);
    p
}

#[test]
fn bare_loop_warms_the_l1i() {
    let mut sys = MemPanelSystem::new();
    sys.load(0, &split(&[addi(6, 0, 0), addi(7, 0, 20), addi(6, 6, 1), bne(6, 7, -4), SPIN], &[SPIN]));
    let ev = run(&mut sys, 600);
    assert_eq!(sys.cores[0].read_register(6), 20);
    assert!(ev.iter().any(|e| matches!(e, Event::Bare { core: 0, .. })), "M-mode: no translation");
    let s = sys.mem.borrow().stats[0];
    assert!(s.l1i_hits > 10 * s.l1i_misses, "loop runs from L1I: {} hits / {} misses", s.l1i_hits, s.l1i_misses);
    assert!(s.stall_cycles > 0, "the first fetches waited for memory");
}

#[test]
fn data_loop_hits_l1d_after_first_miss() {
    let mut sys = MemPanelSystem::new();
    sys.load(0x1000, &[1, 2, 3, 4]);
    // x10 = 0x1000; 10 × { lw x8,0(x10); lw x9,12(x10) }
    sys.load(0, &split(&[lui(10, 1), addi(6, 0, 10), lw(8, 10, 0), lw(9, 10, 12), addi(6, 6, -1), bne(6, 0, -12), SPIN], &[SPIN]));
    let ev = run(&mut sys, 800);
    assert_eq!((sys.cores[0].read_register(8), sys.cores[0].read_register(9)), (1, 4));
    let loads: Vec<Outcome> = ev.iter().filter_map(|e| match e {
        Event::Access { core: 0, port: Port::Load, outcome, .. } => Some(*outcome), _ => None }).collect();
    assert_eq!(loads[0], Outcome::Memory, "first load: total miss");
    assert!(loads[1..].iter().all(|o| *o == Outcome::L1Hit), "then L1 hits: {loads:?}");
}

/// Sv32 tables at 0x8000 (root) / 0x9000 (leaf): VA page 0 → PA 0 (code,
/// RX), VA 0x0040_1000 → PA 0x2000 (data, RW; TLB entry 1, so it does not
/// conflict with the code page's entry 0). VA 0x0080_0000 unmapped.
fn page_tables(sys: &mut MemPanelSystem) {
    const V: u32 = 1; const R: u32 = 2; const W: u32 = 4; const X: u32 = 8; const A: u32 = 64; const D: u32 = 128;
    let leaf = 0x9000u32;
    sys.load(0x8000, &[((leaf >> 12) << 10) | V]);                    // root[0] → leaf table (VA 0..4 MiB)
    sys.load(0x8000 + 4, &[((0xa000u32 >> 12) << 10) | V]);           // root[1] → table at 0xa000 (VA 4..8 MiB)
    sys.load(leaf, &[(0 << 10) | V | R | X | A]);                      // VA page 0 → PA 0
    sys.load(0xa004, &[((0x2000u32 >> 12) << 10) | V | R | W | A | D]); // VA 0x40_1000 → PA 0x2000
    for c in sys.cores.iter_mut() {
        c.sys.priv_mode = PrivMode::S;
        c.sys.csrs.write(addr::SATP, (1 << 31) | (0x8000 >> 12));
        c.sys.csrs.write(addr::MTVEC, 0x80);
    }
}

#[test]
fn paging_walks_once_then_hits_the_tlb() {
    let mut sys = MemPanelSystem::new();
    page_tables(&mut sys);
    sys.load(0x2000, &[0x55]);
    sys.load(0, &split(&[lui(10, 0x401), lw(8, 10, 0), lw(9, 10, 0), SPIN], &[SPIN]));
    let ev = run(&mut sys, 600);
    assert_eq!(sys.cores[0].read_register(8), 0x55);
    assert_eq!(sys.cores[0].read_register(9), 0x55);
    // data page: miss → 2 walk reads → fill, then a hit
    let data: Vec<&Event> = ev.iter().filter(|e| match e {
        Event::TlbMiss { core: 0, va } | Event::TlbHit { core: 0, va, .. } | Event::TlbFill { core: 0, va, .. } => *va == 0x40_1000,
        Event::Walk { core: 0, pte_addr, .. } => *pte_addr == 0x8004 || *pte_addr == 0xa004,
        _ => false }).collect();
    assert!(matches!(data[0], Event::TlbMiss { .. }), "{data:?}");
    assert!(matches!(data[1], Event::Walk { level: 1, pte_addr: 0x8004, .. }));
    assert!(matches!(data[2], Event::Walk { level: 0, pte_addr: 0xa004, .. }));
    assert!(matches!(data[3], Event::TlbFill { idx: 1, pa: 0x2000, .. }));
    assert!(matches!(data[4], Event::TlbHit { pa: 0x2000, .. }));
    // the page-table reads went through the data cache
    assert!(ev.iter().any(|e| matches!(e, Event::Access { core: 0, port: Port::Walk, .. })));
    let s = sys.mem.borrow().stats[0];
    assert!(s.tlb_hits > s.tlb_misses, "code fetches hit the TLB after the first walk");
}

#[test]
fn unmapped_page_faults_into_the_handler() {
    let mut sys = MemPanelSystem::new();
    page_tables(&mut sys);
    sys.load(0, &split(&[lui(10, 0x800), lw(8, 10, 0), SPIN], &[SPIN]));
    sys.load(0x80, &[addi(20, 0, 7), SPIN]);                           // handler (M-mode, bare)
    let ev = run(&mut sys, 800);
    assert!(ev.iter().any(|e| matches!(e, Event::PageFault { core: 0, va: 0x80_0000, cause: 13 })));
    assert_eq!(sys.cores[0].sys.csrs.read(addr::MCAUSE), 13);
    assert_eq!(sys.cores[0].read_register(20), 7, "handler ran");
}

#[test]
fn flag_passing_between_cores_is_coherent() {
    // core 0: data 42 → 0x1000, flag 1 → 0x1010. core 1: spin on flag, read data.
    let core0 = [lui(10, 1), addi(6, 0, 42), sw(6, 10, 0), addi(7, 0, 1), sw(7, 10, 16), SPIN];
    let core1 = [lui(10, 1), lw(7, 10, 16), beq(7, 0, -4), lw(8, 10, 0), SPIN];
    let mut sys = MemPanelSystem::new();
    sys.load(0, &split(&core0, &core1));
    let ev = run(&mut sys, 1500);
    assert_eq!(sys.cores[1].read_register(8), 42, "core 1 sees core 0's data");
    assert!(ev.iter().any(|e| matches!(e, Event::Snoop { from: 1, to: 0, req: BusReq::BusRd, before: Mesi::M, flush: true, .. })),
        "core 1's read pulled core 0's Modified line");
    assert!(ev.iter().any(|e| matches!(e, Event::BusWait { .. })) || ev.iter().any(|e| matches!(e, Event::Stall { core: 1, .. })));
}

#[test]
fn direct_mapped_tlb_conflict_between_code_and_data_pages() {
    // VA 0x0040_0000 (page 0x400) and the code page 0 share TLB entry 0:
    // each fetch evicts the data translation and the next load walks again.
    let mut sys = MemPanelSystem::new();
    page_tables(&mut sys);
    sys.load(0xa000, &[((0x3000u32 >> 12) << 10) | 0xc7]);             // VA 0x40_0000 → PA 0x3000 (VRW AD)
    sys.load(0, &split(&[lui(10, 0x400), lw(8, 10, 0), lw(9, 10, 0), SPIN], &[SPIN]));
    let ev = run(&mut sys, 600);
    let fills = ev.iter().filter(|e| matches!(e, Event::TlbFill { core: 0, va: 0x40_0000, idx: 0, .. })).count();
    assert!(fills >= 2, "data page refilled entry 0 after the code page took it ({fills})");
}
