// ─────────────────────────────────────────────────────────────────────────────
// Memory panel — two pipeline cores on the shared cache hierarchy, all in
// Rust (sim/src/sim/memsys, wasm JsMemPanel). This store only steps the
// machine and turns its per-cycle events into what the panel highlights.
//
// A core's highlights stay on its last access until it makes the next one,
// so a miss stays visible while the core is frozen waiting for memory.
// ─────────────────────────────────────────────────────────────────────────────

import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'

export type Mesi = 'I' | 'S' | 'E' | 'M'
export type CacheId = 'L1I' | 'L1D' | 'L2'
export type Port = 'Fetch' | 'Load' | 'Store' | 'Walk'
export type Outcome = 'L1Hit' | 'L2Hit' | 'Memory' | 'Rom'

/** One event from the Rust memory system (serde tag = kind) */
export interface EntryCmp { valid: boolean; vpn_match: boolean; asid_match: boolean; global: boolean; hit: boolean }
export interface EntryFlush { valid: boolean; va_match: boolean; asid_match: boolean; global: boolean; inval: boolean }
export type PteVerdict = 'Pointer' | 'Leaf' | 'Invalid' | 'Reserved' | 'Misaligned' | 'NoLeaf'

export type MemEvent =
  | { kind: 'Bare'; core: number; port: Port; va: number }
  | { kind: 'TlbLookup'; core: number; port: Port; va: number; asid: number; cmp: EntryCmp[]; hit: number | null; plru_before: number; plru_after: number }
  | { kind: 'PermCheck'; core: number; port: Port; pte: number; priv_mode: number; sum: boolean; mxr: boolean; ok: boolean }
  | { kind: 'PteAddr'; core: number; port: Port; level: number; base: number; vpn: number; addr: number }
  | { kind: 'PteCheck'; core: number; port: Port; level: number; pte: number; verdict: PteVerdict }
  | { kind: 'AdUpdate'; core: number; port: Port; pte_addr: number; old: number; new: number }
  | { kind: 'TlbVictim'; core: number; port: Port; idx: number; was_invalid: boolean; plru_before: number; plru_after: number }
  | { kind: 'Sfence'; core: number; va: number | null; asid: number | null; itlb: EntryFlush[]; dtlb: EntryFlush[] }
  | { kind: 'TlbHit'; core: number; port: Port; idx: number; va: number; pa: number }
  | { kind: 'TlbMiss'; core: number; port: Port; va: number }
  | { kind: 'Walk'; core: number; port: Port; level: number; pte_addr: number; pte: number }
  | { kind: 'TlbFill'; core: number; port: Port; idx: number; va: number; pa: number }
  | { kind: 'PageFault'; core: number; port: Port; va: number; cause: number }
  | { kind: 'Access'; core: number; port: Port; pa: number; outcome: Outcome; latency: number }
  | { kind: 'L1Hit'; core: number; cache: CacheId; set: number; way: number; pa: number }
  | { kind: 'L1Miss'; core: number; cache: CacheId; set: number; pa: number }
  | { kind: 'L2Hit'; core: number; set: number; way: number; pa: number }
  | { kind: 'L2Miss'; core: number; set: number; pa: number }
  | { kind: 'MemRead'; core: number; pa: number }
  | { kind: 'MemWrite'; pa: number }
  | { kind: 'RomRead'; core: number; pa: number; val: number }
  | { kind: 'Fill'; core: number; cache: CacheId; set: number; way: number; pa: number; state: Mesi }
  | { kind: 'Evict'; core: number; cache: CacheId; set: number; way: number; pa: number; dirty: boolean }
  | { kind: 'WriteBack'; core: number; set: number; way: number; pa: number }
  | { kind: 'StateChange'; core: number; cache: CacheId; set: number; way: number; pa: number; from: Mesi; to: Mesi }
  | { kind: 'Snoop'; from: number; to: number; req: string; cache: CacheId; set: number; way: number; pa: number; before: Mesi; after: Mesi; flush: boolean }
  | { kind: 'BusWait'; core: number; cycles: number }
  | { kind: 'Stall'; core: number; left: number }

