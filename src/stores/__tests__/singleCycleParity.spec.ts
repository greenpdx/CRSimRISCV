// Parity: the single-cycle panel stores (cpuState + cpuPhases, backed by the
// Rust core) against the reference TypeScript engine they replaced
// (__tests__/reference), phase by phase, on everything the panel draws:
// bus values + lit state, control lines, registers (+ read / write marks),
// ROM / RAM words (+ active), I/O, PC, CSRs.
// Programs: every example in CpuLayout.vue + random RV32I streams. Examples
// with byte / halfword accesses or MUL are skipped: the reference engine ran
// every access as a word and had no MUL (singleCycleExt.spec.ts covers them).

import { describe, it, expect, beforeAll } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { useCpuState } from '../cpuState'
import { useCpuPhases, type Phase } from '../cpuPhases'
import { useCpuStateRef } from './reference/cpuStateRef'
import { useCpuPhasesRef } from './reference/cpuPhasesRef'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let wasm: any
beforeAll(async () => {
  wasm = await import('../../wasm/cpu_sim.js')
  wasm.initSync({ module: new WebAssembly.Module(readFileSync(resolve(__dirname, '../../wasm/cpu_sim_bg.wasm'))) })
})

const PHASES: Phase[] = ['fetch', 'decode', 'regread', 'execute', 'memwb', 'pcupdate']

/** Byte / halfword load or store, or MUL (not in the reference engine) */
const newOp = (w: number) =>
  ((w & 0x7f) === 0x03 || (w & 0x7f) === 0x23) ? ((w >>> 12) & 3) !== 2 : (w & 0x7f) === 0x33 && (w >>> 25) === 1

/** Examples from the single-cycle panel source */
function panelExamples(): { name: string; words: number[] }[] {
  const src = readFileSync(resolve(__dirname, '../../components/CpuLayout.vue'), 'utf8')
  const out: { name: string; words: number[] }[] = []
  const re = /name:\s*'([^']+)',\s*program:\s*\[([\s\S]*?)\n\s*\],/g
  for (const m of src.matchAll(re)) {
    const words = [...m[2]!.matchAll(/hex:\s*'(0x[0-9a-fA-F]+)'/g)].map(h => parseInt(h[1]!, 16) >>> 0)
    if (words.length && !words.some(newOp)) out.push({ name: m[1]!, words })
  }
  return out
}

/** Random RV32I program (ALU, LUI/AUIPC, LW/SW in RAM, forward branches / JAL, CSR reads) */
function randomProgram(seed: number, n = 48): number[] {
  let s = seed >>> 0
  const rnd = (k: number) => { s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s % k }
  const r = () => rnd(8)          // x0..x7 (8-reg panel mode)
  const w: number[] = []
  // x7 = RAM base 0x10000, x6 = I/O base 0x40020000
  w.push((0x10 << 12) | (7 << 7) | 0x37)
  w.push((0x40020 << 12) | (6 << 7) | 0x37)
  while (w.length < n - 1) {
    const k = rnd(14)
    const rd = 1 + rnd(5), rs1 = r(), rs2 = r()
    const imm = (rnd(4096) - 2048) & 0xfff
    switch (k) {
      case 0: case 1: { const f3 = rnd(8); const f7 = (f3 === 0 || f3 === 5) && rnd(2) ? 0x20 : 0
        w.push(f7 << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | rd << 7 | 0x33); break }
      case 2: case 3: { const f3 = rnd(8); let im = imm
        if (f3 === 1) im = rnd(32); if (f3 === 5) im = rnd(32) | (rnd(2) ? 0x400 : 0)
        w.push(im << 20 | rs1 << 15 | f3 << 12 | rd << 7 | 0x13); break }
      case 4: w.push((rnd(1 << 20) << 12) | rd << 7 | (rnd(2) ? 0x37 : 0x17)); break
      case 5: { const off = rnd(32) * 4; w.push(off << 20 | 7 << 15 | 2 << 12 | rd << 7 | 0x03); break }
      case 6: { const off = rnd(32) * 4; w.push(((off >> 5) << 25) | rs2 << 20 | 7 << 15 | 2 << 12 | ((off & 31) << 7) | 0x23); break }
      case 7: { const f3 = [0, 1, 4, 5, 6, 7][rnd(6)]!; const off = 8
        w.push(((off >> 11) & 1) << 7 | ((off >> 1) & 0xf) << 8 | ((off >> 5) & 0x3f) << 25 | ((off >> 12) & 1) << 31 | rs2 << 20 | rs1 << 15 | f3 << 12 | 0x63); break }
      case 8: { const off = 8; w.push(((off >> 1) & 0x3ff) << 21 | rd << 7 | 0x6f); break }
      case 9: w.push([0xC00, 0xC02, 0x340][rnd(3)]! << 20 | 2 << 12 | rd << 7 | 0x73); break   // csrr rd, cycle / instret / mscratch
      case 10: w.push(0x340 << 20 | rs1 << 15 | [1, 2, 3, 5, 6, 7][rnd(6)]! << 12 | rd << 7 | 0x73); break  // csr ops on mscratch
      case 11: w.push(rnd(2) ? (rs2 << 20 | 6 << 15 | 2 << 12 | 0x23) : (6 << 15 | 2 << 12 | rd << 7 | 0x03)); break  // IO sw / lw
      case 12: { const off = rnd(16) * 4; w.push(((off >> 5) << 25) | rs2 << 20 | 0 << 15 | 2 << 12 | ((off & 31) << 7) | 0x23); break }  // sw outside the RAM window
      case 13: w.push([0x00000073, 0x00100073, 0x30200073, 0x10200073, 0x0000000b, 0x0000100f][rnd(6)]!); break  // system / unknown, traps off
    }
  }
  w.push(0x0000006f)  // j .
  return w
}

