//! Tests for the 5-stage pipeline simulator.
//! See `sim/docs/pipeline-plan.md` §13 for the full test plan.

use cpu_sim::sim::system::{addr, PrivMode};
use cpu_sim::sim::pipeline::PipelineState;

/// Run the pipeline for `n` cycles (helper for readability).
fn run(cpu: &mut PipelineState, n: usize) {
    for _ in 0..n { cpu.step(); }
}

/// Run until either a fixed cycle limit or until the program is "done"
/// (PC stops advancing for two cycles in a row, indicating a spin loop).
fn run_until_done(cpu: &mut PipelineState, max: usize) {
    let mut prev_pc = u32::MAX;
    let mut steady = 0;
    for _ in 0..max {
        let pc_before = cpu.pc;
        cpu.step();
        if cpu.pc == pc_before && pc_before == prev_pc { steady += 1; } else { steady = 0; }
        prev_pc = pc_before;
        if steady > 4 { break; }
    }
}

#[test]
fn pipeline_basic_addi() {
    let mut cpu = PipelineState::new();
    // addi x5, x0, 7
    cpu.load_program(&[0x00700293]);
    // 5 cycles to drain through IF→ID→EX→MA→WB
    for _ in 0..5 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 7);
}

#[test]
fn pipeline_two_adds_no_hazard() {
    let mut cpu = PipelineState::new();
    cpu.load_program(&[
        0x00700293, // addi x5, x0, 7
        0x00500313, // addi x6, x0, 5
        0x006283b3, // add  x7, x5, x6
    ]);
    // 7 cycles: 3 instr + 4 pipe-fill (with forwarding, no stalls)
    for _ in 0..7 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 7);
    assert_eq!(cpu.read_register(6), 5);
    assert_eq!(cpu.read_register(7), 12);
}

#[test]
fn pipeline_load_use_stall() {
    let mut cpu = PipelineState::new();
    // Pre-seed RAM[0x10000] = 42 via direct write through the model
    cpu.machine_ram[0] = 42;
    cpu.load_program(&[
        0x000102b7, // lui  x5, 0x10            (x5 = 0x10000)
        0x0002a303, // lw   x6, 0(x5)           (x6 = 42)
        0x00630333, // add  x6, x6, x6          (x6 = 84)  ← load-use
    ]);
    // Need extra cycles for the bubble.
    for _ in 0..10 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 0x10000);
    assert_eq!(cpu.read_register(6), 84);
    assert!(cpu.cycle_count >= 8, "load-use should add at least one cycle");
}

// ─────────────────────────────────────────────────────────────────────────
// Branches
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pipeline_beq_taken_squashes_two_instructions() {
    let mut cpu = PipelineState::new();
    // Layout:
    //   0x00: addi x5, x0, 7      (just to set up x5)
    //   0x04: beq  x5, x5, +12    → branch always taken to 0x10
    //   0x08: addi x10, x0, 99    (must NOT execute — squashed)
    //   0x0c: addi x11, x0, 99    (must NOT execute — squashed)
    //   0x10: addi x12, x0, 42    (target)
    //   0x14: jal  x0, 0          (spin)
    cpu.load_program(&[
        0x00700293,  // addi x5, x0, 7
        0x00528663,  // beq  x5, x5, 12   (B-type, imm=12)
        0x06300513,  // addi x10, x0, 99
        0x06300593,  // addi x11, x0, 99
        0x02a00613,  // addi x12, x0, 42
        0x0000006f,  // jal  x0, 0
    ]);
    run_until_done(&mut cpu, 50);
    assert_eq!(cpu.read_register(5), 7);
    assert_eq!(cpu.read_register(10), 0, "x10 must NOT have been written (squashed)");
    assert_eq!(cpu.read_register(11), 0, "x11 must NOT have been written (squashed)");
    assert_eq!(cpu.read_register(12), 42, "branch target must execute");
}

#[test]
fn pipeline_bne_not_taken_no_penalty() {
    let mut cpu = PipelineState::new();
    //   0x00: addi x5, x0, 7
    //   0x04: bne  x5, x5, +12   → not taken (5 == 5)
    //   0x08: addi x10, x0, 42   (must execute)
    //   0x0c: addi x11, x0, 99   (must execute)
    //   0x10: jal  x0, 0
    cpu.load_program(&[
        0x00700293,
        0x00529663,  // bne x5, x5, 12
        0x02a00513,  // addi x10, x0, 42
        0x06300593,  // addi x11, x0, 99
        0x0000006f,
    ]);
    run_until_done(&mut cpu, 50);
    assert_eq!(cpu.read_register(10), 42);
    assert_eq!(cpu.read_register(11), 99);
}

#[test]
fn pipeline_blt_taken_with_forwarding() {
    let mut cpu = PipelineState::new();
    //   0x00: addi x5, x0, 3
    //   0x04: addi x6, x0, 7
    //   0x08: blt  x5, x6, +8     → 3 < 7, taken to 0x10
    //   0x0c: addi x10, x0, 99   (squashed)
    //   0x10: addi x11, x0, 42   (target)
    //   0x14: jal  x0, 0
    cpu.load_program(&[
        0x00300293,  // addi x5, x0, 3
        0x00700313,  // addi x6, x0, 7
        0x0062c463,  // blt x5, x6, 8
        0x06300513,  // addi x10, x0, 99
        0x02a00593,  // addi x11, x0, 42
        0x0000006f,
    ]);
    run_until_done(&mut cpu, 50);
    assert_eq!(cpu.read_register(10), 0, "branch should squash x10");
    assert_eq!(cpu.read_register(11), 42);
}

// ─────────────────────────────────────────────────────────────────────────
// JAL — unconditional, always taken
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pipeline_jal_links_and_jumps() {
    let mut cpu = PipelineState::new();
    //   0x00: jal x1, +12             → jumps to 0x0c, x1 ← 0x04
    //   0x04: addi x10, x0, 99       (squashed)
    //   0x08: addi x11, x0, 99       (squashed)
    //   0x0c: addi x12, x0, 42       (target)
    //   0x10: jal x0, 0
    cpu.load_program(&[
        0x00c000ef,  // jal x1, 12
        0x06300513,  // addi x10, x0, 99
        0x06300593,  // addi x11, x0, 99
        0x02a00613,  // addi x12, x0, 42
        0x0000006f,
    ]);
    run_until_done(&mut cpu, 50);
    assert_eq!(cpu.read_register(1), 0x04, "ra must be set to PC+4 of the JAL");
    assert_eq!(cpu.read_register(10), 0, "x10 squashed");
    assert_eq!(cpu.read_register(11), 0, "x11 squashed");
    assert_eq!(cpu.read_register(12), 42);
}

