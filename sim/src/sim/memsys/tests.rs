use super::*;

fn has(m: &MemSystem, f: impl Fn(&Event) -> bool) -> bool { m.events.iter().any(f) }
fn access(m: &MemSystem) -> Option<(Outcome, u32)> {
    m.events.iter().rev().find_map(|e| match e { Event::Access { outcome, latency, .. } => Some((*outcome, *latency)), _ => None })
}
/// Run one access for `core` as its own cycle (begin, access, end).
/// (`now` jumps ahead, so any earlier freeze has run out.)
fn cyc(m: &mut MemSystem, now: u64, f: impl FnOnce(&mut MemSystem)) {
    m.stall = [0; N_CORES];
    m.bus_free = m.bus_free.min(now);
    m.begin_cycle(now);
    assert!(!m.core_begin(0) && !m.core_begin(1));
    f(m);
    m.core_end(0);
    m.core_end(1);
}

#[test]
fn cold_read_goes_to_memory_then_hits_l1() {
    let mut m = MemSystem::new();
    m.load(0x100, &[11, 22, 33, 44]);
    cyc(&mut m, 0, |m| assert_eq!(m.read(0, 0x104, Port::Load), 22));
    assert!(has(&m, |e| matches!(e, Event::L1Miss { cache: CacheId::L1D, .. })));
    assert!(has(&m, |e| matches!(e, Event::L2Miss { .. })));
    assert!(has(&m, |e| matches!(e, Event::MemRead { pa: 0x100, .. })));
    assert!(has(&m, |e| matches!(e, Event::Fill { cache: CacheId::L1D, state: Mesi::E, .. })));
    assert_eq!(access(&m), Some((Outcome::Memory, 25)));
    assert_eq!(m.stall[0], 24, "frozen for the rest of the miss");
    cyc(&mut m, 100, |m| assert_eq!(m.read(0, 0x10c, Port::Load), 44));
    assert!(has(&m, |e| matches!(e, Event::L1Hit { cache: CacheId::L1D, .. })));
    assert_eq!(access(&m), Some((Outcome::L1Hit, 1)));
    assert_eq!(m.stall[0], 0);
}

#[test]
fn l1_conflict_miss_hits_l2() {
    // L1D: 4 sets × 2 ways, 16 B lines → 0x000, 0x040, 0x080 share set 0
    let mut m = MemSystem::new();
    for (i, a) in [0x000u32, 0x040, 0x080].iter().enumerate() {
        cyc(&mut m, i as u64 * 100, |m| { m.read(0, *a, Port::Load); });
    }
    assert!(has(&m, |e| matches!(e, Event::Evict { cache: CacheId::L1D, pa: 0x000, .. })), "LRU line 0x000 left L1");
    cyc(&mut m, 1000, |m| { m.read(0, 0x000, Port::Load); });
    assert!(has(&m, |e| matches!(e, Event::L2Hit { pa: 0x000, .. })));
    assert_eq!(access(&m), Some((Outcome::L2Hit, 5)));
}

#[test]
fn dirty_lines_write_back_to_l2_then_memory() {
    let mut m = MemSystem::new();
    cyc(&mut m, 0, |m| m.write(0, 0x000, 0xaa, Port::Store));
    assert!(has(&m, |e| matches!(e, Event::Fill { cache: CacheId::L1D, state: Mesi::M, .. })), "write miss fills M");
    // two more lines in L1D set 0 → the M line is evicted and written back to L2
    for (i, a) in [0x040u32, 0x080].iter().enumerate() { cyc(&mut m, 100 + i as u64 * 100, |m| { m.read(0, *a, Port::Load); }); }
    assert!(has(&m, |e| matches!(e, Event::WriteBack { pa: 0x000, .. })));
    assert_eq!(m.mem[0], 0, "memory not yet updated");
    assert_eq!(m.peek(0x000), 0xaa);
    // L2 set 0 (8 sets → stride 0x80): 0x000, 0x080 there already; 0x100, 0x180, 0x200 evict 0x000
    for (i, a) in [0x100u32, 0x180, 0x200].iter().enumerate() { cyc(&mut m, 500 + i as u64 * 100, |m| { m.read(0, *a, Port::Load); }); }
    assert_eq!(m.mem[0], 0xaa, "dirty L2 victim reached memory");
}

#[test]
fn write_hit_exclusive_upgrades_silently() {
    let mut m = MemSystem::new();
    cyc(&mut m, 0, |m| { m.read(0, 0x20, Port::Load); });
    cyc(&mut m, 100, |m| m.write(0, 0x20, 5, Port::Store));
    assert!(has(&m, |e| matches!(e, Event::StateChange { from: Mesi::E, to: Mesi::M, .. })));
    assert!(!has(&m, |e| matches!(e, Event::Snoop { .. })), "no bus traffic for E → M");
    assert_eq!(access(&m), Some((Outcome::L1Hit, 1)));
}

