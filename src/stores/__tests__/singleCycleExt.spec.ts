// Single-cycle panel stores on the Rust core: byte / halfword loads and
// stores, the MUL block (Zmmul on / off), misaligned accesses. These ops are
// new (the old TypeScript engine ran every access as a word and had no MUL),
// so this checks expected values instead of parity.

import { describe, it, expect, beforeAll, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { nextTick } from 'vue'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { useCpuState } from '../cpuState'
import { useCpuPhases, type Phase } from '../cpuPhases'
import { useCpuConfig } from '../cpuConfig'
import { DEFAULT_PROGRAM } from '../../data/defaultProgram'

beforeAll(async () => {
  const wasm = await import('../../wasm/cpu_sim.js')
  wasm.initSync({ module: new WebAssembly.Module(readFileSync(resolve(__dirname, '../../wasm/cpu_sim_bg.wasm'))) })
})
beforeEach(() => { localStorage.clear(); setActivePinia(createPinia()) })

/** Example program from the single-cycle panel source */
function example(name: string): number[] {
  const src = readFileSync(resolve(__dirname, '../../components/CpuLayout.vue'), 'utf8')
  for (const m of src.matchAll(/name:\s*'([^']+)',\s*program:\s*\[([\s\S]*?)\n\s*\],/g)) {
    if (m[1] === name) return [...m[2]!.matchAll(/hex:\s*'(0x[0-9a-fA-F]+)'/g)].map(h => parseInt(h[1]!, 16) >>> 0)
  }
  throw new Error(`no example ${name}`)
}

async function setup(words: number[], opts: { traps?: boolean; zmmul?: boolean } = {}) {
  const config = useCpuConfig()
  config.enableZmmul = opts.zmmul ?? true
  const cpu = useCpuState()
  await cpu.whenReady()
  const ph = useCpuPhases()
  cpu.trapsEnabled = opts.traps ?? false
  await nextTick()
  cpu.loadProgram(words)
  return { cpu, ph }
}

const RAM = 0x10000
const reg = (cpu: ReturnType<typeof useCpuState>, k: number) => cpu.registers[k]!.value >>> 0
const ram = (cpu: ReturnType<typeof useCpuState>, a: number) => (cpu.ramWords.find(w => w.address === a)?.value ?? 0) >>> 0
const lw = (rd: number, rs1: number, off: number) => (off << 20 | rs1 << 15 | 2 << 12 | rd << 7 | 0x03) >>> 0
const lui = (rd: number, imm20: number) => (imm20 << 12 | rd << 7 | 0x37) >>> 0
const addi = (rd: number, rs1: number, imm: number) => ((imm & 0xfff) << 20 | rs1 << 15 | rd << 7 | 0x13) >>> 0
const csrw = (csr: number, rs1: number) => (csr << 20 | rs1 << 15 | 1 << 12 | 0x73) >>> 0

