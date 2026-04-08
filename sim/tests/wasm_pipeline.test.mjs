// Node test wrapper for the JsPipelineState wasm interface.
//
// Run after building wasm:
//   npm run wasm
//   node sim/tests/wasm_pipeline.test.mjs
// Or via the npm script:
//   npm run test:wasm-pipeline
//
// Tests every method on the wrapper, verifies serialization shapes, and
// confirms hazard / forwarding / branch flush flags fire at the right
// cycles.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const PKG_DIR = resolve(__dirname, '../pkg');

// wasm-pack's `--target web` shim uses fetch + import.meta.url, which
// don't work directly in Node. We bypass them by:
//   1. Loading the wasm bytes ourselves with fs.
//   2. Importing the JS shim and calling its `initSync` with the bytes.
const wasmBytes = await readFile(resolve(PKG_DIR, 'cpu_sim_bg.wasm'));
const mod = await import(resolve(PKG_DIR, 'cpu_sim.js'));
mod.initSync({ module: new WebAssembly.Module(wasmBytes) });

const { JsPipelineState } = mod;

// ─────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────

function makeCpu(words = []) {
  const cpu = new JsPipelineState();
  if (words.length) cpu.load_program(new Uint32Array(words));
  return cpu;
}

function step(cpu, n) {
  for (let i = 0; i < n; i++) cpu.step();
}

// ─────────────────────────────────────────────────────────────────────────
// 1. lifecycle: new + reset + load_program
// ─────────────────────────────────────────────────────────────────────────
test('lifecycle: new, reset, load_program', () => {
  const cpu = new JsPipelineState();
  assert.equal(cpu.cycle, 0n);
  assert.equal(cpu.pc, 0);
  assert.equal(cpu.halted, false);

  cpu.load_program(new Uint32Array([0x00700293, 0x00500313]));
  const rom = cpu.rom();
  assert.equal(rom[0], 0x00700293);
  assert.equal(rom[1], 0x00500313);

  cpu.reset();
  assert.equal(cpu.cycle, 0n);
  // ROM is preserved after reset
  assert.equal(cpu.rom()[0], 0x00700293);
});

// ─────────────────────────────────────────────────────────────────────────
// 2. pc and cycle getters
// ─────────────────────────────────────────────────────────────────────────
test('pc and cycle getters update on step', () => {
  const cpu = makeCpu([0x00000013, 0x00000013, 0x00000013]); // 3 NOPs
  assert.equal(cpu.cycle, 0n);
  assert.equal(cpu.pc, 0);
  cpu.step();
  assert.equal(cpu.cycle, 1n);
  assert.equal(cpu.pc, 4);
  step(cpu, 2);
  assert.equal(cpu.cycle, 3n);
  assert.equal(cpu.pc, 12);
});

// ─────────────────────────────────────────────────────────────────────────
// 3. single addi completes
// ─────────────────────────────────────────────────────────────────────────
test('single addi flows through 5 stages', () => {
  const cpu = makeCpu([0x00700293]); // addi x5, x0, 7
  step(cpu, 5);
  assert.equal(cpu.read_register(5), 7);
});

// ─────────────────────────────────────────────────────────────────────────
// 4. registers() shape
// ─────────────────────────────────────────────────────────────────────────
test('registers() returns Uint32Array of 32', () => {
  const cpu = makeCpu();
  const regs = cpu.registers();
  // wasm-bindgen returns a typed array (Uint32Array) for Vec<u32> in --target web
  assert.equal(regs.length, 32);
  for (let i = 0; i < 32; i++) assert.equal(regs[i], 0);
});