// ─────────────────────────────────────────────────────────────────────────
// JALR — indirect jump, target = (rs1 + imm) & ~1
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pipeline_jalr_indirect_call() {
    let mut cpu = PipelineState::new();
    //   0x00: addi x5, x0, 16        (target address in x5)
    //   0x04: jalr x1, 0(x5)         → jumps to 0x10, x1 ← 0x08
    //   0x08: addi x10, x0, 99       (squashed)
    //   0x0c: addi x11, x0, 99       (squashed)
    //   0x10: addi x12, x0, 42       (target)
    //   0x14: jal x0, 0
    cpu.load_program(&[
        0x01000293,  // addi x5, x0, 16
        0x000280e7,  // jalr x1, 0(x5)
        0x06300513,  // addi x10, x0, 99
        0x06300593,  // addi x11, x0, 99
        0x02a00613,  // addi x12, x0, 42
        0x0000006f,
    ]);
    run_until_done(&mut cpu, 50);
    assert_eq!(cpu.read_register(1), 0x08, "ra = PC+4 of the JALR");
    assert_eq!(cpu.read_register(5), 16);
    assert_eq!(cpu.read_register(10), 0, "squashed");
    assert_eq!(cpu.read_register(11), 0, "squashed");
    assert_eq!(cpu.read_register(12), 42, "JALR target executed");
}

#[test]
fn pipeline_jalr_with_forwarding() {
    // JALR's rs1 should be forwarded from a recent ALU op.
    let mut cpu = PipelineState::new();
    //   0x00: addi x5, x0, 16
    //   0x04: addi x5, x5, 0          (x5 still 16; this op is in EX/MA when JALR enters EX)
    //   0x08: jalr x1, 0(x5)         → forwarded x5 (= 16) → target = 0x10
    //   0x0c: addi x10, x0, 99       (squashed)
    //   0x10: addi x12, x0, 42       (target)
    //   0x14: jal x0, 0
    cpu.load_program(&[
        0x01000293,  // addi x5, x0, 16
        0x00028293,  // addi x5, x5, 0
        0x000280e7,  // jalr x1, 0(x5)
        0x06300513,  // addi x10, x0, 99
        0x02a00613,  // addi x12, x0, 42
        0x0000006f,
    ]);
    run_until_done(&mut cpu, 50);
    assert_eq!(cpu.read_register(12), 42, "JALR with forwarded rs1 hit the right target");
}

// ─────────────────────────────────────────────────────────────────────────
// Function call: caller passes arg, callee adds 1, returns
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pipeline_function_call() {
    let mut cpu = PipelineState::new();
    //   0x00: addi a0, x0, 41         (argument)
    //   0x04: jal  ra, +12            → jumps to 0x10
    //   0x08: addi x12, a0, 0         (after return: x12 = 42)
    //   0x0c: jal  x0, 0              (spin at 0x0c)
    //   0x10: addi a0, a0, 1          (callee body: a0 = 42)
    //   0x14: jalr x0, 0(ra)          (return)
    cpu.load_program(&[
        0x02900513,  // addi x10, x0, 41
        0x00c000ef,  // jal x1, 12
        0x00050613,  // addi x12, x10, 0
        0x0000006f,  // jal x0, 0  (spin)
        0x00150513,  // addi x10, x10, 1
        0x00008067,  // jalr x0, 0(x1)
    ]);
    run_until_done(&mut cpu, 100);
    assert_eq!(cpu.read_register(10), 42, "callee returned 42 in a0");
    assert_eq!(cpu.read_register(12), 42, "caller saw the returned value");
}

// ─────────────────────────────────────────────────────────────────────────
// Store/Load round-trip — verify memory writes are visible to later loads
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pipeline_store_load_round_trip() {
    let mut cpu = PipelineState::new();
    cpu.load_program(&[
        0x000102b7,  // lui  x5, 0x10       (x5 = 0x10000)
        0x07b00313,  // addi x6, x0, 123    (value to store)
        0x0062a023,  // sw   x6, 0(x5)      (MEM[0x10000] = 123)
        0x00000013,  // nop
        0x00000013,  // nop
        0x00000013,  // nop
        0x0002a383,  // lw   x7, 0(x5)      (x7 = 123)
        0x0000006f,  // spin
    ]);
    run_until_done(&mut cpu, 50);
    assert_eq!(cpu.read_register(6), 123);
    assert_eq!(cpu.read_register(7), 123);
}

// ─────────────────────────────────────────────────────────────────────────
// Forwarding chain: result of i1 → i2; result of i2 → i3
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pipeline_forwarding_chain() {
    let mut cpu = PipelineState::new();
    cpu.load_program(&[
        0x00500293,  // addi x5, x0, 5
        0x00128313,  // addi x6, x5, 1     (x6 = 6, forwarded from x5)
        0x00130393,  // addi x7, x6, 1     (x7 = 7, forwarded from x6)
        0x00138413,  // addi x8, x7, 1     (x8 = 8, forwarded from x7)
        0x0000006f,
    ]);
    run_until_done(&mut cpu, 30);
    assert_eq!(cpu.read_register(5), 5);
    assert_eq!(cpu.read_register(6), 6);
    assert_eq!(cpu.read_register(7), 7);
    assert_eq!(cpu.read_register(8), 8);
}

