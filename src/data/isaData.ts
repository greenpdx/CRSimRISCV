// ─────────────────────────────────────────────────────────────────────────────
// Instruction data bundled from the RISC-V ISA Explorer
// (public/data/isa-instructions.json, built by scripts/isa-slice.mjs).
// Loaded at runtime, once.
// ─────────────────────────────────────────────────────────────────────────────
import { shallowRef, type ShallowRef } from 'vue'

/** One instruction of public/data/isa-instructions.json */
export interface IsaInstruction {
  group: string
  name: string
  description: string
  assembly: string
  extension: string | null
  /** 32 characters, bit 31 first: 0 / 1 fixed, - an operand bit */
  encoding: string
  fields: string[]
  match: string
  mask: string
  /** Link that opens this instruction in the RISC-V ISA Explorer (?ext=…&insn=…) */
  explorer: string | null
  note?: string
}
export interface IsaData {
  source: { name: string; repository: string; commit: string; license: string }
  explorer: string
  instructions: Record<string, IsaInstruction>
}

const cache = new Map<string, ShallowRef<unknown>>()

/** A ref that fills in when public/data/<file> has loaded (null until then, or if it fails) */
export function loadData<T>(file: string): ShallowRef<T | null> {
  let r = cache.get(file) as ShallowRef<T | null> | undefined
  if (!r) {
    r = shallowRef<T | null>(null)
    cache.set(file, r as ShallowRef<unknown>)
    const target = r
    fetch(`${import.meta.env.BASE_URL}data/${file}`)
      .then(res => { if (!res.ok) throw new Error(`${file}: ${res.status}`); return res.json() })
      .then(j => { target.value = j as T })
      .catch(e => console.warn('could not load', file, e))
  }
  return r
}

export const useIsaData = () => loadData<IsaData>('isa-instructions.json')

/** The bundled entry of a mnemonic (any case), or null */
export function isaInstruction(data: IsaData | null, mnemonic: string): IsaInstruction | null {
  return data?.instructions[mnemonic.trim().toLowerCase()] ?? null
}

/** The encoding split at the standard field boundaries: funct7 · rs2 · rs1 · funct3 · rd · opcode */
export function encodingFields(encoding: string): string[] {
  return [[0, 7], [7, 12], [12, 17], [17, 20], [20, 25], [25, 32]].map(([a, b]) => encoding.slice(a, b))
}