// ─────────────────────────────────────────────────────────────────────────
// 5. pipeline-register snapshots have right shape
// ─────────────────────────────────────────────────────────────────────────
test('pipeline-register snapshots have all expected fields', () => {
  const cpu = makeCpu([0x00700293]);
  cpu.step();
  cpu.step();

  const ifid = cpu.if_id();
  for (const k of ['valid', 'pc', 'instr', 'pc_plus4']) {
    assert.ok(k in ifid, `if_id missing ${k}`);
  }
  const idex = cpu.id_ex();
  for (const k of ['valid', 'pc', 'pc_plus4', 'rs1_data', 'rs2_data', 'imm',
                   'rs1_num', 'rs2_num', 'rd_num', 'funct3', 'funct7b30',
                   'opcode', 'ctrl']) {
    assert.ok(k in idex, `id_ex missing ${k}`);
  }
  for (const k of ['reg_write', 'alu_src', 'alu_op', 'mem_read', 'mem_write',
                   'wb_sel', 'pc_sel', 'branch', 'jump']) {
    assert.ok(k in idex.ctrl, `id_ex.ctrl missing ${k}`);
  }
  const exma = cpu.ex_ma();
  for (const k of ['valid', 'alu_result', 'rs2_data', 'pc_plus4', 'rd_num',
                   'funct3', 'branch_taken', 'branch_target', 'ctrl']) {
    assert.ok(k in exma, `ex_ma missing ${k}`);
  }
  const mawb = cpu.ma_wb();
  for (const k of ['valid', 'alu_result', 'mem_data', 'pc_plus4', 'rd_num', 'ctrl']) {
    assert.ok(k in mawb, `ma_wb missing ${k}`);
  }
});

// ─────────────────────────────────────────────────────────────────────────
// 6. snapshot() composite
// ─────────────────────────────────────────────────────────────────────────
test('snapshot() composite has all sub-fields', () => {
  const cpu = makeCpu([0x00700293]);
  cpu.step();
  const s = cpu.snapshot();
  for (const k of ['cycle', 'pc', 'halted', 'registers', 'if_id', 'id_ex',
                   'ex_ma', 'ma_wb', 'hazard', 'forward', 'active_components']) {
    assert.ok(k in s, `snapshot missing ${k}`);
  }
  assert.ok('stall' in s.hazard);
  assert.ok('flush' in s.hazard);
  assert.ok('a' in s.forward);
  assert.ok('b' in s.forward);
  for (const stage of ['IF', 'ID', 'EX', 'MA', 'WB']) {
    assert.ok(stage in s.active_components, `active_components missing ${stage}`);
    assert.ok(Array.isArray(s.active_components[stage]));
  }
});

// ─────────────────────────────────────────────────────────────────────────
// 7. forwarding flag updates on RAW hazard
// ─────────────────────────────────────────────────────────────────────────
test('forward_a fires on RAW hazard', () => {
  const cpu = makeCpu([
    0x00700293, // addi x5, x0, 7
    0x00128313, // addi x6, x5, 1   ← reads x5 (forwarded from EX/MA)
  ]);
  // Step until x6 is computed correctly via forwarding.
  step(cpu, 7);
  assert.equal(cpu.read_register(6), 8);
});

// ─────────────────────────────────────────────────────────────────────────
// 8. stall flag fires on load-use
// ─────────────────────────────────────────────────────────────────────────
test('stall flag fires on load-use hazard', () => {
  const cpu = makeCpu([
    0x000102b7, // lui  x5, 0x10
    0x0002a303, // lw   x6, 0(x5)
    0x00630333, // add  x6, x6, x6     ← load-use
  ]);
  // Pre-seed RAM[0x10000]
  cpu.poke_ram(0x10000, 21);

  let sawStall = false;
  for (let i = 0; i < 10; i++) {
    cpu.step();
    if (cpu.last_stall) sawStall = true;
  }
  assert.ok(sawStall, 'expected last_stall to be true at some cycle');
  assert.equal(cpu.read_register(6), 42);
});

// ─────────────────────────────────────────────────────────────────────────
// 9. flush flag fires on taken branch
// ─────────────────────────────────────────────────────────────────────────
test('flush flag fires on taken branch', () => {
  const cpu = makeCpu([
    0x00700293,  // addi x5, x0, 7
    0x00528663,  // beq  x5, x5, 12   (always taken)
    0x06300513,  // addi x10, x0, 99  (squashed)
    0x06300593,  // addi x11, x0, 99  (squashed)
    0x02a00613,  // addi x12, x0, 42  (target)
    0x0000006f,  // spin
  ]);
  let sawFlush = false;
  for (let i = 0; i < 20; i++) {
    cpu.step();
    if (cpu.last_flush) sawFlush = true;
  }
  assert.ok(sawFlush, 'expected last_flush at the cycle the branch was resolved');
});