// ─────────────────────────────────────────────────────────────────────────
// Tight loop: branch-back exercises both branch resolution and forwarding
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pipeline_tight_loop() {
    let mut cpu = PipelineState::new();
    //   0x00: addi x5, x0, 0       (sum = 0)
    //   0x04: addi x6, x0, 1       (i = 1)
    //   0x08: addi x7, x0, 5       (n = 5)
    //   0x0c: add  x5, x5, x6      (sum += i)   ← loop top
    //   0x10: addi x6, x6, 1       (i++)
    //   0x14: bge  x7, x6, -8      (i <= n? branch back to 0x0c)
    //   0x18: jal  x0, 0           (spin)
    cpu.load_program(&[
        0x00000293,
        0x00100313,
        0x00500393,
        0x006282b3,
        0x00130313,
        0xfe6356e3,  // bge x6, x6, -20  -- oops let me recompute
        0x0000006f,
    ]);
    // Replace with a correctly-encoded backward branch
    // bge x7, x6, -8 → branch from 0x14 to 0x0c (offset -8)
    // B-type with imm = -8 = 0xff8
    //   imm[12]=1, imm[11]=1, imm[10:5]=111111, imm[4:1]=1100
    //   rs2=6, rs1=7, funct3=101 (BGE), opcode=1100011
    //
    //   bits[31] = 1
    //   bits[30:25] = 111111
    //   bits[24:20] = 00110
    //   bits[19:15] = 00111
    //   bits[14:12] = 101
    //   bits[11:8] = 1100
    //   bits[7] = 1
    //   bits[6:0] = 1100011
    // = 1111_1110_0110_0011_1101_1100_1110_0011 = 0xfe63dce3
    cpu.load_program(&[
        0x00000293,  // addi x5, x0, 0
        0x00100313,  // addi x6, x0, 1
        0x00500393,  // addi x7, x0, 5
        0x006282b3,  // add  x5, x5, x6
        0x00130313,  // addi x6, x6, 1
        0xfe63dce3,  // bge  x7, x6, -8
        0x0000006f,  // spin
    ]);
    run_until_done(&mut cpu, 200);
    // sum = 1 + 2 + 3 + 4 + 5 = 15
    assert_eq!(cpu.read_register(5), 15);
}

// ── Zicsr (Phase 1) ──────────────────────────────────────────────────
fn enc_csr(csr: u32, rs1: u32, funct3: u32, rd: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | 0b1110011
}

#[test]
fn pipeline_csrrw_round_trip() {
    let mut cpu = PipelineState::new();
    // addi x6, x0, 0x123 ; csrrw x5, 0x340 (mscratch), x6 ; csrrw x7, 0x340, x0
    let addi_x6 = 0x12300313;
    let csrrw_x5 = enc_csr(0x340, 6, 0b001, 5);
    let csrrw_x7 = enc_csr(0x340, 0, 0b001, 7);
    cpu.load_program(&[addi_x6, csrrw_x5, csrrw_x7]);
    run_until_done(&mut cpu, 30);
    assert_eq!(cpu.sys.csrs.read(0x340), 0); // 2nd csrrw wrote x0=0
    assert_eq!(cpu.read_register(5), 0); // first read saw initial 0
    assert_eq!(cpu.read_register(7), 0x123); // second read saw value from x6
}

#[test]
fn pipeline_csrrs_sets_bits() {
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(0x340, 0x0000_00f0);
    let addi = 0x00f00313; // addi x6, x0, 0x0f
    let csrrs = enc_csr(0x340, 6, 0b010, 5); // csrrs x5, mscratch, x6
    cpu.load_program(&[addi, csrrs]);
    run_until_done(&mut cpu, 20);
    assert_eq!(cpu.read_register(5), 0xf0);     // old value
    assert_eq!(cpu.sys.csrs.read(0x340), 0xff);     // 0xf0 | 0x0f
}

#[test]
fn pipeline_csrrc_clears_bits() {
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(0x340, 0xff);
    let addi = 0x00f00313; // x6 = 0x0f
    let csrrc = enc_csr(0x340, 6, 0b011, 5);
    cpu.load_program(&[addi, csrrc]);
    run_until_done(&mut cpu, 20);
    assert_eq!(cpu.read_register(5), 0xff);
    assert_eq!(cpu.sys.csrs.read(0x340), 0xf0);
}

#[test]
fn pipeline_csrrwi_immediate() {
    let mut cpu = PipelineState::new();
    // csrrwi x5, mscratch, 0x1f  (rs1 field is uimm = 0x1f)
    let csrrwi = enc_csr(0x340, 0x1f, 0b101, 5);
    cpu.load_program(&[csrrwi]);
    run_until_done(&mut cpu, 10);
    assert_eq!(cpu.sys.csrs.read(0x340), 0x1f);
    assert_eq!(cpu.read_register(5), 0);
}

#[test]
fn pipeline_pmpaddr_storage() {
    let mut cpu = PipelineState::new();
    // addi x6, x0, 0x7ff ; csrrw x0, pmpaddr0 (0x3b0), x6
    let addi = 0x7ff00313;
    let csrrw = enc_csr(0x3b0, 6, 0b001, 0);
    cpu.load_program(&[addi, csrrw]);
    run_until_done(&mut cpu, 20);
    assert_eq!(cpu.sys.csrs.read(0x3b0), 0x7ff);
}

#[test]
fn pipeline_mhartid_read_only() {
    let mut cpu = PipelineState::new();
    // csrrwi x5, mhartid (0xf14), 0x1f  — write should be ignored
    let csrrwi = enc_csr(0xf14, 0x1f, 0b101, 5);
    cpu.load_program(&[csrrwi]);
    run_until_done(&mut cpu, 10);
    assert_eq!(cpu.sys.csrs.read(0xf14), 0);
    assert_eq!(cpu.read_register(5), 0);
}

// ── Phase 2: traps + interrupts ──────────────────────────────────────

/// Step exactly enough cycles for the first instruction to reach EX
/// and trap, capturing CSR state right at trap delivery.
fn step_to_trap(cpu: &mut PipelineState) {
    for _ in 0..3 { cpu.step(); }
}

#[test]
fn pipeline_ecall_traps_to_mtvec() {
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    // ecall ; addi x5, x0, 0xaa  (the addi must NOT execute)
    cpu.load_program(&[0x00000073, 0x0aa00293]);
    step_to_trap(&mut cpu);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 11);
    assert_eq!(cpu.sys.csrs.read(addr::MEPC), 0);
    assert_eq!(cpu.pc & !0x3, 0x80);
    // Drain a few more cycles; the addi was squashed.
    for _ in 0..5 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 0);
}

#[test]
fn pipeline_ebreak_traps_with_cause_3() {
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(addr::MTVEC, 0x40);
    cpu.load_program(&[0x00100073]); // ebreak
    step_to_trap(&mut cpu);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 3);
    assert_eq!(cpu.pc & !0x3, 0x40);
}

#[test]
fn pipeline_illegal_instr_traps_with_cause_2() {
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(addr::MTVEC, 0x100);
    cpu.load_program(&[0x0000000B]); // unknown opcode
    step_to_trap(&mut cpu);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 2);
    assert_eq!(cpu.sys.csrs.read(addr::MTVAL), 0x0000000B);
    assert_eq!(cpu.pc & !0x3, 0x100);
}

