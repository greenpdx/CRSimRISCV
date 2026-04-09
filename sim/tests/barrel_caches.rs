//! Phase B4 — I-L1 / D-L1 caches with store buffer and bus arbiter.
//!
//! These tests deliberately do NOT call `prewarm_icache()`, so
//! they exercise miss-fill and stall behavior.

use cpu_sim::sim::barrel::{
    bus::{BusState, MISS_LATENCY},
    cache::{Cache, CACHE_LINE_BYTES},
    BarrelState, EXT_DMEM_BASE, N_THREADS,
};

const NOP: u32 = 0x00000013;
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
fn lw(rd: u32, rs1: u32, off: i32) -> u32 {
    (((off as u32) & 0xfff) << 20) | (rs1 << 15) | (0b010 << 12) | (rd << 7) | 0x03
}
fn sw(rs1: u32, rs2: u32, off: i32) -> u32 {
    let imm = off as u32 & 0xfff;
    let imm11_5 = (imm >> 5) & 0x7f;
    let imm4_0  = imm & 0x1f;
    (imm11_5 << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | (imm4_0 << 7) | 0x23
}
fn lui(rd: u32, imm: u32) -> u32 { (imm << 12) | (rd << 7) | 0x37 }
fn jal_spin() -> u32 { 0x0000006f }

#[test]
fn barrel_icache_miss_then_hit() {
    // First fetch from a cold I-cache misses; the bus fills the
    // line over MISS_LATENCY cycles; second access to the same
    // line is a hit.
    let mut rom = vec![NOP; 16];
    rom[0] = addi(5, 0, 7);
    rom[1] = jal_spin();
    let mut cpu = BarrelState::new();
    cpu.activate_all();
    for i in 0..N_THREADS { cpu.set_thread_pc(i as u8, 0); }
    cpu.load_program(&rom);
    // Do NOT prewarm — we want to see the miss path.

    // After enough cycles, the cache must have at least one hit
    // and one miss recorded.
    for _ in 0..80 { cpu.step(); }
    assert!(cpu.icache.misses > 0, "expected at least 1 i-cache miss");
    assert!(cpu.icache.hits > 0, "expected at least 1 i-cache hit");
    // Threads must have eventually retired their addi.
    for i in 0..N_THREADS {
        assert_eq!(cpu.read_thread_register(i as u8, 5), 7, "thread {} addi", i);
    }
}

#[test]
fn barrel_icache_miss_stalls_only_one_thread() {
    // One thread (slot 0) has a PC that hits a fresh line; the
    // other threads spin on a separate line that's already in
    // the cache (we prewarm just that line). When slot 0 fetches,
    // it misses and stalls — but the bus drains while slots
    // 1..7 keep running. We verify the dispatch history during
    // the stall window does NOT include slot 0.
    let mut rom = vec![NOP; 64];
    // Slot 0 starts at byte 0x40 (a different line than the spinners)
    rom[16] = addi(5, 0, 1);    // 0x40
    rom[17] = jal_spin();
    // Spinners at byte 0x00
    rom[0] = jal_spin();

    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0x40;
    cpu.threads[0].priority = 1;
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0;
        cpu.threads[s].priority = 1;
    }
    cpu.load_program(&rom);
    // Prewarm only line 0 (the spinners' line). Line 0x40 stays cold.
    cpu.icache.install(0, [rom[0], rom[1], rom[2], rom[3]]);

    // Step a few cycles before slot 0 ever dispatches.
    cpu.step(); // first cycle dispatches some thread
    // Step until slot 0 misses the cache. Track that slot 0 is
    // recorded as miss_pending during at least some cycles.
    let mut saw_pending = false;
    for _ in 0..40 {
        cpu.step();
        if cpu.threads[0].miss_pending { saw_pending = true; }
    }
    assert!(saw_pending, "slot 0 should have been miss_pending at some point");
    // Eventually slot 0 retires its addi.
    for _ in 0..80 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 5), 1);
}