export interface McStage { valid: boolean; pc: number; instr: number; bubble: boolean }
export interface MpTlb { valid: boolean; vpn: number; ppn: number; pte: number; is_super: boolean; asid: number; global: boolean }
export interface MpCore {
  pc: number; priv_mode: number
  satp: number; mstatus: number; mtvec: number; mepc: number; mcause: number; mtval: number; mhartid: number
  stalled: boolean; stall_left: number; stages: McStage[]; itlb: MpTlb[]; dtlb: MpTlb[]
  itlb_plru: number; dtlb_plru: number
}

// ── MMU phases: what the hardware does in one access, one step at a time ──
export type Side = 'I' | 'D'
export type PhaseKind = 'bare' | 'split' | 'compare' | 'perm' | 'hit' | 'miss' | 'pteaddr' | 'pteread' | 'ptecheck'
  | 'ad' | 'victim' | 'fill' | 'fault' | 'sfence-gates' | 'sfence-inval'
type AccessEv = Extract<MemEvent, { kind: 'Access' }>
export interface Phase {
  /** Index in this cycle's phase list (−1: not stepped, e.g. a skipped TLB hit) */
  gi: number
  core: number
  /** TLB side; null = both (SFENCE.VMA) */
  side: Side | null
  kind: PhaseKind
  ev: MemEvent
  /** The L1D access of a PTE read (pteread) */
  read?: AccessEv
}
/** One translation (or one SFENCE.VMA) of one core in one cycle */
export interface Group { core: number; side: Side | null; sfence: boolean; hit: boolean; phases: Phase[] }
/** A core's MMU work in the last cycle it did any, and its state before that cycle */
export interface CoreMmu { cycle: number; groups: Group[]; before: MpCore | null }

const MMU_KINDS = new Set(['Bare', 'TlbLookup', 'PermCheck', 'TlbHit', 'TlbMiss', 'PteAddr', 'Walk', 'PteCheck', 'AdUpdate', 'TlbVictim', 'TlbFill', 'PageFault', 'Sfence'])

/** Split a cycle's events into per-core translation groups and phases */
function groupsOf(evs: MemEvent[]): Group[][] {
  const out: Group[][] = [[], []]
  const cur: (Group | null)[] = [null, null]
  const pendingRead: (AccessEv | undefined)[] = [undefined, undefined]
  for (const e of evs) {
    if (e.kind === 'Access' && e.port === 'Walk') { pendingRead[e.core] = e; continue }
    if (!MMU_KINDS.has(e.kind) || !('core' in e)) continue
    const k = e.core
    const sd: Side | null = 'port' in e ? (e.port === 'Fetch' ? 'I' : 'D') : null
    if (e.kind === 'TlbLookup' || e.kind === 'Bare' || e.kind === 'Sfence' || !cur[k]) {
      cur[k] = { core: k, side: sd, sfence: e.kind === 'Sfence', hit: e.kind === 'TlbLookup' && e.hit !== null, phases: [] }
      out[k]!.push(cur[k]!)
    }
    const g = cur[k]!
    const ph = (kind: PhaseKind, extra: Partial<Phase> = {}) => g.phases.push({ gi: -1, core: k, side: g.side, kind, ev: e, ...extra })
    switch (e.kind) {
      case 'Bare': ph('bare'); break
      case 'TlbLookup': ph('split'); ph('compare'); break
      case 'PermCheck': ph('perm'); break
      case 'TlbHit': ph('hit'); break
      case 'TlbMiss': ph('miss'); break
      case 'PteAddr': ph('pteaddr'); break
      case 'Walk': ph('pteread', { read: pendingRead[k] }); pendingRead[k] = undefined; break
      case 'PteCheck': ph('ptecheck'); break
      case 'AdUpdate': ph('ad'); break
      case 'TlbVictim': ph('victim'); break
      case 'TlbFill': ph('fill'); break
      case 'PageFault': ph('fault'); break
      case 'Sfence': ph('sfence-gates'); ph('sfence-inval'); break
    }
  }
  return out
}

