// Single-cycle panel stores on the Rust core: Traps checkbox and IRQ buttons
// (the old TypeScript phase engine ignored both).

import { describe, it, expect, beforeAll, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { nextTick } from 'vue'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { useCpuState } from '../cpuState'
import { useCpuPhases } from '../cpuPhases'

beforeAll(async () => {
  const wasm = await import('../../wasm/cpu_sim.js')
  wasm.initSync({ module: new WebAssembly.Module(readFileSync(resolve(__dirname, '../../wasm/cpu_sim_bg.wasm'))) })
})
beforeEach(() => { localStorage.clear(); setActivePinia(createPinia()) })

const csrw = (csr: number, rs1: number) => (csr << 20 | rs1 << 15 | 1 << 12 | 0x73) >>> 0
const addi = (rd: number, rs1: number, imm: number) => ((imm & 0xfff) << 20 | rs1 << 15 | rd << 7 | 0x13) >>> 0
const ECALL = 0x00000073, MRET = 0x30200073

async function setup(words: number[], traps: boolean) {
  const cpu = useCpuState()
  await cpu.whenReady()
  const ph = useCpuPhases()
  cpu.trapsEnabled = traps
  await nextTick()
  cpu.loadProgram(words)
  return { cpu, ph }
}

describe('single-cycle traps on the Rust core', () => {
  // mtvec = 0x40; ecall at 0x08; handler at 0x40: mepc += 4, mret
  const prog = [addi(1, 0, 0x40), csrw(0x305, 1), ECALL, addi(5, 0, 7), 0x0000006f]
  while (prog.length < 16) prog.push(0x13)
  prog.push(0x34102373, addi(6, 6, 4), csrw(0x341, 6), MRET)   // csrr x6, mepc; addi; csrw mepc; mret

  it('traps off: ECALL is a no-op', async () => {
    const { cpu, ph } = await setup(prog, false)
    for (let i = 0; i < 4; i++) ph.instantStep()
    expect(cpu.registers[5]!.value).toBe(7)
    expect(cpu.trapDepth).toBe(0)
  })

  it('traps on: ECALL enters the handler and MRET returns', async () => {
    const { cpu, ph } = await setup(prog, true)
    for (let i = 0; i < 3; i++) ph.instantStep()
    expect(cpu.pc).toBe(0x40)
    expect(cpu.trapDepth).toBe(1)
    expect(cpu.csrRead(0x342)).toBe(11)        // mcause = ecall from M
    expect(cpu.lastTrap?.cause).toBe(11)
    for (let i = 0; i < 4; i++) ph.instantStep()   // handler
    expect(cpu.pc).toBe(0x0c)
    expect(cpu.trapDepth).toBe(0)
    ph.instantStep()
    expect(cpu.registers[5]!.value).toBe(7)
  })

  it('IRQ button: MTI taken at the next fetch when enabled', async () => {
    const p2 = [addi(1, 0, 0x40), csrw(0x305, 1), addi(2, 0, 0x80), csrw(0x304, 2), addi(3, 0, 8), csrw(0x300, 3), 0x13, 0x13]
    const { cpu, ph } = await setup(p2, true)
    for (let i = 0; i < 6; i++) ph.instantStep()
    cpu.raiseIrq('mti')
    ph.instantStep()                 // IrqCheck takes it, then the handler's first instruction runs
    expect(cpu.csrRead(0x342) >>> 0).toBe(0x80000007)
    expect(cpu.csrRead(0x341)).toBe(0x18)   // mepc = the interrupted instruction
    expect(cpu.privMode).toBe('M')
    cpu.clearIrqs()
    expect(cpu.pendingIrq).toBe(0)
  })
})