#[test]
fn pipeline_mret_returns_to_mepc() {
    let mut cpu = PipelineState::new();
    // Preset mepc so mret jumps to a known target containing an addi.
    cpu.sys.csrs.write(addr::MEPC, 0x08);
    // 0x00: mret ; 0x04: nop ; 0x08: addi x6, x0, 0xbb
    cpu.load_program(&[0x30200073, 0x00000013, 0x0bb00313]);
    for _ in 0..10 { cpu.step(); }
    assert_eq!(cpu.read_register(6), 0xbb);
}

#[test]
fn pipeline_external_interrupt_taken() {
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.sys.csrs.write(addr::MSTATUS, 1 << 3); // MIE = 1
    cpu.sys.csrs.write(addr::MIE, 1 << 11);    // MEIE
    cpu.sys.csrs.write(addr::MIP, 1 << 11);    // pending external interrupt
    // Long stream of NOPs (addi x0, x0, 0)
    cpu.load_program(&vec![0x00000013u32; 32]);
    let pc_before = cpu.pc;
    cpu.step(); // takes interrupt at top of step(); pc then advances by 4
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0x8000_000b);
    assert_eq!(cpu.sys.csrs.read(addr::MEPC), pc_before);
    // After redirect to mtvec=0x80 and IF runs, end-of-cycle pc++ → 0x84.
    assert_eq!(cpu.pc, 0x84);
    // mstatus.MIE cleared, MPIE = 1
    assert_eq!((cpu.sys.csrs.read(addr::MSTATUS) >> 3) & 1, 0);
    assert_eq!((cpu.sys.csrs.read(addr::MSTATUS) >> 7) & 1, 1);
}

#[test]
fn pipeline_interrupt_disabled_when_mie_zero() {
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.sys.csrs.write(addr::MSTATUS, 0); // MIE = 0
    cpu.sys.csrs.write(addr::MIE, 1 << 11);
    cpu.sys.csrs.write(addr::MIP, 1 << 11);
    cpu.load_program(&[0x00100293]); // addi x5, x0, 1
    for _ in 0..6 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 1);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0); // no trap
}

// ── Phase 4.1: privilege modes + delegation ──────────────────────────

#[test]
fn pipeline_ecall_from_s_mode_cause_9() {
    let mut cpu = PipelineState::new();
    cpu.sys.priv_mode = PrivMode::from_bits(1); // S
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.load_program(&[0x00000073]); // ecall
    step_to_trap(&mut cpu);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 9);  // env-call-from-S
    assert_eq!(cpu.priv_bits(), 3);    // escalated to M (no delegation)
}

#[test]
fn pipeline_ecall_from_u_delegated_to_s() {
    let mut cpu = PipelineState::new();
    cpu.sys.priv_mode = PrivMode::from_bits(0);               // U
    cpu.sys.csrs.write(addr::MEDELEG, 1 << 8);       // delegate env-call-from-U
    cpu.sys.csrs.write(addr::STVEC, 0xc0);
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.load_program(&[0x00000073]); // ecall
    step_to_trap(&mut cpu);
    assert_eq!(cpu.sys.csrs.read(addr::SCAUSE), 8);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0);  // M side untouched
    assert_eq!(cpu.priv_bits(), 1);    // now in S
    assert_eq!(cpu.pc & !0x3, 0xc0);
}

#[test]
fn pipeline_sret_returns_to_sepc_and_priv_u() {
    let mut cpu = PipelineState::new();
    cpu.sys.priv_mode = PrivMode::from_bits(1);          // executing in S
    cpu.sys.csrs.write(addr::SEPC, 0x10);
    // sstatus.SPP=0 (U) so SRET drops to U
    cpu.sys.csrs.write(addr::MSTATUS, 0);
    cpu.load_program(&[0x10200073]); // sret
    for _ in 0..5 { cpu.step(); }
    assert_eq!(cpu.priv_bits(), 0);
}

#[test]
fn pipeline_mret_restores_prev_priv() {
    let mut cpu = PipelineState::new();
    // Set MPP=01 (S) so MRET drops to S.
    cpu.sys.csrs.write(addr::MSTATUS, 1 << 11);
    cpu.sys.csrs.write(addr::MEPC, 0x10);
    cpu.sys.priv_mode = PrivMode::from_bits(3);
    cpu.load_program(&[0x30200073]); // mret
    for _ in 0..5 { cpu.step(); }
    assert_eq!(cpu.priv_bits(), 1);
}

#[test]
fn pipeline_wfi_is_nop_in_v1() {
    let mut cpu = PipelineState::new();
    // wfi ; addi x5,x0,7
    cpu.load_program(&[0x10500073, 0x00700293]);
    for _ in 0..6 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 7);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0); // not illegal
}

#[test]
fn pipeline_sstatus_is_view_of_mstatus() {
    let mut cpu = PipelineState::new();
    // Set SIE bit via sstatus, observe in mstatus.
    cpu.sys.csrs.write(0x100, 1 << 1);
    assert_eq!(cpu.sys.csrs.read(addr::MSTATUS) & (1 << 1), 1 << 1);
    // Setting MIE in mstatus should NOT show through sstatus.
    { let v = cpu.sys.csrs.read(addr::MSTATUS) | (1 << 3); cpu.sys.csrs.write(addr::MSTATUS, v); }
    assert_eq!(cpu.sys.csrs.read(0x100) & (1 << 3), 0);
}

// ── Phase 4.2: Sv32 page-table walker ────────────────────────────────

/// Build a Sv32 PTE: ppn at bits 31:10 + flag bits in 9:0.
fn pte(ppn: u32, flags: u32) -> u32 { (ppn << 10) | (flags & 0x3ff) }

/// PTE flag bits.
const V: u32 = 1 << 0;
const R: u32 = 1 << 1;
const W: u32 = 1 << 2;
const X: u32 = 1 << 3;
const U: u32 = 1 << 4;

#[test]
fn pipeline_sv32_bare_passthrough_in_s_mode() {
    let mut cpu = PipelineState::new();
    cpu.sys.priv_mode = PrivMode::from_bits(1);
    cpu.sys.csrs.write(addr::SATP, 0); // MODE = Bare
    cpu.load_program(&[0x00700293]); // addi x5, x0, 7
    for _ in 0..6 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 7);
}