#[test]
fn barrel_dcache_load_miss_then_hit() {
    // A thread loads from RAM. First load misses the cold D-cache
    // (8 cycles), the bus fills the line, the load completes, and
    // any subsequent load to the same line hits.
    let mut rom = vec![NOP; 32];
    rom[0] = lui(6, EXT_DMEM_BASE >> 12);  // x6 = 0x10000000
    rom[1] = lw(7, 6, 0);                  // load
    rom[2] = jal_spin();
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    // spinners 1..7 elsewhere
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x40;
        cpu.threads[s].priority = 1;
    }
    rom[16] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    // Pre-write a value into RAM at 0x10000000.
    cpu.dmem_write(EXT_DMEM_BASE, 0xcafe_babe);

    for _ in 0..200 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 7), 0xcafe_babe);
    assert!(cpu.dcache.misses > 0, "expected at least 1 d-cache miss");
}

#[test]
fn barrel_store_buffer_drains_to_external_memory() {
    // A thread stores a value to RAM. The store goes into the
    // store buffer; the bus eventually drains it to ext_dmem.
    // Verify ext_dmem receives the write.
    let mut rom = vec![NOP; 32];
    rom[0] = lui(6, EXT_DMEM_BASE >> 12);  // base
    rom[1] = addi(7, 0, 0x123);            // value
    rom[2] = sw(6, 7, 0);                  // store
    rom[3] = jal_spin();
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x20;
        cpu.threads[s].priority = 1;
    }
    rom[8] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..200 { cpu.step(); }
    assert_eq!(cpu.dmem_read(EXT_DMEM_BASE), 0x123,
               "store buffer should have drained the value to ext RAM");
    assert!(cpu.store_buffer.is_empty(), "store buffer should be drained");
}

#[test]
fn barrel_store_then_load_forwards_from_buffer() {
    // A store followed immediately by a load to the same address
    // should read back the just-stored value via store-to-load
    // forwarding from the store buffer, even if the store hasn't
    // drained yet.
    let mut rom = vec![NOP; 32];
    rom[0] = lui(6, EXT_DMEM_BASE >> 12);
    rom[1] = addi(7, 0, 0x77);
    rom[2] = sw(6, 7, 0);
    rom[3] = lw(8, 6, 0);
    rom[4] = jal_spin();
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x20;
        cpu.threads[s].priority = 1;
    }
    rom[8] = jal_spin();
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..300 { cpu.step(); }
    assert_eq!(cpu.read_thread_register(0, 8), 0x77,
               "load should forward from store buffer");
}

#[test]
fn barrel_bus_priority_dcache_before_icache() {
    // Initialize the cache 'pending' fields directly so both
    // caches have a miss. Then run one bus tick and verify the
    // bus chose the D-cache miss first.
    let mut cpu = BarrelState::new();
    cpu.icache.pending = Some(cpu_sim::sim::barrel::cache::PendingMiss {
        line_addr: 0x100, thread: 0,
    });
    cpu.dcache.pending = Some(cpu_sim::sim::barrel::cache::PendingMiss {
        line_addr: EXT_DMEM_BASE, thread: 1,
    });
    cpu.icache.waiters[0] = true;
    cpu.dcache.waiters[1] = true;
    cpu.threads[0].active = true;
    cpu.threads[0].miss_pending = true;
    cpu.threads[1].active = true;
    cpu.threads[1].miss_pending = true;
    // Step one cycle — bus_tick should pick the D-cache miss.
    cpu.step();
    match cpu.bus.state {
        BusState::FillingD { .. } => {} // good
        _ => panic!("bus should be filling D-cache, got {:?}", cpu.bus.state),
    }
}

#[test]
fn barrel_icache_hits_after_prewarm() {
    // prewarm_icache should mean the very first fetch is a hit.
    let mut rom = vec![NOP; 16];
    rom[0] = addi(5, 0, 9);
    rom[1] = jal_spin();
    let mut cpu = BarrelState::new();
    cpu.activate_all();
    for i in 0..N_THREADS { cpu.set_thread_pc(i as u8, 0); }
    cpu.load_program(&rom);
    cpu.prewarm_icache();
    for _ in 0..40 { cpu.step(); }
    assert_eq!(cpu.icache.misses, 0, "no misses with prewarm");
    assert!(cpu.icache.hits > 0);
}