const fh = (n: number, w: number) => (n >>> 0).toString(16).padStart(w, '0')
const OUT: Record<Outcome, string> = { L1Hit: 'L1D hit', L2Hit: 'L1D miss, L2 hit', Memory: 'L1D + L2 miss, memory', Rom: 'boot ROM (uncached)' }
const VERDICT: Record<PteVerdict, string> = {
  Pointer: 'V=1, R=W=X=0 → pointer to the next-level table',
  Leaf: 'V=1 with R/X → leaf: this is the translation',
  Invalid: 'V=0 → invalid → page fault',
  Reserved: 'W=1 without R → reserved encoding → page fault',
  Misaligned: 'level-1 leaf with PPN[0] ≠ 0 → misaligned superpage → page fault',
  NoLeaf: 'pointer at level 0 → no more levels → page fault',
}
const MODE_TXT = ['U', 'S', '?', 'M']

/** The caption of one phase */
export function phaseText(p: Phase): string {
  const e = p.ev, c = `C${p.core} ${p.side ?? ''}`.trim() + ': '
  const tlb = `${p.side}TLB`
  switch (p.kind) {
    case 'bare': return `${c}no translation (M-mode or satp.MODE = Bare) — PA = VA ${fh((e as { va: number }).va, 8)}`
    case 'split': { const x = e as Extract<MemEvent, { kind: 'TlbLookup' }>
      return `${c}VA ${fh(x.va, 8)} = VPN[1] ${fh(x.va >>> 22, 3)} · VPN[0] ${fh((x.va >>> 12) & 0x3ff, 3)} · offset ${fh(x.va & 0xfff, 3)}; ASID ${x.asid} from satp` }
    case 'compare': { const x = e as Extract<MemEvent, { kind: 'TlbLookup' }>
      const arr = x.cmp.map(q => q.hit ? '1' : '0').join('')
      return `${c}${tlb} compares VPN ${fh(x.va >>> 12, 5)} + ASID ${x.asid} with all 4 entries at once (valid ∧ VPN = ∧ (ASID = ∨ G)) → hit array ${arr} → ${x.hit !== null ? `HIT entry ${x.hit}` : 'MISS'}` }
    case 'perm': { const x = e as Extract<MemEvent, { kind: 'PermCheck' }>
      const need = p.side === 'I' ? 'X' : (e as { port: Port }).port === 'Store' ? 'W' : x.mxr ? 'R (or X, MXR)' : 'R'
      return `${c}permission check: ${(e as { port: Port }).port.toLowerCase()} needs ${need}; ${MODE_TXT[x.priv_mode]}-mode, U=${(x.pte >> 4) & 1}${x.sum ? ', SUM=1' : ''} → ${x.ok ? 'allowed' : 'DENIED → page fault'}` }
    case 'hit': { const x = e as Extract<MemEvent, { kind: 'TlbHit' }>
      return `${c}PA = PPN ∥ offset = ${fh(x.pa, 8)} → tag to L1${p.side}; PLRU tree moves away from entry ${x.idx}` }
    case 'miss': return `${c}TLB miss → the page table walker leaves IDLE`
    case 'pteaddr': { const x = e as Extract<MemEvent, { kind: 'PteAddr' }>
      return `${c}PTW L${x.level}: PTE address = ${x.level ? 'satp.PPN' : 'PTE.PPN'} × 4096 (${fh(x.base, 5)}) + VPN[${x.level}] × 4 (${fh(x.vpn, 3)} × 4) = ${fh(x.addr, 5)}` }
    case 'pteread': { const x = e as Extract<MemEvent, { kind: 'Walk' }>
      return `${c}PTW L${x.level}: read ${fh(x.pte_addr, 5)} through L1D${p.read ? ` — ${OUT[p.read.outcome]}, ${p.read.latency} cyc` : ''} → PTE ${fh(x.pte, 8)}` }
    case 'ptecheck': { const x = e as Extract<MemEvent, { kind: 'PteCheck' }>
      return `${c}PTW L${x.level}: PTE ${fh(x.pte, 8)}: ${VERDICT[x.verdict]}` }
    case 'ad': { const x = e as Extract<MemEvent, { kind: 'AdUpdate' }>
      return `${c}A/D update: PTE @${fh(x.pte_addr, 5)} ${fh(x.old, 8)} → ${fh(x.new, 8)} (written through L1D)` }
    case 'victim': { const x = e as Extract<MemEvent, { kind: 'TlbVictim' }>
      return x.was_invalid ? `${c}${tlb} victim: entry ${x.idx} is free (first invalid)`
        : `${c}${tlb} full → PLRU tree ${x.plru_before.toString(2).padStart(3, '0')} points at entry ${x.idx}` }
    case 'fill': { const x = e as Extract<MemEvent, { kind: 'TlbFill' }>
      return `${c}UPDATE: ${tlb}[${x.idx}] ← VPN ${fh(x.va >>> 12, 5)} → PPN ${fh(x.pa >>> 12, 5)}; PA ${fh(x.pa, 8)} → tag to L1${p.side}` }
    case 'fault': { const x = e as Extract<MemEvent, { kind: 'PageFault' }>
      return `${c}PAGE FAULT (cause ${x.cause}) → trap: mcause = ${x.cause}, mepc = the instruction, mtval = VA ${fh(x.va, 8)}, priv → M, pc → mtvec` }
    case 'sfence-gates': { const x = e as Extract<MemEvent, { kind: 'Sfence' }>
      return `C${p.core}: sfence.vma ${x.va === null ? 'x0 (all addresses)' : `VA ${fh(x.va, 8)}`}, ${x.asid === null ? 'x0 (all ASIDs, global too)' : `ASID ${x.asid} (global kept)`} — every entry: valid ∧ VA match ∧ ASID match ∧ ¬(rs2≠x0 ∧ G)` }
    case 'sfence-inval': { const x = e as Extract<MemEvent, { kind: 'Sfence' }>
      const l = (f: EntryFlush[]) => f.map((q, i) => q.inval ? i : -1).filter(i => i >= 0).join(',') || 'none'
      return `C${p.core}: invalidated ITLB {${l(x.itlb)}} · DTLB {${l(x.dtlb)}}` }
  }
}