const hex = (v: number) => '0x' + (v >>> 0).toString(16)

/** First difference between the new stores and the reference engine, or null */
async function firstDiff(name: string, words: number[], instrs: number): Promise<string | null> {
  localStorage.clear()
  setActivePinia(createPinia())
  const cpu = useCpuState()
  await cpu.whenReady()
  const ph = useCpuPhases()
  const ref = useCpuStateRef()
  const phRef = useCpuPhasesRef()
  ph.reset(); phRef.reset()
  cpu.loadProgram(words); ref.loadProgram(words)
  for (let i = 0; i < instrs; i++) {
    for (const p of PHASES) {
      ph.runPhase(p); phRef.runPhase(p)
      const at = `${name} instr ${i} ir ${hex(ref.lastInstr)} @${p}`
      for (const [id, b] of Object.entries(ref.busStates)) {
        const n = cpu.busStates[id]!
        if ((n.value >>> 0) !== (b.value >>> 0) || n.state !== b.state) return `${at}: bus ${id} ref=${hex(b.value)}/${b.state} new=${hex(n.value)}/${n.state}`
      }
      for (const [id, c] of Object.entries(ref.controlStates)) {
        if (cpu.controlStates[id]!.asserted !== c.asserted) return `${at}: ctrl ${id} ref=${c.asserted} new=${cpu.controlStates[id]!.asserted}`
      }
      for (let k = 0; k < 32; k++) {
        const a = ref.registers[k]!, b = cpu.registers[k]!
        if ((a.value >>> 0) !== (b.value >>> 0) || !!a.active !== !!b.active || !!a.readA !== !!b.readA || !!a.readB !== !!b.readB)
          return `${at}: x${k} ref=${hex(a.value)} ${a.active}/${a.readA}/${a.readB} new=${hex(b.value)} ${b.active}/${b.readA}/${b.readB}`
      }
      for (let k = 0; k < ref.romWords.length; k++) {
        if (!!ref.romWords[k]!.active !== !!cpu.romWords[k]!.active) return `${at}: rom ${k} active ref=${ref.romWords[k]!.active}`
      }
      const rr = new Map(ref.ramWords.map(w => [w.address >>> 0, w]))
      const nr = new Map(cpu.ramWords.map(w => [w.address >>> 0, w]))
      for (const [a, w] of rr) {
        const n = nr.get(a)
        if (!n || (n.value >>> 0) !== (w.value >>> 0) || !!n.active !== !!w.active) return `${at}: ram ${hex(a)} ref=${hex(w.value)}/${w.active} new=${n ? hex(n.value) + '/' + n.active : '-'}`
      }
      for (const [a, n] of nr) if (!rr.has(a) && n.value !== 0) return `${at}: ram ${hex(a)} only in new = ${hex(n.value)}`
      if ((cpu.ioData >>> 0) !== (ref.ioData >>> 0) || cpu.ioOp !== ref.ioOp) return `${at}: io ref=${hex(ref.ioData)}/${ref.ioOp} new=${hex(cpu.ioData)}/${cpu.ioOp}`
      if ((cpu.pc >>> 0) !== (ref.pc >>> 0) || (cpu.nextPc >>> 0) !== (ref.nextPc >>> 0)) return `${at}: pc ref=${hex(ref.pc)}/${hex(ref.nextPc)} new=${hex(cpu.pc)}/${hex(cpu.nextPc)}`
      if ((cpu.lastInstr >>> 0) !== (ref.lastInstr >>> 0)) return `${at}: lastInstr ref=${hex(ref.lastInstr)} new=${hex(cpu.lastInstr)}`
      if (cpu.lastOpEntry?.name !== ref.lastOpEntry?.name) return `${at}: opEntry ref=${ref.lastOpEntry?.name} new=${cpu.lastOpEntry?.name}`
      if (cpu.cycleCount !== ref.cycleCount) return `${at}: cycleCount ref=${ref.cycleCount} new=${cpu.cycleCount}`
      for (const a of [0x340, 0xC00, 0xC02]) {
        if (cpu.csrRead(a) !== (ref.csrRead(a) >>> 0)) return `${at}: csr ${hex(a)} ref=${hex(ref.csrRead(a))} new=${hex(cpu.csrRead(a))}`
      }
      if (ph.currentPhase !== phRef.currentPhase) return `${at}: phase ref=${phRef.currentPhase} new=${ph.currentPhase}`
    }
  }
  return null
}

describe('single-cycle: TypeScript engine vs Rust core', () => {
  for (const ex of panelExamples()) {
    it(`example: ${ex.name}`, async () => { const d = await firstDiff(ex.name, ex.words, Math.max(ex.words.length + 4, 12)); if (d) console.log("DIFF " + d); expect(d).toBeNull() })
  }
  for (let seed = 1; seed <= 60; seed++) {
    it(`random program ${seed}`, async () => { const d = await firstDiff(`rand${seed}`, randomProgram(seed * 2654435761), 48); if (d) console.log("DIFF " + d); expect(d).toBeNull() })
  }
})
