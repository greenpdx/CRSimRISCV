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
        Event::TlbMiss { core: 0, va, .. } | Event::TlbHit { core: 0, va, .. } | Event::TlbFill { core: 0, va, .. } => *va == 0x40_1000,
        Event::Walk { core: 0, pte_addr, .. } => *pte_addr == 0x8004 || *pte_addr == 0xa004,
        _ => false }).collect();
    assert!(matches!(data[0], Event::TlbMiss { .. }), "{data:?}");
    assert!(matches!(data[0], Event::TlbMiss { port: Port::Load, .. }), "tagged as the data side");
    assert!(matches!(data[1], Event::Walk { level: 1, pte_addr: 0x8004, port: Port::Load, .. }));
    assert!(matches!(data[2], Event::Walk { level: 0, pte_addr: 0xa004, .. }));
    assert!(matches!(data[3], Event::TlbFill { idx: 0, pa: 0x2000, .. }), "first free DTLB entry");
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
    assert!(ev.iter().any(|e| matches!(e, Event::PageFault { core: 0, port: Port::Load, va: 0x80_0000, cause: 13 })));
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
fn split_itlb_dtlb_code_and_data_pages_do_not_conflict() {
    // VA 0x0040_0000 (page 0x400) and the code page 0 both index entry 0,
    // but code goes in the ITLB and data in the DTLB: one walk each.
    let mut sys = MemPanelSystem::new();
    page_tables(&mut sys);
    sys.load(0xa000, &[((0x3000u32 >> 12) << 10) | 0xc7]);             // VA 0x40_0000 → PA 0x3000 (VRW AD)
    sys.load(0, &split(&[lui(10, 0x400), lw(8, 10, 0), lw(9, 10, 0), SPIN], &[SPIN]));
    let ev = run(&mut sys, 600);
    let fills = ev.iter().filter(|e| matches!(e, Event::TlbFill { core: 0, va: 0x40_0000, idx: 0, port: Port::Load, .. })).count();
    assert_eq!(fills, 1, "data page walked once");
    assert!(ev.iter().any(|e| matches!(e, Event::TlbHit { core: 0, va: 0x40_0000, port: Port::Load, .. })));
    let c = &sys.cores[0];
    assert!(c.itlb.entries[0].valid && c.itlb.entries[0].vpn == 0, "code page in ITLB[0]");
    assert!(c.dtlb.entries[0].valid && c.dtlb.entries[0].vpn == 0x400, "data page in DTLB[0]");
}

// ── The panel's examples (memsys::examples) each show what they promise ──

use cpu_sim::sim::memsys::examples::examples;
use cpu_sim::sim::memsys::CacheId;

fn run_example(i: usize, cycles: usize) -> (MemPanelSystem, Vec<Event>) {
    let mut sys = MemPanelSystem::new();
    (examples()[i].setup)(&mut sys);
    let ev = run(&mut sys, cycles);
    (sys, ev)
}
fn data_outcomes(ev: &[Event]) -> Vec<Outcome> {
    ev.iter().filter_map(|e| match e { Event::Access { core: 0, port: Port::Load, outcome, .. } => Some(*outcome), _ => None }).collect()
}

#[test]
fn example_l1_hits() {
    let (sys, ev) = run_example(1, 1500);
    assert_eq!(sys.cores[0].read_register(11), 8 * (1 + 2 + 3 + 4));
    let o = data_outcomes(&ev);
    assert_eq!(o[0], Outcome::Memory);
    assert!(o[1..].iter().all(|x| *x == Outcome::L1Hit), "{o:?}");
}

#[test]
fn example_l1_miss_l2_hit() {
    let (_, ev) = run_example(2, 3000);
    let o = data_outcomes(&ev);
    assert_eq!(o.len(), 18);
    assert!(o[..3].iter().all(|x| *x == Outcome::Memory));
    assert!(o[3..].iter().all(|x| *x == Outcome::L2Hit), "{o:?}");
}

#[test]
fn example_total_miss() {
    let (_, ev) = run_example(3, 4000);
    let o = data_outcomes(&ev);
    assert_eq!(o.len(), 20);
    assert!(o.iter().all(|x| *x == Outcome::Memory), "{o:?}");
    assert!(ev.iter().any(|e| matches!(e, Event::Evict { cache: CacheId::L2, .. })));
}

#[test]
fn example_mmu_walk_then_hits() {
    let (sys, ev) = run_example(4, 2000);
    assert_eq!((sys.cores[0].read_register(7), sys.cores[0].read_register(8)), (0x55, 0x66));
    let walks = ev.iter().filter(|e| matches!(e, Event::Walk { core: 0, .. })).count();
    assert!(walks >= 4, "code page + data page walks ({walks})");
    assert!(ev.iter().filter(|e| matches!(e, Event::TlbHit { core: 0, va: 0x40_1000, .. })).count() >= 3);
}