/** CSRs the CPU block shows (priv is the mode, not a CSR, but changes on traps too) */
export const CSR_NAMES = ['priv_mode', 'satp', 'mstatus', 'mtvec', 'mepc', 'mcause', 'mtval'] as const
export type CsrName = typeof CSR_NAMES[number]
export interface MpLine { state: Mesi; tag: number; lru: number; addr: number; data: number[] }
export interface MpCache { sets: number; ways: number; lines: MpLine[] }
export interface MpCaches { l1i: MpCache[]; l1d: MpCache[]; l2: MpCache }
export interface MpStats {
  tlb_hits: number; tlb_misses: number; walks: number; page_faults: number
  l1i_hits: number; l1i_misses: number; l1d_hits: number; l1d_misses: number
  l2_hits: number; l2_misses: number; rom_reads: number; accesses: number; total_latency: number
  stall_cycles: number; bus_wait_cycles: number
}

interface Panel {
  examples(): { name: string; about: string }[]
  load_example(i: number): boolean
  step(): MemEvent[]
  readonly cycle: number
  cores(): MpCore[]; caches(): MpCaches; stats(): MpStats[]
  mem_words(pa: number, n: number): Uint32Array
  mem_raw(pa: number, n: number): Uint32Array
}
interface Disasm { disasm(w: number): { mnemonic: string; operands: string } | null }

/** Highlight kinds a cache line / set can get */
export type LineMark = 'hit' | 'fill-l2' | 'fill-mem' | 'evict' | 'snoop' | 'state' | 'writeback'

/** One side's translation this cycle: I = fetch, D = load / store */
export interface Xlat { va: number; pa: number | null; how: 'bare' | 'hit' | 'walk' | 'fault'; port: Port; cause?: number }

/** One TLB's part in an access: entry hit, entry filled by the walker, missed */
export interface TlbUse { hit: number | null; fill: number | null; miss: boolean }

