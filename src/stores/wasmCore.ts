// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers for the wasm cores (src/wasm/cpu_sim.js, built from sim/src/wasm).
//
// Every core class (SingleCycle, Pipeline, Barrel, Ooo) has the
// same common methods; see sim/src/wasm/mod.rs. This file holds the parts the
// stores share: the interrupt-line table and the shapes of the common
// methods' JSON results (types generated into cpu_sim.d.ts by tsify).
// ─────────────────────────────────────────────────────────────────────────────

/** External interrupt lines → mip bit (raise_irq takes the mask) */
export const IRQ_MASK = {
  ssi: 1 << 1, msi: 1 << 3, sti: 1 << 5, mti: 1 << 7, sei: 1 << 9, mei: 1 << 11,
} as const
export type IrqLine = keyof typeof IRQ_MASK

/** Mask for an interrupt line name (0 if unknown) */
export const irqMask = (line: string): number => IRQ_MASK[line as IrqLine] ?? 0

// Result types of the common methods, generated from the Rust structs (tsify).
export type {
  Region, CsrEntryJson as CsrEntry, TrapJson as TrapInfo, DisasmJson, BoundaryJson,
} from '../wasm/cpu_sim'
import type { Region } from '../wasm/cpu_sim'

/** Words of the region called `name` (empty if the core has none) */
export function regionWords(
  core: { regions(): Region[]; mem(base: number, n: number): Uint32Array },
  name: string,
): Uint32Array {
  const r = core.regions().find(x => x.name === name)
  return r ? core.mem(r.base, r.words) : new Uint32Array(0)
}