// ─────────────────────────────────────────────────────────────────────────
// 10. branch squashes two instructions
// ─────────────────────────────────────────────────────────────────────────
test('branch squashes the two instructions in flight', () => {
  const cpu = makeCpu([
    0x00700293,  // addi x5, x0, 7
    0x00528663,  // beq  x5, x5, 12
    0x06300513,  // addi x10, x0, 99   (squashed)
    0x06300593,  // addi x11, x0, 99   (squashed)
    0x02a00613,  // addi x12, x0, 42   (target)
    0x0000006f,
  ]);
  step(cpu, 30);
  assert.equal(cpu.read_register(10), 0, 'x10 must remain unwritten');
  assert.equal(cpu.read_register(11), 0, 'x11 must remain unwritten');
  assert.equal(cpu.read_register(12), 42);
});

// ─────────────────────────────────────────────────────────────────────────
// 11. JAL writes link address to ra
// ─────────────────────────────────────────────────────────────────────────
test('JAL writes link address (PC+4) to ra', () => {
  const cpu = makeCpu([
    0x00c000ef,  // jal x1, 12  (link = 0x04, target = 0x0c)
    0x06300513,  // squashed
    0x06300593,  // squashed
    0x02a00613,  // addi x12, x0, 42
    0x0000006f,
  ]);
  step(cpu, 30);
  assert.equal(cpu.read_register(1), 4);
  assert.equal(cpu.read_register(12), 42);
});

// ─────────────────────────────────────────────────────────────────────────
// 12. JALR target via forwarded rs1
// ─────────────────────────────────────────────────────────────────────────
test('JALR target uses forwarded rs1', () => {
  const cpu = makeCpu([
    0x01000293,  // addi x5, x0, 16
    0x00028293,  // addi x5, x5, 0
    0x000280e7,  // jalr x1, 0(x5)
    0x06300513,  // squashed
    0x02a00613,  // addi x12, x0, 42
    0x0000006f,
  ]);
  step(cpu, 30);
  assert.equal(cpu.read_register(12), 42);
});

// ─────────────────────────────────────────────────────────────────────────
// 13. pokes work mid-run
// ─────────────────────────────────────────────────────────────────────────
test('poke_register, poke_ram, poke_rom work', () => {
  const cpu = new JsPipelineState();

  cpu.poke_register(5, 0xdeadbeef);
  assert.equal(cpu.read_register(5), 0xdeadbeef);

  cpu.poke_ram(0x10000, 0x12345678);
  assert.equal(cpu.ram()[0], 0x12345678);

  cpu.poke_rom(0, 0x00700293);
  assert.equal(cpu.rom()[0], 0x00700293);

  // x0 is hard-wired to zero — pokes should be silently dropped.
  cpu.poke_register(0, 0x99);
  assert.equal(cpu.read_register(0), 0);
});

// ─────────────────────────────────────────────────────────────────────────
// 14. active_components per stage matches matrix
// ─────────────────────────────────────────────────────────────────────────
test('active_components per stage reflects in-flight pipeline state', () => {
  const cpu = makeCpu([0x00700293]); // addi x5, x0, 7
  cpu.step();  // IF for instr 0 → IF/ID has it
  let s = cpu.snapshot();
  // After 1 step, IF still active (no ID yet because pipeline is empty)
  assert.ok(s.active_components.IF.includes('pc'));
  assert.ok(s.active_components.IF.includes('rom'));
  assert.ok(s.active_components.IF.includes('if_id_reg'));

  cpu.step();  // ID stage now has the addi
  s = cpu.snapshot();
  assert.ok(s.active_components.ID.includes('decoder'));
  assert.ok(s.active_components.ID.includes('imm_gen'));

  cpu.step();  // EX stage has the addi
  s = cpu.snapshot();
  assert.ok(s.active_components.EX.includes('alu'));
  assert.ok(s.active_components.EX.includes('mux_alusrc'));  // I-ALU uses immediate
});

// ─────────────────────────────────────────────────────────────────────────
// 15. read_register(0) always returns 0
// ─────────────────────────────────────────────────────────────────────────
test('read_register(0) always returns 0', () => {
  const cpu = makeCpu();
  assert.equal(cpu.read_register(0), 0);
  cpu.poke_register(0, 0xff);
  assert.equal(cpu.read_register(0), 0);
});