/** What one core's last access touched */
export interface CoreView {
  cycle: number
  events: MemEvent[]
  /** "L1I:0:set:way" / "L1D:1:set:way" / "L2:set:way" → mark */
  lines: Map<string, LineMark>
  /** "L1I:0:set" / "L2:set" → miss */
  missSets: Set<string>
  /** Path per side: fetch / data (incl. walk) */
  fetch: Outcome | null
  data: Outcome | null
  /** ITLB (I) / DTLB (D) entries hit / filled this access */
  tlb: { I: TlbUse; D: TlbUse }
  walks: { side: 'I' | 'D'; level: number; pte_addr: number; pte: number }[]
  fault: boolean
  xlat: { I: Xlat | null; D: Xlat | null }
  snoop: boolean
  busWait: number
  /** Privilege mode the access ran in (before any trap it caused) */
  priv: number
  /** L1 set used by the fetch / data side (= VA[5:4], the VIPT index) */
  iSet: number | null
  dSet: number | null
}

const emptyView = (): CoreView => ({
  cycle: -1, events: [], lines: new Map(), missSets: new Set(), fetch: null, data: null,
  tlb: { I: { hit: null, fill: null, miss: false }, D: { hit: null, fill: null, miss: false } }, walks: [], fault: false, xlat: { I: null, D: null },
  snoop: false, busWait: 0, priv: 3, iSet: null, dSet: null,
})

const hex = (v: number, w = 8) => (v >>> 0).toString(16).padStart(w, '0')

const side = (p: Port): 'I' | 'D' => p === 'Fetch' ? 'I' : 'D'

/** Build a core's view from the events of the cycle it accessed memory in */
function viewOf(core: number, cycle: number, evs: MemEvent[], priv: number): CoreView {
  const v = emptyView()
  v.cycle = cycle
  v.priv = priv
  v.events = evs
  const key = (c: CacheId, owner: number, set: number, way: number) => c === 'L2' ? `L2:${set}:${way}` : `${c}:${owner}:${set}:${way}`
  const outcomeOf = (port: Port) => evs.find((e): e is Extract<MemEvent, { kind: 'Access' }> => e.kind === 'Access' && e.core === core && e.port === port)?.outcome ?? null
  for (const e of evs) {
    switch (e.kind) {
      case 'L1Hit': v.lines.set(key(e.cache, e.core, e.set, e.way), 'hit'); if (e.cache === 'L1I') v.iSet = e.set; else v.dSet = e.set; break
      case 'L1Miss': v.missSets.add(`${e.cache}:${e.core}:${e.set}`); if (e.cache === 'L1I') v.iSet = e.set; else v.dSet = e.set; break
      case 'L2Hit': v.lines.set(key('L2', 0, e.set, e.way), 'hit'); break
      case 'L2Miss': v.missSets.add(`L2:${e.set}`); break
      case 'Fill': v.lines.set(key(e.cache, e.core, e.set, e.way), e.cache === 'L2' ? 'fill-mem' : 'fill-l2'); break
      case 'Evict': if (!v.lines.has(key(e.cache, e.core, e.set, e.way))) v.lines.set(key(e.cache, e.core, e.set, e.way), 'evict'); break
      case 'WriteBack': v.lines.set(key('L1D', e.core, e.set, e.way), 'writeback'); break
      case 'StateChange': if (!v.lines.has(key(e.cache, e.core, e.set, e.way))) v.lines.set(key(e.cache, e.core, e.set, e.way), 'state'); break
      case 'Snoop': v.snoop = true; v.lines.set(key(e.cache, e.to, e.set, e.way), 'snoop'); break
      case 'TlbHit': v.tlb[side(e.port)].hit = e.idx; v.xlat[side(e.port)] = { va: e.va, pa: e.pa, how: 'hit', port: e.port }; break
      case 'TlbMiss': v.tlb[side(e.port)].miss = true; v.xlat[side(e.port)] = { va: e.va, pa: null, how: 'walk', port: e.port }; break
      case 'Walk': v.walks.push({ side: side(e.port), level: e.level, pte_addr: e.pte_addr, pte: e.pte }); break
      case 'TlbFill': v.tlb[side(e.port)].fill = e.idx; v.xlat[side(e.port)] = { va: e.va, pa: e.pa, how: 'walk', port: e.port }; break
      case 'PageFault': v.fault = true; v.xlat[side(e.port)] = { va: e.va, pa: null, how: 'fault', port: e.port, cause: e.cause }; break
      case 'Bare': v.xlat[side(e.port)] = { va: e.va, pa: e.va, how: 'bare', port: e.port }; break
      case 'BusWait': v.busWait += e.cycles; break
    }
  }
  // An L1 fill after a total miss came from memory
  const memFill = evs.some(e => e.kind === 'MemRead')
  if (memFill) for (const [k, m] of v.lines) if (m === 'fill-l2' && !k.startsWith('L2')) v.lines.set(k, 'fill-mem')
  v.fetch = outcomeOf('Fetch')
  const order: Outcome[] = ['L1Hit', 'L2Hit', 'Memory']
  const ds = [outcomeOf('Load'), outcomeOf('Store'), outcomeOf('Walk')].filter((o): o is Outcome => !!o)
  v.data = ds.length ? ds.reduce((a, b) => order.indexOf(b) > order.indexOf(a) ? b : a) : null
  return v
}