#[test]
fn pipeline_sv32_load_traps_when_pte_invalid() {
    let mut cpu = PipelineState::new();
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    // L1[0] identity-maps the code page; L1[1] left invalid so that
    // a load to vaddr 0x00400000 page-faults.
    cpu.ram_write(root + 0*4, pte(0, V | R | W | X));
    cpu.sys.priv_mode = PrivMode::from_bits(1); // S
    cpu.load_program(&[
        0x00400337, // lui x6, 0x400
        0x00032283, // lw x5, 0(x6)
    ]);
    for _ in 0..15 { cpu.step(); }
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 13);
    assert_eq!(cpu.sys.csrs.read(addr::MTVAL), 0x00400000);
    assert_eq!(cpu.read_register(5), 0);
}

#[test]
fn pipeline_sv32_identity_map_load_succeeds() {
    let mut cpu = PipelineState::new();
    // Root page table at phys 0x10000.
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    // L1[0] = leaf 4 MiB superpage at PPN1=0, perms RWX, valid
    let pte1 = pte(0, V | R | W | X);
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    // Use M-mode to set up RAM, then drop to S.
    cpu.ram_write(root + 0*4, pte1);
    cpu.user_ram_write(0x10010, 0xdeadbeef);
    cpu.sys.priv_mode = PrivMode::from_bits(1);
    // lui x6, 0x10 ; lw x5, 0x10(x6)  — virtual = 0x10010
    cpu.load_program(&[
        0x00010337,            // lui x6, 0x10
        0x01032283,            // lw x5, 16(x6)
    ]);
    for _ in 0..10 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 0xdeadbeef);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0);
}

#[test]
fn pipeline_sv32_store_to_read_only_page_traps() {
    let mut cpu = PipelineState::new();
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    // L1[0] superpage with R but not W
    cpu.ram_write(root + 0*4, pte(0, V | R | X));
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.sys.priv_mode = PrivMode::from_bits(1);
    // lui x6, 0x10 ; sw x0, 16(x6)
    cpu.load_program(&[
        0x00010337,
        0x00032823,
    ]);
    for _ in 0..10 { cpu.step(); }
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 15); // store page fault
}

#[test]
fn pipeline_sv32_user_page_blocks_s_access() {
    let mut cpu = PipelineState::new();
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    // L1[0] identity-maps code (no U). L1[1] = U-page leaf with R+W.
    cpu.ram_write(root + 0*4, pte(0, V | R | W | X));
    cpu.ram_write(root + 1*4, pte(1 << 10, V | R | W | U)); // PPN1=1
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.sys.priv_mode = PrivMode::from_bits(1); // S — should NOT be able to access U page (no SUM in 4.2)
    cpu.load_program(&[
        0x00400337,        // lui x6, 0x400
        0x00032283,        // lw x5, 0(x6)
    ]);
    for _ in 0..15 { cpu.step(); }
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 13);
}

// ── Phase 4.3: A/D bits, SUM/MXR, SFENCE.VMA ─────────────────────────

const A_BIT: u32 = 1 << 6;
const D_BIT: u32 = 1 << 7;

#[test]
fn pipeline_sv32_load_sets_a_bit() {
    let mut cpu = PipelineState::new();
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    cpu.ram_write(root + 0*4, pte(0, V | R | W | X)); // identity, no A yet
    cpu.user_ram_write(0x10010, 0xcafebabe);
    cpu.sys.priv_mode = PrivMode::from_bits(1);
    cpu.load_program(&[
        0x00010337, // lui x6, 0x10
        0x01032283, // lw x5, 16(x6)
    ]);
    for _ in 0..10 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 0xcafebabe);
    let pte_v = cpu.ram_read(root + 0*4);
    assert!(pte_v & A_BIT != 0, "A bit should be set after load");
    assert!(pte_v & D_BIT == 0, "D bit should NOT be set after load");
}

#[test]
fn pipeline_sv32_store_sets_a_and_d_bits() {
    let mut cpu = PipelineState::new();
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    cpu.ram_write(root + 0*4, pte(0, V | R | W | X));
    cpu.sys.priv_mode = PrivMode::from_bits(1);
    cpu.load_program(&[
        0x00010337, // lui x6, 0x10
        0x00100293, // addi x5, x0, 1
        0x00532823, // sw x5, 16(x6)
    ]);
    for _ in 0..12 { cpu.step(); }
    let pte_v = cpu.ram_read(root + 0*4);
    assert!(pte_v & A_BIT != 0);
    assert!(pte_v & D_BIT != 0);
}

#[test]
fn pipeline_sv32_sum_allows_s_to_access_u_page() {
    let mut cpu = PipelineState::new();
    // Need a second RAM page for the L0 table.
    cpu.machine_ram.resize(2048, 0);
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    // L1[0] = identity superpage covering code (no U).
    cpu.ram_write(root + 0*4, pte(0, V | R | W | X));
    // L1[1] = pointer to L0 table at phys 0x11000 (PPN = 0x11).
    cpu.ram_write(root + 1*4, pte(0x11, V));
    // L0[0] (vpn0=0) leaf: vaddr 0x00400000 → phys 0x10000, U/R/W.
    cpu.ram_write(0x11000 + 0*4, pte(0x10, V | R | W | U));
    cpu.user_ram_write(0x10010, 0x1234_5678);
    cpu.sys.csrs.write(addr::MSTATUS, 1 << 18); // SUM
    cpu.sys.priv_mode = PrivMode::from_bits(1);
    cpu.load_program(&[
        0x00400337, // lui x6, 0x400
        0x01032283, // lw x5, 16(x6)  → vaddr 0x00400010 → phys 0x10010
    ]);
    for _ in 0..12 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 0x1234_5678);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0);
}

#[test]
fn pipeline_sv32_mxr_allows_load_from_x_only_page() {
    let mut cpu = PipelineState::new();
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    // L1[0] is X-only (executable, NOT readable). Without MXR a load
    // would fault; with MXR set in mstatus it should succeed.
    cpu.ram_write(root + 0*4, pte(0, V | X));
    cpu.sys.csrs.write(addr::MSTATUS, 1 << 19); // MXR
    cpu.sys.priv_mode = PrivMode::from_bits(1);
    cpu.user_ram_write(0x10010, 0xface_d00d);
    cpu.load_program(&[
        0x00010337, // lui x6, 0x10
        0x01032283, // lw x5, 16(x6)
    ]);
    for _ in 0..12 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 0xface_d00d);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0);
}

