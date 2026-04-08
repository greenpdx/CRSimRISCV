//! Tests for the 5-stage pipeline simulator.
//! See `sim/docs/pipeline-plan.md` §13 for the full test plan.

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
    cpu.ram[0] = 42;
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