#[test]
fn example_page_fault_handler_maps_page() {
    let (sys, ev) = run_example(5, 2000);
    assert!(ev.iter().any(|e| matches!(e, Event::PageFault { core: 0, port: Port::Load, va: 0x80_0000, cause: 13 })));
    assert!(ev.iter().any(|e| matches!(e, Event::TlbFill { core: 0, va: 0x80_0000, pa: 0x3000, .. })), "retry walked");
    assert_eq!(sys.cores[0].read_register(7), 0x77);
}

#[test]
fn example_flag_passing() {
    let (sys, ev) = run_example(9, 2000);
    assert_eq!(sys.cores[1].read_register(8), 42);
    assert!(ev.iter().any(|e| matches!(e, Event::Snoop { from: 1, to: 0, flush: true, .. })));
}

#[test]
fn example_false_sharing() {
    let (sys, ev) = run_example(10, 4000);
    assert_eq!((sys.peek(0x1000), sys.peek(0x1004)), (8, 8), "both counters correct");
    let bounces = ev.iter().filter(|e| matches!(e, Event::Snoop { cache: CacheId::L1D, after: Mesi::I, .. })).count();
    assert!(bounces >= 8, "the line ping-pongs ({bounces} invalidations)");
}

// ── TLB: phases, ASIDs, SFENCE.VMA, PLRU ──

use cpu_sim::sim::memsys::PteVerdict;

/// Core 0's DTLB lookups as 'H' (hit) / 'M' (miss), in order.
fn load_lookups(ev: &[Event]) -> String {
    ev.iter().filter_map(|e| match e {
        Event::TlbLookup { core: 0, port: Port::Load, hit, .. } => Some(if hit.is_some() { 'H' } else { 'M' }),
        _ => None }).collect()
}

#[test]
fn tlb_miss_reports_every_walk_phase_in_order() {
    let (_, ev) = run_example(4, 2000);
    let i = ev.iter().position(|e| matches!(e, Event::TlbLookup { core: 0, port: Port::Load, hit: None, .. })).unwrap();
    let phases: Vec<&Event> = ev[i..].iter().filter(|e| matches!(e,
        Event::TlbLookup { core: 0, .. } | Event::TlbMiss { core: 0, .. } | Event::PteAddr { core: 0, .. } | Event::Walk { core: 0, .. }
        | Event::PteCheck { core: 0, .. } | Event::PermCheck { core: 0, .. } | Event::TlbVictim { core: 0, .. } | Event::TlbFill { core: 0, .. }))
        .take(11).collect();
    assert!(matches!(phases[1], Event::TlbMiss { .. }));
    assert!(matches!(phases[2], Event::PteAddr { level: 1, base: 0x8000, vpn: 1, addr: 0x8004, .. }), "{:?}", phases[2]);
    assert!(matches!(phases[3], Event::Walk { level: 1, .. }));
    assert!(matches!(phases[4], Event::PteCheck { level: 1, verdict: PteVerdict::Pointer, .. }));
    assert!(matches!(phases[5], Event::PteAddr { level: 0, base: 0xa000, vpn: 1, addr: 0xa004, .. }));
    assert!(matches!(phases[6], Event::Walk { level: 0, .. }));
    assert!(matches!(phases[7], Event::PteCheck { level: 0, verdict: PteVerdict::Leaf, .. }));
    assert!(matches!(phases[8], Event::PermCheck { ok: true, .. }));
    assert!(matches!(phases[9], Event::TlbVictim { idx: 0, was_invalid: true, .. }));
    assert!(matches!(phases[10], Event::TlbFill { idx: 0, pa: 0x2000, .. }));
}

#[test]
fn example_asids_and_global_page() {
    let (sys, ev) = run_example(6, 2000);
    let c = &sys.cores[0];
    assert_eq!((c.read_register(7), c.read_register(8), c.read_register(9)), (0x11, 0x22, 0x11));
    assert_eq!(load_lookups(&ev), "MMH", "ASID 2 misses, ASID 1's entry survives the satp switch");
    let code_fills = ev.iter().filter(|e| matches!(e, Event::TlbFill { core: 0, port: Port::Fetch, .. })).count();
    assert_eq!(code_fills, 1, "global code page: one ITLB walk for both ASIDs");
    let asid2 = ev.iter().find_map(|e| match e { Event::TlbLookup { core: 0, port: Port::Load, asid: 2, cmp, .. } => Some(cmp[0]), _ => None }).unwrap();
    assert!(asid2.valid && asid2.vpn_match && !asid2.asid_match && !asid2.global && !asid2.hit);
}