#[test]
fn pipeline_sfence_vma_is_nop() {
    let mut cpu = PipelineState::new();
    // sfence.vma x0, x0  → 0x12000073
    // (funct7=0x09, rs2=x0, rs1=x0, funct3=0, rd=x0, opcode=0x73)
    cpu.load_program(&[0x12000073, 0x00700293]); // sfence.vma; addi x5, x0, 7
    for _ in 0..6 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 7);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0);
}

// ── Phase 4.4: TLB ───────────────────────────────────────────────────

#[test]
fn pipeline_tlb_hit_after_first_walk() {
    let mut cpu = PipelineState::new();
    let root = 0x10000u32;
    cpu.sys.csrs.write(addr::SATP, (1 << 31) | (root >> 12));
    cpu.ram_write(root + 0*4, pte(0, V | R | W | X));
    cpu.user_ram_write(0x10010, 0xa1a2a3a4);
    cpu.sys.priv_mode = PrivMode::from_bits(1);
    cpu.load_program(&[
        0x00010337, // lui x6, 0x10
        0x01032283, // lw x5, 16(x6)
        0x01032303, // lw x6, 16(x6)  ← second access, should hit TLB
    ]);
    for _ in 0..15 { cpu.step(); }
    assert_eq!(cpu.read_register(5), 0xa1a2a3a4);
    // After running, the TLB superpage entry for vpn1=0 should be installed.
    let any_valid = cpu.dtlb.entries.iter().any(|e| e.valid);
    assert!(any_valid, "TLB should have at least one entry installed");
}

#[test]
fn pipeline_satp_write_keeps_asid_tagged_tlb() {
    // A satp write does not flush (spec / CVA6): entries carry their ASID.
    let mut cpu = PipelineState::new();
    cpu.dtlb.install(0, 0, V | R, true, 0, false);
    let csrrw = enc_csr(0x180, 0, 0b001, 0);   // csrrw x0, satp, x0
    cpu.load_program(&[csrrw]);
    for _ in 0..6 { cpu.step(); }
    assert!(cpu.dtlb.entries.iter().any(|e| e.valid), "satp write must not flush the TLB");
}

#[test]
fn pipeline_tlb_flushed_on_sfence_vma() {
    let mut cpu = PipelineState::new();
    cpu.dtlb.install(0x10, 0x10, V | R | X, false, 0, false);
    cpu.itlb.install(0x10, 0x10, V | R | X, false, 0, true);
    cpu.load_program(&[0x12000073]); // sfence.vma x0, x0
    for _ in 0..6 { cpu.step(); }
    assert!(cpu.dtlb.entries.iter().chain(&cpu.itlb.entries).all(|e| !e.valid),
            "sfence.vma x0, x0 must flush everything, global too");
}

#[test]
fn pipeline_sfence_vma_by_address_and_asid() {
    let sfence = |rs1: u32, rs2: u32| (0x09 << 25) | (rs2 << 20) | (rs1 << 15) | 0x73;
    let lui = |rd: u32, imm: u32| (imm << 12) | (rd << 7) | 0x37;
    let addi = |rd: u32, rs1: u32, imm: u32| (imm << 20) | (rs1 << 15) | (rd << 7) | 0x13;
    // sfence.vma x5, x0 with x5 = 0x10000 just written (forwarded from EX/MA)
    let mut cpu = PipelineState::new();
    cpu.dtlb.install(0x10, 0x10, V | R, false, 1, false);
    cpu.dtlb.install(0x20, 0x20, V | R, false, 1, false);
    cpu.load_program(&[lui(5, 0x10), sfence(5, 0)]);
    for _ in 0..8 { cpu.step(); }
    let v: Vec<bool> = cpu.dtlb.entries.iter().map(|e| e.valid).collect();
    assert_eq!(&v[..2], &[false, true], "only page 0x10 flushed");
    // sfence.vma x0, x6 with x6 = 2: ASID 2 only, global kept
    let mut cpu = PipelineState::new();
    cpu.dtlb.install(0x10, 0x10, V | R, false, 1, false);
    cpu.dtlb.install(0x11, 0x11, V | R, false, 2, false);
    cpu.dtlb.install(0x12, 0x12, V | R, false, 2, true);
    cpu.load_program(&[addi(6, 0, 2), sfence(0, 6)]);
    for _ in 0..8 { cpu.step(); }
    let v: Vec<bool> = cpu.dtlb.entries.iter().map(|e| e.valid).collect();
    assert_eq!(&v[..3], &[true, false, true], "ASID 2 flushed, ASID 1 and the global entry kept");
}

#[test]
fn pipeline_inject_irq_handler_returns() {
    // Mirror the live MicroStepPipelineView default ROM image:
    // 0x00 setup mtvec=0x80; demo body; handler at 0x80; mret at 0x8c.
    let mut cpu = PipelineState::new();
    let nop = 0x00000013u32;
    let mut rom = vec![nop; 64];
    rom[0]  = 0x08000293; // addi t0, x0, 0x80
    rom[1]  = 0x30529073; // csrrw x0, mtvec, t0
    rom[2]  = 0x00500293; // addi x5, x0, 5
    rom[3]  = 0x00128313; // addi x6, x5, 1
    rom[4]  = 0x000102b7; // lui  x5, 0x10
    rom[5]  = 0x0062a023; // sw   x6, 0(x5)
    rom[6]  = 0x0002a383; // lw   x7, 0(x5)
    rom[7]  = 0x00738433; // add  x8, x7, x7
    rom[8]  = 0x00540863; // beq  x8, x5, 16
    rom[9]  = 0x02a00913; // addi s2, x0, 42
    rom[10] = 0x0000006f; // jal  x0, 0  (spin)
    rom[32] = 0x34202373; // csrrs x6, mcause, x0
    rom[33] = 0x34405073; // csrrwi x0, mip, 0
    rom[34] = 0x00140413; // addi s0, s0, 1
    rom[35] = 0x30200073; // mret
    cpu.load_program(&rom);

    // Run setup + a few demo instructions.
    for _ in 0..10 { cpu.step(); }
    // Inject IRQ exactly like the Vue button does.
    { let v = cpu.sys.csrs.read(addr::MSTATUS) | (1 << 3); cpu.sys.csrs.write(addr::MSTATUS, v); }     // MIE
    { let v = cpu.sys.csrs.read(addr::MIE) | (1 << 11); cpu.sys.csrs.write(addr::MIE, v); }    // MEIE
    { let v = cpu.sys.csrs.read(addr::MIP) | (1 << 11); cpu.sys.csrs.write(addr::MIP, v); }    // MEIP

    cpu.step();
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0x8000_000b);
    let mepc = cpu.sys.csrs.read(addr::MEPC);
    // Drain the handler — after ~15 cycles mret has resolved in EX
    // and PC is back near mepc.
    for _ in 0..15 { cpu.step(); }
    assert!(cpu.pc < 0x80 && cpu.pc >= mepc && cpu.pc < mepc + 0x40,
            "pc=0x{:x} should have returned to near mepc=0x{:x}", cpu.pc, mepc);
}