/** One line of the event log */
export function describe(e: MemEvent): string {
  const c = 'core' in e ? `C${e.core} ` : ''
  switch (e.kind) {
    case 'Bare': return `${c}${side(e.port)} bare (no translation) VA ${hex(e.va)}`
    case 'TlbLookup': return `${c}${side(e.port)}TLB lookup ${hex(e.va)} ASID ${e.asid}: ${e.cmp.map(q => q.hit ? 1 : 0).join('')}`
    case 'PermCheck': return `${c}${side(e.port)} permission ${e.ok ? 'ok' : 'DENIED'}`
    case 'PteAddr': return `${c}PTW L${e.level} address ${hex(e.addr)}`
    case 'PteCheck': return `${c}PTW L${e.level} PTE ${hex(e.pte)}: ${e.verdict}`
    case 'AdUpdate': return `${c}A/D update @${hex(e.pte_addr)} ${hex(e.old)} → ${hex(e.new)}`
    case 'TlbVictim': return `${c}${side(e.port)}TLB victim [${e.idx}]${e.was_invalid ? ' (free)' : ' (PLRU)'}`
    case 'Sfence': return `${c}sfence.vma ${e.va === null ? 'x0' : hex(e.va)}, ${e.asid === null ? 'x0' : 'ASID ' + e.asid}`
    case 'TlbHit': return `${c}${side(e.port)}TLB hit [${e.idx}] ${hex(e.va)} → ${hex(e.pa)}`
    case 'TlbMiss': return `${c}${side(e.port)}TLB miss ${hex(e.va)} — page walk`
    case 'Walk': return `${c}PTW (${side(e.port)}) L${e.level}: PTE @${hex(e.pte_addr)} = ${hex(e.pte)}`
    case 'TlbFill': return `${c}PTW update ${side(e.port)}TLB [${e.idx}] ${hex(e.va)} → ${hex(e.pa)}`
    case 'PageFault': return `${c}${side(e.port)} PAGE FAULT VA ${hex(e.va)} (cause ${e.cause})`
    case 'Access': return `${c}${e.port} ${hex(e.pa)}: ${e.outcome === 'L1Hit' ? 'L1 hit' : e.outcome === 'L2Hit' ? 'L1 miss, L2 hit' : e.outcome === 'Rom' ? 'boot ROM, uncached' : 'total miss (memory)'} — ${e.latency} cyc`
    case 'RomRead': return `${c}ROM read ${hex(e.pa)} = ${hex(e.val)} (uncached, over the bus)`
    case 'L1Hit': return `${c}${e.cache} hit set ${e.set} way ${e.way}`
    case 'L1Miss': return `${c}${e.cache} miss set ${e.set}`
    case 'L2Hit': return `${c}L2 hit set ${e.set} way ${e.way}`
    case 'L2Miss': return `${c}L2 miss set ${e.set}`
    case 'MemRead': return `${c}memory read line ${hex(e.pa)}`
    case 'MemWrite': return `memory write-back line ${hex(e.pa)}`
    case 'Fill': return `${c}${e.cache} fill set ${e.set} way ${e.way} ← ${hex(e.pa)} (${e.state})`
    case 'Evict': return `${c}${e.cache} evict set ${e.set} way ${e.way} (${hex(e.pa)}${e.dirty ? ', dirty' : ''})`
    case 'WriteBack': return `${c}L1D write-back ${hex(e.pa)} → L2`
    case 'StateChange': return `${c}${e.cache} ${hex(e.pa)} ${e.from} → ${e.to}`
    case 'Snoop': return `snoop C${e.from}→C${e.to} ${e.req} ${hex(e.pa)}: ${e.cache} ${e.before} → ${e.after}${e.flush ? ' (flush)' : ''}`
    case 'BusWait': return `${c}bus busy — waits ${e.cycles} cyc`
    case 'Stall': return `${c}waiting for memory (${e.left})`
  }
}