describe('single-cycle: sized loads / stores, MUL block', () => {
  it('default program: SB lanes merge, LH reads the halfword', async () => {
    const { cpu, ph } = await setup(DEFAULT_PROGRAM.map(e => e.value))
    for (let i = 0; i < 80 && cpu.pc !== 0xd8; i++) ph.instantStep()
    expect(cpu.pc).toBe(0xd8)
    expect(reg(cpu, 26)).toBe(0x6655)          // lh  (was 0x66: SB used to write the whole word)
    expect(reg(cpu, 27)).toBe(0x6655)          // lw
    expect(ram(cpu, RAM)).toBe(0x6655)
    expect(ram(cpu, RAM + 4)).toBe(0x6655)
    expect(reg(cpu, 29)).toBe(16)              // function call result unchanged
  })

  it('Byte / half lanes example', async () => {
    const { cpu, ph } = await setup(example('Byte / half lanes'))
    for (let i = 0; i < 17; i++) ph.instantStep()
    expect([1, 2, 3, 4, 5, 6, 7].map(k => reg(cpu, k))).toEqual(
      [0xab, 0x80ffab01, 0x00ab0000, 0x40020000, 0xffff80ff, 0x80ff, 0x10000])
    expect(ram(cpu, RAM)).toBe(0x80ffab01)
    expect(ram(cpu, RAM + 4)).toBe(0x00ab0000)
    expect(cpu.ioData).toBe(0xab)
  })

  it('SB marks only its byte lane in the RAM row', async () => {
    const { cpu, ph } = await setup(example('Byte / half lanes'))
    for (let i = 0; i < 10; i++) ph.instantStep()      // up to sb x1, 1(x7)
    for (const p of ['fetch', 'decode', 'regread', 'execute', 'memwb'] as Phase[]) ph.runPhase(p)
    const w = cpu.ramWords.find(x => x.address === RAM)!
    expect(w.active).toBe(true)
    expect(w.lanes).toBe(0b0010)
    ph.runPhase('pcupdate'); ph.runPhase('fetch')
    expect(w.lanes).toBeUndefined()
  })

  it('Multiply example: MUL block → WB Sel 4', async () => {
    const { cpu, ph } = await setup(example('Multiply (Zmmul)'))
    ph.instantStep(); ph.instantStep()
    ph.runPhase('fetch'); ph.runPhase('decode')
    expect(cpu.busStates['ctrl_wb_sel']!.value).toBe(4)
    ph.runPhase('regread'); ph.runPhase('execute')
    expect(cpu.busStates['mul_result']!.value).toBe(42)
    expect(cpu.busStates['mul_result']!.state).toBe('active')
    ph.runPhase('memwb'); ph.runPhase('pcupdate')
    expect(reg(cpu, 3)).toBe(42)
    for (let i = 0; i < 40 && cpu.pc !== 0x3c; i++) ph.instantStep()
    expect([1, 2, 3, 4, 5, 6, 7].map(k => reg(cpu, k))).toEqual(
      [120, 0, 0x10000, 0xffffffee, 0xffffffff, 0xffffffff, 5])
    expect(ram(cpu, RAM)).toBe(120)
  })

  it('Zmmul off, traps off: MUL is a NOP', async () => {
    const { cpu, ph } = await setup(example('Multiply (Zmmul)'), { zmmul: false })
    for (let i = 0; i < 3; i++) ph.instantStep()
    expect(reg(cpu, 3)).toBe(0)
    expect(cpu.pc).toBe(12)
  })

  it('Zmmul off, traps on: MUL is an illegal instruction', async () => {
    const prog = [addi(1, 0, 0x40), csrw(0x305, 1), 0x022081b3]     // mtvec = 0x40; mul x3, x1, x2
    const { cpu, ph } = await setup(prog, { zmmul: false, traps: true })
    for (let i = 0; i < 3; i++) ph.instantStep()
    expect(cpu.pc).toBe(0x40)
    expect(cpu.lastTrap?.cause).toBe(2)
    expect(cpu.csrRead(0x343)).toBe(0x022081b3)    // mtval = the instruction
    expect(reg(cpu, 3)).toBe(0)
  })

  it('Zmmul switch reaches the core while running', async () => {
    const config = useCpuConfig()
    const { cpu, ph } = await setup(example('Multiply (Zmmul)'), { zmmul: false })
    config.enableZmmul = true
    await nextTick()
    for (let i = 0; i < 3; i++) ph.instantStep()
    expect(reg(cpu, 3)).toBe(42)
  })

  it('misaligned LW: traps off reads the aligned word, traps on raises cause 4', async () => {
    const sw = (rs2: number, rs1: number) => (rs2 << 20 | rs1 << 15 | 2 << 12 | 0x23) >>> 0
    const prog = [addi(1, 0, 0x40), csrw(0x305, 1), lui(7, 0x10), addi(3, 0, 0x77), sw(3, 7), addi(2, 0, 0x55), lw(2, 7, 2)]
    {
      const { cpu, ph } = await setup(prog)
      for (let i = 0; i < 7; i++) ph.instantStep()
      expect(reg(cpu, 2)).toBe(0x77)           // LW @ RAM+2 → the word @ RAM
      expect(cpu.pc).toBe(28)
    }
    setActivePinia(createPinia()); localStorage.clear()
    {
      const { cpu, ph } = await setup(prog, { traps: true })
      for (let i = 0; i < 6; i++) ph.instantStep()
      for (const p of ['fetch', 'decode', 'regread', 'execute', 'memwb'] as Phase[]) ph.runPhase(p)
      expect(cpu.ramWords.some(w => w.active)).toBe(false)     // no access
      ph.runPhase('pcupdate')
      expect(cpu.pc).toBe(0x40)
      expect(cpu.lastTrap?.cause).toBe(4)
      expect(cpu.csrRead(0x343)).toBe(RAM + 2)
      expect(reg(cpu, 2)).toBe(0x55)          // rd not written
    }
  })
})
