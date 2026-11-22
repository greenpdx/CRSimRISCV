//! Every core's memory through the shared `Bus` view: `poke` then `peek`
//! lands in the right region, and agrees with the core's own accessors.

use cpu_sim::sim::barrel::{BarrelState, EXT_DMEM_BASE, ISRAM_BASE, ISRAM_BYTES_PER_SLOT};
use cpu_sim::sim::cpu_state::CpuState;
use cpu_sim::sim::mem::{Bus, Port};
use cpu_sim::sim::memsys::system::MemPanelSystem;
use cpu_sim::sim::ooo::mem::{MemMode, OooMem};
use cpu_sim::sim::pipeline::PipelineState;

fn round_trip(m: &mut impl Bus, addr: u32, v: u32) {
    m.poke(addr, v);
    assert_eq!(m.peek(addr), v, "{addr:#x}");
}

#[test]
fn single_cycle() {
    let mut c = CpuState::new();
    for (a, v) in [(0x4, 0x11), (0x1_0008, 0x22), (0x1_0000 + 4 * 300, 0x33), (c.mem.io_base + 8, 0x44)] {
        round_trip(&mut c.mem, a, v);
    }
    assert_eq!(c.mem.rom[1], 0x11);
    assert_eq!(c.mem.ram[2], 0x22);
    assert_eq!(c.mem.ram_extra[&(0x1_0000 + 4 * 300)], 0x33);
    assert_eq!(c.mem.io_data, 0x44);
    assert_eq!(c.mem.read(0x4, Port::Fetch), 0x11);
    c.mem.write_lanes(0x1_0008, 0xab00, 0b0010, Port::Store);
    assert_eq!(c.mem.ram[2], 0xab22);
}

#[test]
fn pipeline_flat_and_hierarchy() {
    let mut p = PipelineState::new();
    for (a, v) in [(0x8, 0x11), (0x1_0004, 0x22), (0x4002_0000, 0x33)] { round_trip(&mut p.mem, a, v); }
    assert_eq!(p.rom_read(0x8), 0x11);
    assert_eq!(p.ram_read(0x1_0004), 0x22);
    assert_eq!(p.mem.io_data(), 0x33);

    let mut m = MemPanelSystem::new();
    round_trip(&mut m.cores[0].mem, 0x2000, 0x55);
    assert_eq!(m.peek(0x2000), 0x55);
    assert_eq!(m.cores[1].mem.peek(0x2000), 0x55, "one physical memory");
}

#[test]
fn barrel() {
    let mut b = BarrelState::new();
    let slot1 = ISRAM_BASE + ISRAM_BYTES_PER_SLOT + 8;
    for (a, v) in [(0x10, 0x11), (EXT_DMEM_BASE + 4, 0x22), (slot1, 0x33)] { round_trip(&mut b.mem, a, v); }
    assert_eq!(b.imem_read(0x10), 0x11);
    assert_eq!(b.dmem_read(EXT_DMEM_BASE + 4), 0x22);
    assert_eq!(b.isram_read(1, 8), 0x33);
}

#[test]
fn out_of_order() {
    for mode in [MemMode::Fixed, MemMode::Caches] {
        let mut m = OooMem::new(mode);
        round_trip(&mut m, 0x1000, 0x77);
        assert_eq!(m.read(0x1000, Port::Load), 0x77, "{mode:?}");
        m.write(0x1004, 0x88, Port::Store);
        assert_eq!(m.peek(0x1004), 0x88, "{mode:?}");
    }
}