#[test]
fn mesi_share_upgrade_and_flush() {
    let mut m = MemSystem::new();
    m.load(0x40, &[7]);
    cyc(&mut m, 0, |m| { m.read(0, 0x40, Port::Load); });                // core 0: E
    cyc(&mut m, 100, |m| { m.read(1, 0x40, Port::Load); });              // core 1: BusRd → both S
    assert!(has(&m, |e| matches!(e, Event::Snoop { from: 1, to: 0, req: BusReq::BusRd, before: Mesi::E, after: Mesi::S, .. })));
    assert!(has(&m, |e| matches!(e, Event::Fill { core: 1, cache: CacheId::L1D, state: Mesi::S, .. })));
    cyc(&mut m, 200, |m| m.write(1, 0x40, 9, Port::Store));             // core 1: BusUpgr → core 0 I
    assert!(has(&m, |e| matches!(e, Event::Snoop { from: 1, to: 0, req: BusReq::BusUpgr, before: Mesi::S, after: Mesi::I, .. })));
    assert!(has(&m, |e| matches!(e, Event::StateChange { core: 1, from: Mesi::S, to: Mesi::M, .. })));
    cyc(&mut m, 300, |m| assert_eq!(m.read(0, 0x40, Port::Load), 9, "core 0 sees core 1's write"));
    assert!(has(&m, |e| matches!(e, Event::Snoop { from: 0, to: 1, req: BusReq::BusRd, before: Mesi::M, after: Mesi::S, flush: true, .. })));
    assert_eq!(access(&m).unwrap().0, Outcome::L2Hit, "served from L2 after the flush");
}

#[test]
fn write_miss_invalidates_other_l1d_and_l1i() {
    let mut m = MemSystem::new();
    cyc(&mut m, 0, |m| { m.read(1, 0x80, Port::Load); m.read(1, 0x84, Port::Fetch); });
    cyc(&mut m, 100, |m| m.write(0, 0x88, 1, Port::Store));
    assert!(has(&m, |e| matches!(e, Event::Snoop { to: 1, req: BusReq::BusRdX, cache: CacheId::L1D, after: Mesi::I, .. })));
    assert!(has(&m, |e| matches!(e, Event::Snoop { to: 1, req: BusReq::BusRdX, cache: CacheId::L1I, after: Mesi::I, .. })));
}

#[test]
fn fetch_after_store_sees_the_new_word() {
    let mut m = MemSystem::new();
    cyc(&mut m, 0, |m| m.write(0, 0x10, 0x1234, Port::Store));
    cyc(&mut m, 100, |m| assert_eq!(m.read(1, 0x10, Port::Fetch), 0x1234));
    cyc(&mut m, 200, |m| assert_eq!(m.read(0, 0x10, Port::Fetch), 0x1234));
}

#[test]
fn l2_eviction_back_invalidates_l1() {
    let mut m = MemSystem::new();
    cyc(&mut m, 0, |m| { m.read(0, 0x000, Port::Load); });
    for (i, a) in [0x080u32, 0x100, 0x180, 0x200].iter().enumerate() { cyc(&mut m, 100 + i as u64 * 100, |m| { m.read(1, *a, Port::Load); }); }
    assert!(has(&m, |e| matches!(e, Event::Evict { cache: CacheId::L2, pa: 0x000, .. })));
    assert!(has(&m, |e| matches!(e, Event::Evict { core: 0, cache: CacheId::L1D, pa: 0x000, .. })), "inclusion");
    assert!(m.l1d[0].find(0x000).is_none());
}

#[test]
fn second_core_waits_for_the_bus() {
    let mut m = MemSystem::new();
    cyc(&mut m, 0, |m| { m.read(0, 0x000, Port::Load); m.read(1, 0x400, Port::Load); });
    assert!(has(&m, |e| matches!(e, Event::BusWait { core: 1, cycles: 25 })));
    assert_eq!(m.stall, [24, 49]);
}

#[test]
fn walk_then_data_are_one_chain() {
    let mut m = MemSystem::new();
    cyc(&mut m, 0, |m| { m.read(0, 0x000, Port::Load); m.read(0, 0x004, Port::Load); });
    m.events.clear();
    cyc(&mut m, 100, |m| { m.read(0, 0x000, Port::Walk); m.read(0, 0x004, Port::Walk); m.read(0, 0x008, Port::Load); });
    assert_eq!(m.stall[0], 2, "three dependent L1 hits = 3 cycles");
}