// ── Branch resolution modes ──────────────────────────────────────────
use cpu_sim::sim::pipeline::BranchMode;

#[test]
fn pipeline_mode2_taken_branch_one_bubble() {
    let mut cpu = PipelineState::new();
    cpu.branch_mode = BranchMode::AlwaysTarget;
    cpu.load_program(&[
        0x00500513, // addi a0, x0, 5      ; pc=0x00
        0x00500593, // addi a1, x0, 5      ; pc=0x04
        0x00b50463, // beq  a0, a1, 8      ; pc=0x08, target=0x10
        0x06300613, // addi a2, x0, 0x63   ; pc=0x0c (squashed)
        0x07700693, // addi a3, x0, 0x77   ; pc=0x10 (target)
        0x0000006f, // jal  x0, 0          ; spin
    ]);
    for _ in 0..15 { cpu.step(); }
    assert_eq!(cpu.read_register(10), 5);     // a0
    assert_eq!(cpu.read_register(11), 5);     // a1
    assert_eq!(cpu.read_register(12), 0);     // a2 squashed
    assert_eq!(cpu.read_register(13), 0x77);  // a3 target ran
}

#[test]
fn pipeline_mode2_not_taken_branch_one_bubble() {
    let mut cpu = PipelineState::new();
    cpu.branch_mode = BranchMode::AlwaysTarget;
    cpu.load_program(&[
        0x00500513, // addi a0, x0, 5
        0x00600593, // addi a1, x0, 6
        0x00b50463, // beq  a0, a1, 8       (NOT taken: 5 != 6)
        0x06300613, // addi a2, x0, 0x63    (executes since not taken)
        0x07700693, // addi a3, x0, 0x77    (also executes)
        0x0000006f, // spin
    ]);
    for _ in 0..15 { cpu.step(); }
    // Both fall-through instructions must execute.
    assert_eq!(cpu.read_register(12), 0x63);
    assert_eq!(cpu.read_register(13), 0x77);
}

#[test]
fn pipeline_mode2_jal_no_penalty() {
    // In mode 2 we still pay 1 cycle for JAL (the seq1 fetched at PC+4
    // gets squashed), unless we add the IF-side JAL predecode trick.
    // For now just check correctness: JAL still jumps to its target.
    let mut cpu = PipelineState::new();
    cpu.branch_mode = BranchMode::AlwaysTarget;
    cpu.load_program(&[
        0x0080006f, // jal x0, 8     ; pc=0x00, target=0x08
        0x06300613, // addi a2, x0, 0x63  (squashed)
        0x07700693, // addi a3, x0, 0x77  ; pc=0x08 (target)
        0x0000006f,
    ]);
    for _ in 0..10 { cpu.step(); }
    assert_eq!(cpu.read_register(12), 0);     // squashed
    assert_eq!(cpu.read_register(13), 0x77);
}

#[test]
fn pipeline_mode2_jal_spin_does_not_escape() {
    // jal x0, 0 — target is the JAL itself. In mode 2 the pc must
    // never escape past JAL+4.
    let mut cpu = PipelineState::new();
    cpu.branch_mode = BranchMode::AlwaysTarget;
    let nop = 0x00000013u32;
    let mut rom = vec![nop; 16];
    rom[5] = 0x0000006f;
    cpu.load_program(&rom);
    cpu.pc = 0x14;
    for _ in 0..40 { cpu.step(); }
    assert!(cpu.pc == 0x14 || cpu.pc == 0x18,
            "JAL spin escaped: pc=0x{:x}", cpu.pc);
}

#[test]
fn pipeline_mode2_loop_then_spin() {
    // Mirror the live default ROM: countdown loop, then beq, then
    // JAL spin. After ~50 cycles the pc must be parked in the spin
    // (0x4c or 0x50), not escaped to 0.
    let mut cpu = PipelineState::new();
    cpu.branch_mode = BranchMode::AlwaysTarget;
    let nop = 0x00000013u32;
    let mut rom = vec![nop; 64];
    rom[0]=0x08000293; rom[1]=0x30529073;
    rom[2]=0x00100293; rom[3]=0x00b29293;
    rom[4]=0x30429073; rom[5]=0x00800293; rom[6]=0x3002a073;
    rom[7]=0x00500293; rom[8]=0x00128313; rom[9]=0x000102b7;
    rom[10]=0x0062a023; rom[11]=0x0002a383; rom[12]=0x00738433;
    rom[13]=0x407404b3; rom[14]=0x00300513; rom[15]=0xfff50513;
    rom[16]=0xfe051ee3; rom[17]=0x00540863; rom[18]=0x02a00913;
    rom[19]=0x0000006f;
    cpu.load_program(&rom);
    for _ in 0..80 { cpu.step(); }
    assert!(cpu.pc == 0x4c || cpu.pc == 0x50,
            "demo escaped to pc=0x{:x}", cpu.pc);
    assert_eq!(cpu.read_register(18), 42); // s2 from `addi s2, x0, 42`
}

// ── Shared system unit (sim/system): what the pipeline gained ───────────

#[test]
fn pipeline_irq_line_via_raise_irq() {
    // External line (not a CSR poke): raise_irq ORs into mip at sampling
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.sys.csrs.write(addr::MSTATUS, 1 << 3);
    cpu.sys.csrs.write(addr::MIE, 1 << 7);         // MTIE
    cpu.load_program(&vec![0x00000013u32; 32]);
    cpu.step();
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0);
    cpu.sys.raise_irq(1 << 7);                      // MTIP
    cpu.step();
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0x8000_0007);
    assert!(cpu.sys.in_trap());
}