export const useMemPanel = defineStore('memPanel', () => {
  let p: Panel | null = null
  let dis: Disasm | null = null
  const ready = ref(false)
  const error = ref<string | null>(null)
  const examples = ref<{ name: string; about: string }[]>([])
  const exampleIndex = ref(0)
  const tick = ref(0)
  const views = shallowRef<CoreView[]>([emptyView(), emptyView()])
  const log = shallowRef<{ cycle: number; text: string; core: number | null }[]>([])
  /** Per core: CSRs changed by the last Step / Next access */
  const csrChanged = shallowRef<Set<CsrName>[]>([new Set(), new Set()])
  /** Per core: the MMU work of its last cycle with any (sticky) */
  const mmu = shallowRef<CoreMmu[]>([{ cycle: -1, groups: [], before: null }, { cycle: -1, groups: [], before: null }])
  /** This cycle's phases (in order) and the one being shown */
  const phases = shallowRef<Phase[]>([])
  const phaseIdx = ref(-1)
  const phaseCycle = ref(-1)
  /** Step only misses, faults and flushes (not TLB hits / bare) */
  const skipHits = ref(true)
  const playing = ref(false)
  /** Recent boot-ROM reads, newest first */
  const romLog = shallowRef<{ cycle: number; core: number; pa: number; val: number }[]>([])
  const running = ref(false)
  const speed = ref(150)
  let loading: Promise<void> | null = null

  function init(): Promise<void> {
    if (p) return Promise.resolve()
    if (!loading) loading = (async () => {
      for (let attempt = 1; ; attempt++) {
        try {
          const mod = await import('../wasm/cpu_sim.js')
          await mod.default()
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const m = mod as any
          p = new m.JsMemPanel() as Panel
          dis = new m.JsCpuState() as Disasm
          examples.value = p.examples()
          error.value = null
          ready.value = true
          loadExample(exampleIndex.value)
          return
        } catch (e) {
          error.value = String(e)
          if (attempt >= 5) { loading = null; throw e }
          await new Promise(r => setTimeout(r, 1000 * attempt))
        }
      }
    })()
    return loading
  }

  function loadExample(i: number) {
    if (!p) return
    running.value = false
    exampleIndex.value = i
    p.load_example(i)
    views.value = [emptyView(), emptyView()]
    csrChanged.value = [new Set(), new Set()]
    mmu.value = [{ cycle: -1, groups: [], before: null }, { cycle: -1, groups: [], before: null }]
    romLog.value = []
    phases.value = []
    phaseIdx.value = -1
    playing.value = false
    log.value = []
    tick.value++
  }

  /** Run `fn` and record which CSRs it changed on each core */
  function trackCsrs<T>(fn: () => T): T {
    const before = p!.cores()
    const r = fn()
    const after = p!.cores()
    csrChanged.value = after.map((a, i) => new Set(CSR_NAMES.filter(n => a[n] !== before[i]![n])))
    return r
  }

  /** One clock (the Step button). */
  function step(): boolean {
    if (!p) return false
    return trackCsrs(clock)
  }

  /** One clock. Returns true if any core touched memory (not just waited). */
  function clock(): boolean {
    if (!p) return false
    const cycle = p.cycle
    const before = p.cores()
    const priv = before.map(c => c.priv_mode)
    const evs = p.step()
    // serde_wasm_bindgen sends Rust `None` as undefined: make it null
    for (const e of evs) {
      if (e.kind === 'TlbLookup') e.hit ??= null
      else if (e.kind === 'Sfence') { e.va ??= null; e.asid ??= null }
    }
    const roms = evs.filter((e): e is Extract<MemEvent, { kind: 'RomRead' }> => e.kind === 'RomRead')
    if (roms.length) romLog.value = [...roms.map(e => ({ cycle, core: e.core, pa: e.pa, val: e.val })).reverse(), ...romLog.value].slice(0, 10)
    // MMU phases of this cycle
    const gs = groupsOf(evs)
    const nextMmu = [...mmu.value]
    const list: Phase[] = []
    gs.forEach((g, k) => {
      if (!g.length) return
      nextMmu[k] = { cycle, groups: g, before: before[k]! }
      for (const grp of g) {
        if (skipHits.value && !grp.sfence && (grp.hit || grp.phases[0]?.kind === 'bare') && !grp.phases.some(x => x.kind === 'fault' || x.kind === 'ad')) continue
        for (const x of grp.phases) { x.gi = list.length; list.push(x) }
      }
    })
    mmu.value = nextMmu
    phases.value = list
    phaseIdx.value = list.length - 1
    phaseCycle.value = cycle
    const next = [...views.value]
    let touched = false
    for (const core of [0, 1]) {
      const mine = evs.filter(e => ('core' in e && e.core === core && e.kind !== 'Stall') || (e.kind === 'Snoop' && e.from === core))
      if (mine.length) {
        touched = true
        // memory write-backs of this cycle belong to whoever caused them
        next[core] = viewOf(core, cycle, [...mine, ...evs.filter(e => e.kind === 'MemWrite')], priv[core]!)
      }
    }
    views.value = next
    const lines = evs.filter(e => e.kind !== 'Stall' && e.kind !== 'Bare' && !(e.kind === 'Access' && e.port === 'Fetch' && (e.outcome === 'L1Hit' || e.outcome === 'Rom')))
      .map(e => ({ cycle, text: describe(e), core: 'core' in e ? e.core : 'from' in e ? e.from : null }))
    if (lines.length) log.value = [...lines.reverse(), ...log.value].slice(0, 80)
    tick.value++
    return touched
  }

  /** Step until a core touches memory again (skips frozen cycles). */
  /** Show the next MMU phase; at the end of a cycle's phases, run to the next cycle that has any */
  function nextPhase() {
    if (!p) return
    if (phaseIdx.value < phases.value.length - 1) { phaseIdx.value++; tick.value++; return }
    trackCsrs(() => { for (let i = 0; i < 2000; i++) { clock(); if (phases.value.length) break } })
    phaseIdx.value = phases.value.length ? 0 : -1
    tick.value++
  }

  async function togglePlay() {
    if (playing.value) { playing.value = false; return }
    playing.value = true
    while (playing.value) {
      nextPhase()
      await new Promise(r => setTimeout(r, Math.max(400, speed.value * 4)))
    }
  }

  function nextAccess() {
    if (!p) return
    trackCsrs(() => { for (let i = 0; i < 200; i++) if (clock()) break })
  }

  async function toggleRun() {
    if (running.value) { running.value = false; return }
    running.value = true
    while (running.value) {
      nextAccess()
      await new Promise(r => setTimeout(r, speed.value))
    }
  }

  function asm(w: number): string {
    const d = dis?.disasm(w >>> 0)
    return d ? `${d.mnemonic} ${d.operands}`.trim().toLowerCase() : ''
  }

  const snap = computed(() => {
    void tick.value
    if (!p) return null
    return { cycle: p.cycle, cores: p.cores(), caches: p.caches(), stats: p.stats() }
  })

  function memWords(pa: number, n: number) { return p ? { cached: Array.from(p.mem_words(pa, n)), raw: Array.from(p.mem_raw(pa, n)) } : { cached: [], raw: [] } }

  return {
    ready, error, examples, exampleIndex, views, log, running, speed, snap, tick, csrChanged,
    mmu, phases, phaseIdx, romLog, phaseCycle, skipHits, playing, nextPhase, togglePlay,
    init, loadExample, step, nextAccess, toggleRun, asm, memWords, hex,
  }
})
