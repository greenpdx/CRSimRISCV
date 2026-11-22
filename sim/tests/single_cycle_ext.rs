//! Single-cycle core: byte / halfword loads and stores, Zmmul (MUL block,
//! on / off), misaligned accesses (trap with traps on, low bits ignored off).

use cpu_sim::sim::core::Core;
use cpu_sim::sim::cpu_state::CpuState;
use cpu_sim::isa::csr::addr as csra;
use cpu_sim::sim::system::EXC_ILLEGAL_INSTR;

const RAM: u32 = 0x0001_0000;

fn i_type(op: u32, f3: u32, rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | op
}
fn s_type(f3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let i = imm as u32 & 0xfff;
    ((i >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (f3 << 12) | ((i & 0x1f) << 7) | 0x23
}
fn r_type(f7: u32, f3: u32, rd: u32, rs1: u32, rs2: u32) -> u32 {
    (f7 << 25) | (rs2 << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | 0x33
}
fn load(f3: u32, rd: u32, rs1: u32, imm: i32) -> u32 { i_type(0x03, f3, rd, rs1, imm) }
fn mul(f3: u32, rd: u32, rs1: u32, rs2: u32) -> u32 { r_type(1, f3, rd, rs1, rs2) }

fn step(cpu: &mut CpuState) { for _ in 0..10 { cpu.micro_step(); } }

/// Panel-like core (traps off) with x1 = RAM base and the program loaded
fn core(prog: &[u32]) -> CpuState {
    let mut c = CpuState::with_sizes(64, 64);
    c.traps_enabled = false;
    c.load_program(prog);
    c.write_register(1, RAM);
    c
}

#[test]
fn loads_pick_the_lane_and_extend() {
    // RAM[0] = 0x80FF_7F01 (bytes 01 7F FF 80)
    let cases: &[(u32, i32, u32)] = &[
        (0b000, 0, 0x0000_0001), (0b000, 1, 0x0000_007f), (0b000, 2, 0xffff_ffff), (0b000, 3, 0xffff_ff80),
        (0b100, 2, 0x0000_00ff), (0b100, 3, 0x0000_0080),
        (0b001, 0, 0x0000_7f01), (0b001, 2, 0xffff_80ff),
        (0b101, 2, 0x0000_80ff), (0b010, 0, 0x80ff_7f01),
    ];
    for &(f3, off, want) in cases {
        let mut c = core(&[load(f3, 2, 1, off)]);
        c.poke_ram(RAM, 0x80ff_7f01);
        step(&mut c);
        assert_eq!(c.registers[2], want, "funct3 {f3:03b} offset {off}");
        assert_eq!(c.micro.mem_word, Some(0x80ff_7f01));
    }
}

#[test]
fn stores_write_only_their_lanes() {
    let cases: &[(u32, i32, u32, u8)] = &[
        (0b000, 0, 0x1111_11ab, 0b0001), (0b000, 1, 0x1111_ab11, 0b0010),
        (0b000, 2, 0x11ab_1111, 0b0100), (0b000, 3, 0xab11_1111, 0b1000),
        (0b001, 0, 0x1111_cdab, 0b0011), (0b001, 2, 0xcdab_1111, 0b1100),
        (0b010, 0, 0x1234_cdab, 0b1111),
    ];
    for &(f3, off, want, bw) in cases {
        let mut c = core(&[s_type(f3, 1, 3, off)]);
        c.poke_ram(RAM, 0x1111_1111);
        c.write_register(3, 0x1234_cdab);
        step(&mut c);
        assert_eq!(c.mem.ram[0], want, "funct3 {f3:03b} offset {off}");
        assert_eq!(c.micro.mem_bw, Some(bw));
    }
}

#[test]
fn byte_store_and_load_on_the_io_register() {
    let mut c = core(&[s_type(0b000, 4, 3, 1), load(0b100, 5, 4, 1)]);
    c.write_register(4, c.mem.io_base);
    c.write_register(3, 0xa5);
    c.mem.io_data = 0x0000_0000;
    step(&mut c);
    assert_eq!(c.mem.io_data, 0x0000_a500);
    step(&mut c);
    assert_eq!(c.registers[5], 0xa5);
}

#[test]
fn mul_block_all_four_ops() {
    let (a, b) = (0xffff_fffeu32, 0x0000_0003u32);    // -2, 3
    let prog = [mul(0, 10, 2, 3), mul(1, 11, 2, 3), mul(2, 12, 2, 3), mul(3, 13, 2, 3)];
    let mut c = core(&prog);
    c.write_register(2, a);
    c.write_register(3, b);
    for _ in 0..4 { step(&mut c); }
    assert_eq!(c.registers[10], 0xffff_fffa);                         // MUL    -6
    assert_eq!(c.registers[11], 0xffff_ffff);                         // MULH   s×s hi
    assert_eq!(c.registers[12], 0xffff_ffff);                         // MULHSU s×u hi
    assert_eq!(c.registers[13], 0x0000_0002);                         // MULHU  u×u hi
    assert_eq!(c.micro.ctrl_wb_sel, Some(4));
    assert_eq!(c.micro.mul_result, Some(2));
}

#[test]
fn zmmul_off_traps_off_is_a_nop() {
    let mut c = core(&[mul(0, 10, 2, 3)]);
    c.zmmul = false;
    c.write_register(2, 6);
    c.write_register(3, 7);
    step(&mut c);
    assert_eq!(c.registers[10], 0);
    assert_eq!(c.pc, 4);
}

#[test]
fn zmmul_off_traps_on_is_illegal() {
    let w = mul(0, 10, 2, 3);
    let mut c = core(&[w]);
    c.zmmul = false;
    c.traps_enabled = true;
    c.csr_write(csra::MTVEC, 0x80);
    c.write_register(2, 6);
    c.write_register(3, 7);
    step(&mut c);
    assert_eq!(c.registers[10], 0);
    assert_eq!(c.pc, 0x80);
    let t = c.last_trap().unwrap();
    assert_eq!(t.cause, EXC_ILLEGAL_INSTR);
    assert_eq!(c.csr_read(csra::MTVAL), w);
    assert_eq!(c.csr_read(csra::MEPC), 0);
}

#[test]
fn misaligned_traps_off_ignores_low_bits() {
    let mut c = core(&[load(0b010, 2, 1, 2), load(0b001, 3, 1, 3), s_type(0b010, 1, 4, 1)]);
    c.poke_ram(RAM, 0x1234_5678);
    c.write_register(4, 0xcafe_f00d);
    for _ in 0..3 { step(&mut c); }
    assert_eq!(c.registers[2], 0x1234_5678);       // LW @+2 → word @+0
    assert_eq!(c.registers[3], 0x0000_1234);       // LH @+3 → half @+2
    assert_eq!(c.mem.ram[0], 0xcafe_f00d);             // SW @+1 → word @+0
}

#[test]
fn misaligned_traps_on_raise_cause_4_and_6() {
    for (instr, cause) in [(load(0b010, 2, 1, 2), 4u32), (load(0b001, 2, 1, 1), 4), (s_type(0b001, 1, 4, 3), 6)] {
        let mut c = core(&[instr]);
        c.traps_enabled = true;
        c.csr_write(csra::MTVEC, 0x80);
        c.poke_ram(RAM, 0x1234_5678);
        c.write_register(2, 0x55);
        c.write_register(4, 0xffff);
        step(&mut c);
        let t = c.last_trap().unwrap();
        assert_eq!(t.cause, cause);
        assert_eq!(c.pc, 0x80);
        assert_eq!(c.csr_read(csra::MTVAL), RAM + if cause == 4 { (instr >> 20) & 0xfff } else { 3 });
        assert_eq!(c.registers[2], 0x55, "load must not write rd");
        assert_eq!(c.mem.ram[0], 0x1234_5678, "store must not write memory");
    }
}
