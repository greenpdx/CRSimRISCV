// Single-cycle save / restore in the browser: SingleCycle.save() → JSON text
// in localStorage → JSON.parse → migrateSave → restore().

import { describe, it, expect, beforeAll, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { useCpuState, migrateSave } from '../cpuState'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let wasm: any
beforeAll(async () => {
  wasm = await import('../../wasm/cpu_sim.js')
  wasm.initSync({ module: new WebAssembly.Module(readFileSync(resolve(__dirname, '../../wasm/cpu_sim_bg.wasm'))) })
})
beforeEach(() => { localStorage.clear(); setActivePinia(createPinia()) })

const addi = (rd: number, rs1: number, imm: number) => ((imm & 0xfff) << 20 | rs1 << 15 | rd << 7 | 0x13) >>> 0
const lui = (rd: number, imm20: number) => ((imm20 << 12) | rd << 7 | 0x37) >>> 0
const sw = (rs2: number, rs1: number, imm: number) => (((imm >> 5) & 0x7f) << 25 | rs2 << 20 | rs1 << 15 | 2 << 12 | (imm & 0x1f) << 7 | 0x23) >>> 0
const csrw = (csr: number, rs1: number) => (csr << 20 | rs1 << 15 | 1 << 12 | 0x73) >>> 0

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const state = (c: any) => JSON.stringify({ pc: c.pc, regs: Array.from(c.regs()), micro: c.micro_raw(), csrs: c.csr_snapshot(), extra: Array.from(c.ram_extra()) })

describe('single-cycle save / restore', () => {
  it('round-trips through JSON, mid-instruction, with sparse RAM and CSRs', () => {
    const c = wasm.SingleCycle.panel()
    c.set_traps_enabled(true)
    // x5 = 0x20000 (outside the 64-word RAM window → ram_extra), mscratch = 99
    c.set_rom(new Uint32Array([lui(5, 0x20), addi(6, 0, 99), sw(6, 5, 0), csrw(0x340, 6), addi(7, 0, 1)]))
    for (let i = 0; i < 43; i++) c.micro_step()
    const text = JSON.stringify(c.save())
    expect(JSON.parse(text).mem.ram_extra).toEqual({ '131072': 99 })
    const d = wasm.SingleCycle.panel()
    expect(d.restore(migrateSave(JSON.parse(text)))).toBe(true)
    expect(state(d)).toBe(state(c))
    expect(d.csr_read(0x340)).toBe(99)
    for (let i = 0; i < 12; i++) { c.micro_step(); d.micro_step() }
    expect(state(d)).toBe(state(c))
  })

  it('restores a save from before `mem` (fields at the top level)', () => {
    const old = JSON.parse(readFileSync(resolve(__dirname, '../../../sim/tests/fixtures/single_cycle_save_v1.json'), 'utf8'))
    // the state the fixture holds: two instructions after 25 micro-steps
    const ref = new wasm.SingleCycle()
    ref.load(0, new Uint32Array([0x0800_0293, 0x0062_a023]))
    for (let i = 0; i < 25; i++) ref.micro_step()
    const d = new wasm.SingleCycle()
    expect(d.restore(migrateSave(old))).toBe(true)
    expect(state(d)).toBe(state(ref))
    expect(Array.from(d.mem(0, 2))).toEqual([0x0800_0293, 0x0062_a023])
  })

  it('the store restores the saved program and core', async () => {
    const prog = [addi(1, 0, 5), addi(2, 1, 3), 0x0000006f]
    const a = useCpuState()
    await a.whenReady()
    a.loadProgram(prog)
    a.runMicro(13)
    a.saveState()
    setActivePinia(createPinia())
    const b = useCpuState()
    await b.whenReady()
    expect(b.romWords.slice(0, 3).map(w => w.value)).toEqual(prog)
    expect(b.pc).toBe(a.pc)
    expect(b.registers[1]!.value).toBe(5)
  })
})