#[test]
fn pipeline_vectored_mtvec_for_interrupts() {
    let mut cpu = PipelineState::new();
    cpu.sys.csrs.write(addr::MTVEC, 0x100 | 1);     // vectored
    cpu.sys.csrs.write(addr::MSTATUS, 1 << 3);
    cpu.sys.csrs.write(addr::MIE, 1 << 7);
    cpu.sys.csrs.write(addr::MIP, 1 << 7);
    cpu.load_program(&vec![0x00000013u32; 32]);
    cpu.step();
    // handler at base + 4 × cause (7); IF then advances pc by 4
    assert_eq!(cpu.pc, 0x100 + 4 * 7 + 4);
}

#[test]
fn pipeline_s_interrupt_delegated_from_u() {
    let mut cpu = PipelineState::new();
    cpu.sys.priv_mode = PrivMode::U;
    cpu.sys.csrs.write(addr::MIDELEG, 1 << 9);      // SEI → S
    cpu.sys.csrs.write(addr::STVEC, 0xc0);
    cpu.sys.csrs.write(addr::MTVEC, 0x80);
    cpu.sys.csrs.write(addr::MIE, 1 << 9);          // SEIE
    cpu.sys.csrs.write(addr::MIP, 1 << 9);          // SEIP
    cpu.load_program(&vec![0x00000013u32; 64]);
    cpu.step();
    assert_eq!(cpu.priv_bits(), 1);                 // now in S
    assert_eq!(cpu.sys.csrs.read(addr::SCAUSE), 0x8000_0009);
    assert_eq!(cpu.sys.csrs.read(addr::MCAUSE), 0);
    assert_eq!(cpu.pc, 0xc4);
}

#[test]
fn pipeline_counters_and_misa() {
    let mut cpu = PipelineState::new();
    cpu.load_program(&[0x00100293, 0x00200313, 0x00300393]);   // 3 × addi
    for _ in 0..8 { cpu.step(); }
    // 3 addi + the NOPs behind them have left WB by now; cycle = clocks done
    assert_eq!(cpu.sys.csrs.read(addr::CYCLE), 7);
    assert!(cpu.sys.csrs.read(addr::INSTRET) >= 3);
    assert_eq!(cpu.sys.csrs.read(addr::MISA), 0x4000_0100);
    cpu.sys.csrs.write(addr::MISA, 0);                          // read-only
    assert_eq!(cpu.sys.csrs.read(addr::MISA), 0x4000_0100);
    cpu.reset();
    assert_eq!(cpu.sys.csrs.read(addr::MISA), 0x4000_0100);
    assert_eq!(cpu.sys.csrs.read(addr::INSTRET), 0);
}

// ── Data hazards in every branch mode (regression) ──────────────────────
// Each program's dependent instruction must see the producer's value, by
// EX/MA or MA/WB forwarding, WB-before-ID, or a load-use / CSR stall.

mod hazards {
    use cpu_sim::sim::pipeline::{BranchMode, PipelineState};

    fn addi(rd: u32, rs1: u32, imm: i32) -> u32 { ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13 }
    fn add(rd: u32, rs1: u32, rs2: u32) -> u32 { (rs2 << 20) | (rs1 << 15) | (rd << 7) | 0x33 }
    fn jal(rd: u32, off: i32) -> u32 {
        let i = off as u32;
        ((i >> 20) & 1) << 31 | ((i >> 1) & 0x3ff) << 21 | ((i >> 11) & 1) << 20 | ((i >> 12) & 0xff) << 12 | rd << 7 | 0x6f
    }
    fn jalr(rd: u32, rs1: u32, imm: i32) -> u32 { ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x67 }
    fn branch(f3: u32, rs1: u32, rs2: u32, off: i32) -> u32 {
        let o = off as u32;
        ((o >> 12) & 1) << 31 | ((o >> 5) & 0x3f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | ((o >> 1) & 0xf) << 8 | ((o >> 11) & 1) << 7 | 0x63
    }
    fn csr(f3: u32, rd: u32, c: u32, rs1: u32) -> u32 { c << 20 | rs1 << 15 | f3 << 12 | rd << 7 | 0x73 }
    const NOP: u32 = 0x13;
    const MODES: [BranchMode; 3] = [BranchMode::Eager, BranchMode::AlwaysTarget, BranchMode::EarlyResolve];

    /// Run `prog` (+ spin) in every branch mode; check (reg, value) pairs.
    fn check(prog: &[u32], want: &[(u8, u32)]) {
        for mode in MODES {
            let mut p = prog.to_vec();
            p.push(0x0000_006f);
            p.resize(64, NOP);
            let mut cpu = PipelineState::new();
            cpu.branch_mode = mode;
            cpu.load_program(&p);
            for _ in 0..60 { cpu.step(); }
            for &(r, v) in want {
                assert_eq!(cpu.read_register(r), v, "{mode:?}: x{r}");
            }
        }
    }

    #[test] fn raw_1_apart_ex_ma_forward() { check(&[addi(5, 0, 9), addi(6, 5, 1)], &[(6, 10)]); }
    #[test] fn raw_2_apart_ma_wb_forward() { check(&[addi(5, 0, 9), NOP, addi(6, 5, 1)], &[(6, 10)]); }
    #[test] fn raw_3_apart_wb_before_id()  { check(&[addi(5, 0, 9), NOP, NOP, addi(6, 5, 1)], &[(6, 10)]); }
    #[test] fn both_operands_forwarded()    { check(&[addi(5, 0, 9), addi(6, 0, 4), add(7, 5, 6)], &[(7, 13)]); }
    #[test] fn jal_link_used_at_target()    { check(&[jal(1, 8), NOP, addi(5, 1, 0)], &[(5, 4)]); }
    #[test] fn jalr_link_used_at_target()   { check(&[addi(2, 0, 12), jalr(1, 2, 0), NOP, addi(5, 1, 0)], &[(5, 8)]); }
    #[test] fn branch_on_fresh_reg_taken()  { check(&[addi(5, 0, 1), branch(1, 5, 0, 8), addi(6, 0, 7), addi(7, 0, 3)], &[(6, 0), (7, 3)]); }
    #[test] fn branch_on_fresh_reg_not_taken() { check(&[addi(5, 0, 1), branch(0, 5, 0, 8), addi(6, 0, 7), addi(7, 0, 3)], &[(6, 7), (7, 3)]); }
    #[test] fn csrr_result_used_next()      { check(&[addi(5, 0, 5), csr(1, 0, 0x340, 5), csr(2, 6, 0x340, 0), addi(7, 6, 1)], &[(7, 6)]); }
}