#[test]
fn example_sfence_four_cases() {
    let (_, ev) = run_example(7, 3000);
    assert_eq!(load_lookups(&ev), "MMMMMHMMM");
    let fences: Vec<_> = ev.iter().filter_map(|e| match e {
        Event::Sfence { core: 0, va, asid, itlb, dtlb } => Some((*va, *asid, itlb.map(|f| f.inval), dtlb.map(|f| f.inval))), _ => None }).collect();
    assert_eq!(fences.len(), 4);
    assert_eq!((fences[0].0, fences[0].1, fences[0].3), (Some(0x40_1000), None, [true, false, false, false]));
    assert_eq!(fences[1].1, Some(1));
    assert!(fences[1].2.contains(&true), "the ASID flush drops the (non-global) code page from the ITLB");
    assert_eq!(fences[1].3, [true, true, true, false], "global page 0x404 survives");
    assert_eq!(fences[2].3, [true, false, false, false], "page 0x402 in ASID 1 (refilled into entry 0)");
    assert_eq!(fences[3].3, [true, false, false, true], "x0, x0 flushes every valid entry, global 0x404 too");
}

#[test]
fn example_plru_replacement() {
    let (_, ev) = run_example(8, 3000);
    assert_eq!(load_lookups(&ev), "MMMMHMMM");
    let victims: Vec<(u8, bool)> = ev.iter().filter_map(|e| match e {
        Event::TlbVictim { core: 0, port: Port::Load, idx, was_invalid, .. } => Some((*idx, *was_invalid)), _ => None }).collect();
    assert_eq!(victims, [(0, true), (1, true), (2, true), (3, true), (2, false), (1, false), (3, false)]);
}

// ── Boot from ROM ──

use cpu_sim::sim::memsys::ROM_BASE;

#[test]
fn example_boot_from_rom() {
    let (sys, ev) = run_example(0, 3000);
    let c0 = &sys.cores[0];
    assert_eq!(c0.read_register(8), 15, "program ran in S-mode and read its DMEM store back");
    assert_eq!(c0.sys.priv_mode, PrivMode::S);
    assert_eq!(sys.peek(0x2000), 15, "DMEM VA 0x1001_0000 → PA 0x2000");
    // boot fetches were uncached ROM reads, by both harts
    assert!(ev.iter().any(|e| matches!(e, Event::RomRead { core: 0, .. })));
    assert!(ev.iter().any(|e| matches!(e, Event::RomRead { core: 1, .. })));
    assert!(!ev.iter().any(|e| matches!(e, Event::Fill { pa, .. } if *pa >= ROM_BASE)), "ROM is never cached");
    // first translated fetch: ITLB miss, walk; the PTE reads come from the cache hierarchy, not memory
    let i = ev.iter().position(|e| matches!(e, Event::TlbMiss { core: 0, port: Port::Fetch, va: 0x40_0000 })).expect("ITLB miss at the entry point");
    let walk_reads: Vec<Outcome> = ev[i..].iter().filter_map(|e| match e {
        Event::Access { core: 0, port: Port::Walk, outcome, .. } => Some(*outcome), _ => None }).take(2).collect();
    assert_eq!(walk_reads.len(), 2);
    assert!(walk_reads.iter().all(|o| *o != Outcome::Memory), "PTEs were just written by the boot code: {walk_reads:?}");
    assert!(ev.iter().any(|e| matches!(e, Event::TlbFill { core: 0, port: Port::Fetch, va: 0x40_0000, pa: 0x1000, .. })));
    // the store to DMEM walks and sets D
    assert!(ev.iter().any(|e| matches!(e, Event::AdUpdate { core: 0, port: Port::Store, .. })));
    // hart 1 parked in RAM
    assert!(sys.cores[1].pc >= 0x40 && sys.cores[1].pc < 0x50, "hart 1 parked at 0x40 (pc {:#x})", sys.cores[1].pc);
}

#[test]
fn cache_lookup_reports_tag_compare_before_fill() {
    let (_, ev) = run_example(2, 3000);   // L1 miss, L2 hit
    let mut checked = 0;
    for w in ev.windows(2) {
        if let Event::Lookup { core: 0, cache, set, tag, ways, pa } = &w[0] {
            let hits: Vec<usize> = ways.iter().enumerate().filter(|(_, x)| x.hit).map(|(i, _)| i).collect();
            assert!(hits.len() <= 1, "at most one way matches");
            for x in ways { assert_eq!(x.hit, x.valid && x.tag == *tag); }
            match (&w[1], *cache) {
                (Event::L1Hit { way, set: s, .. }, CacheId::L1I | CacheId::L1D) => { assert_eq!(hits, [*way as usize]); assert_eq!(s, set); }
                (Event::L1Miss { set: s, .. }, CacheId::L1I | CacheId::L1D) => { assert!(hits.is_empty()); assert_eq!(s, set); }
                (Event::L2Hit { way, .. }, CacheId::L2) => assert_eq!(hits, [*way as usize]),
                (Event::L2Miss { .. }, CacheId::L2) => assert!(hits.is_empty()),
                (e, c) => panic!("lookup {c:?} {pa:#x} followed by {e:?}"),
            }
            checked += 1;
        }
    }
    assert!(checked > 20, "{checked}");
}
