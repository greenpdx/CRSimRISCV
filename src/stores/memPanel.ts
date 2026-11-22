// ─────────────────────────────────────────────────────────────────────────────
// Memory panel — two pipeline cores on the shared cache hierarchy, all in
// Rust (sim/src/sim/memsys, wasm MemPanel). This store only steps the
// machine and turns its per-cycle events into what the panel highlights.
//
// A core's highlights stay on its last access until it makes the next one,
// so a miss stays visible while the core is frozen waiting for memory.
// ─────────────────────────────────────────────────────────────────────────────

import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'

// Event / snapshot types generated from the Rust structs (sim/src/sim/memsys).
import type {
  MemPanel, Event, Mesi, CacheId, Port, Outcome, WayCmp, EntryCmp, EntryFlush, PteVerdict,
  MpStage, MpTlb, MpCore, MpLine, MpCache, MpCaches, Stats,
} from '../wasm/cpu_sim'
export type {
  Mesi, CacheId, Port, Outcome, WayCmp, EntryCmp, EntryFlush, PteVerdict, MpTlb, MpCore, MpLine, MpCache, MpCaches,
}
/** One event from the Rust memory system (serde tag = kind) */
export type MemEvent = Event
export type LookupEv = Extract<MemEvent, { kind: 'Lookup' }>
export type McStage = MpStage
export type MpStats = Stats

// ── MMU phases: what the hardware does in one access, one step at a time ──
export type Side = 'I' | 'D'
export type PhaseKind = 'bare' | 'split' | 'compare' | 'perm' | 'hit' | 'miss' | 'pteaddr' | 'pteread' | 'ptecheck'
  | 'ad' | 'victim' | 'fill' | 'fault' | 'sfence-gates' | 'sfence-inval'
  // cache lookups: address split, set select, tag compare, then what came of it
  | 'c-split' | 'c-set' | 'c-cmp' | 'c-hit' | 'c-miss' | 'c-snoop' | 'c-wb' | 'c-evict' | 'c-memrd' | 'c-memwr' | 'c-fill' | 'c-state'
type AccessEv = Extract<MemEvent, { kind: 'Access' }>
export interface Phase {
  /** Index in this cycle's phase list (−1: not stepped, e.g. a skipped TLB hit) */
  gi: number
  core: number
  /** TLB side; null = both (SFENCE.VMA) */
  side: Side | null
  kind: PhaseKind
  ev: MemEvent
  /** Index of `ev` in the cycle's event list (phases are revealed in event order) */
  ei: number
  /** Cache phases: which cache */
  cache?: CacheId
  /** The L1D access of a PTE read (pteread) */
  read?: AccessEv
}
/** One translation, one SFENCE.VMA, or one cache access (L1 → L2 → memory) of one core in one cycle */
export interface Group { core: number; side: Side | null; sfence: boolean; hit: boolean; cache?: CacheId; phases: Phase[] }
/** A core's MMU work in the last cycle it did any, and its state before that cycle */
export interface CoreMmu { cycle: number; groups: Group[]; before: MpCore | null }

const MMU_KINDS = new Set(['Bare', 'TlbLookup', 'PermCheck', 'TlbHit', 'TlbMiss', 'PteAddr', 'Walk', 'PteCheck', 'AdUpdate', 'TlbVictim', 'TlbFill', 'PageFault', 'Sfence'])

const CACHE_KINDS = new Set(['Lookup', 'L1Hit', 'L1Miss', 'L2Hit', 'L2Miss', 'Snoop', 'WriteBack', 'Evict', 'MemRead', 'MemWrite', 'Fill', 'StateChange'])

/**
 * Split a cycle's events into groups of phases, in event order: MMU groups
 * (a translation or an SFENCE.VMA) and cache groups (one L1 access and what
 * follows: L2, memory, fills). A page walk's PTE reads go into its MMU group.
 */
function groupsOf(evs: MemEvent[]): Group[] {
  const out: Group[] = []
  const mmuCur: (Group | null)[] = [null, null]
  const cacheCur: (Group | null)[] = [null, null]
  const walkOpen = [false, false]
  const pendingRead: (AccessEv | undefined)[] = [undefined, undefined]
  let lastCore = 0
  evs.forEach((e, ei) => {
    if (e.kind === 'Access' && e.port === 'Walk') { pendingRead[e.core] = e; return }
    if (MMU_KINDS.has(e.kind) && 'core' in e) {
      const k = e.core
      const sd: Side | null = 'port' in e ? (e.port === 'Fetch' ? 'I' : 'D') : null
      if (e.kind === 'TlbLookup' || e.kind === 'Bare' || e.kind === 'Sfence' || !mmuCur[k]) {
        mmuCur[k] = { core: k, side: sd, sfence: e.kind === 'Sfence', hit: e.kind === 'TlbLookup' && e.hit !== null, phases: [] }
        out.push(mmuCur[k]!)
      }
      const g = mmuCur[k]!
      const ph = (kind: PhaseKind, extra: Partial<Phase> = {}) => g.phases.push({ gi: -1, ei, core: k, side: g.side, kind, ev: e, ...extra })
      switch (e.kind) {
        case 'Bare': ph('bare'); break
        case 'TlbLookup': ph('split'); ph('compare'); break
        case 'PermCheck': ph('perm'); break
        case 'TlbHit': ph('hit'); break
        case 'TlbMiss': ph('miss'); break
        case 'PteAddr': ph('pteaddr'); walkOpen[k] = true; break
        case 'Walk': ph('pteread', { read: pendingRead[k] }); pendingRead[k] = undefined; walkOpen[k] = false; break
        case 'PteCheck': ph('ptecheck'); break
        case 'AdUpdate': ph('ad'); break
        case 'TlbVictim': ph('victim'); break
        case 'TlbFill': ph('fill'); break
        case 'PageFault': ph('fault'); break
        case 'Sfence': ph('sfence-gates'); ph('sfence-inval'); break
      }
      return
    }
    if (!CACHE_KINDS.has(e.kind)) return
    const k = 'core' in e ? e.core : e.kind === 'Snoop' ? e.from : lastCore
    lastCore = k
    let g: Group
    if (walkOpen[k] && mmuCur[k]) g = mmuCur[k]!                       // a PTE read inside a walk
    else {
      if (e.kind === 'Lookup' && e.cache !== 'L2' || !cacheCur[k]) {
        const c = e.kind === 'Lookup' ? e.cache : 'L1D'
        cacheCur[k] = { core: k, side: c === 'L1I' ? 'I' : 'D', sfence: false, hit: false, cache: c, phases: [] }
        out.push(cacheCur[k]!)
      }
      g = cacheCur[k]!
    }
    const cacheOf = 'cache' in e ? e.cache : e.kind === 'WriteBack' ? 'L1D' : e.kind === 'L2Hit' || e.kind === 'L2Miss' || e.kind === 'MemRead' || e.kind === 'MemWrite' ? 'L2' : undefined
    const ph = (kind: PhaseKind) => g.phases.push({ gi: -1, ei, core: k, side: g.side, kind, ev: e, cache: cacheOf })
    switch (e.kind) {
      case 'Lookup': ph('c-split'); ph('c-set'); ph('c-cmp'); break
      case 'L1Hit': if (g.cache && g.phases.length === 3) g.hit = true; ph('c-hit'); break
      case 'L2Hit': ph('c-hit'); break
      case 'L1Miss': case 'L2Miss': ph('c-miss'); break
      case 'Snoop': ph('c-snoop'); break
      case 'WriteBack': ph('c-wb'); break
      case 'Evict': ph('c-evict'); break
      case 'MemRead': ph('c-memrd'); break
      case 'MemWrite': ph('c-memwr'); break
      case 'Fill': ph('c-fill'); break
      case 'StateChange': ph('c-state'); break
    }
  })
  return out
}

/** Does event `e` change the contents of cache `id` ("L1I:0", "L1D:1", "L2")? */
export function changesCache(e: MemEvent, id: string): boolean {
  switch (e.kind) {
    case 'Fill': case 'Evict': case 'StateChange': return id === (e.cache === 'L2' ? 'L2' : `${e.cache}:${e.core}`)
    case 'WriteBack': return id === `L1D:${e.core}` || id === 'L2'
    case 'Snoop': return id === `${e.cache}:${e.to}` || id === 'L2'
    default: return false
  }
}

/** Events of `core` in a cycle (for its view) */
export function mineOf(core: number, evs: MemEvent[]): MemEvent[] {
  const mine = evs.filter(e => ('core' in e && e.core === core && e.kind !== 'Stall') || (e.kind === 'Snoop' && e.from === core))
  return mine.length ? [...mine, ...evs.filter(e => e.kind === 'MemWrite')] : []
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
    case 'c-split': { const x = e as LookupEv
      const lo = x.cache === 'L2' ? 7 : 6
      return `C${p.core} ${x.cache}: PA ${fh(x.pa, 8)} → tag PA[31:${lo}] = ${x.tag.toString(16)} · index PA[${lo - 1}:4] = ${x.set} · offset PA[3:0] = ${x.pa & 0xf}${x.cache === 'L2' ? ' (address from the bus)' : ' (index straight from the VA, tag from the MMU)'}` }
    case 'c-set': { const x = e as LookupEv
      return `C${p.core} ${x.cache}: index ${x.set} selects set ${x.set} — its ${x.ways.length} ways are read out (V, tag, data) to the comparators` }
    case 'c-cmp': { const x = e as LookupEv
      return `C${p.core} ${x.cache}: compare tag ${x.tag.toString(16)} with each way (valid ∧ tag =): ${x.ways.map((w, i) => `w${i} ${w.valid ? (w.hit ? '✓' : '✗') : 'V=0'}`).join(' · ')}` }
    case 'c-hit': { const x = e as Extract<MemEvent, { kind: 'L1Hit' | 'L2Hit' }>
      return `C${p.core} ${p.cache}: HIT way ${x.way} → the offset picks word ${(x.pa >> 2) & 3} of the line` }
    case 'c-miss': return `C${p.core} ${p.cache}: MISS → ${p.cache === 'L2' ? 'read the line from memory' : 'bus request to L2'}`
    case 'c-snoop': case 'c-wb': case 'c-evict': case 'c-memrd': case 'c-memwr': case 'c-fill': case 'c-state':
      return describe(e)
    case 'sfence-inval': { const x = e as Extract<MemEvent, { kind: 'Sfence' }>
      const l = (f: EntryFlush[]) => f.map((q, i) => q.inval ? i : -1).filter(i => i >= 0).join(',') || 'none'
      return `C${p.core}: invalidated ITLB {${l(x.itlb)}} · DTLB {${l(x.dtlb)}}` }
  }
}

/** CSRs the CPU block shows (priv is the mode, not a CSR, but changes on traps too) */
export const CSR_NAMES = ['priv_mode', 'satp', 'mstatus', 'mtvec', 'mepc', 'mcause', 'mtval'] as const
export type CsrName = typeof CSR_NAMES[number]
type Panel = MemPanel
type Disasm = (w: number) => { mnemonic: string; operands: string } | null

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
  /** Last tag lookup per cache: "L1I:core" / "L1D:core" / "L2" */
  lookups: Record<string, LookupEv>
  /** Privilege mode the access ran in (before any trap it caused) */
  priv: number
  /** L1 set used by the fetch / data side (= VA[5:4], the VIPT index) */
  iSet: number | null
  dSet: number | null
}

const emptyView = (): CoreView => ({
  cycle: -1, events: [], lines: new Map(), missSets: new Set(), fetch: null, data: null,
  tlb: { I: { hit: null, fill: null, miss: false }, D: { hit: null, fill: null, miss: false } }, walks: [], fault: false, xlat: { I: null, D: null },
  snoop: false, busWait: 0, priv: 3, lookups: {}, iSet: null, dSet: null,
})

const hex = (v: number, w = 8) => (v >>> 0).toString(16).padStart(w, '0')

const side = (p: Port): 'I' | 'D' => p === 'Fetch' ? 'I' : 'D'

/** Build a core's view from the events of the cycle it accessed memory in */
export function viewOf(core: number, cycle: number, evs: MemEvent[], priv: number): CoreView {
  const v = emptyView()
  v.cycle = cycle
  v.priv = priv
  v.events = evs
  const key = (c: CacheId, owner: number, set: number, way: number) => c === 'L2' ? `L2:${set}:${way}` : `${c}:${owner}:${set}:${way}`
  const outcomeOf = (port: Port) => evs.find((e): e is Extract<MemEvent, { kind: 'Access' }> => e.kind === 'Access' && e.core === core && e.port === port)?.outcome ?? null
  for (const e of evs) {
    switch (e.kind) {
      case 'Lookup': v.lookups[e.cache === 'L2' ? 'L2' : `${e.cache}:${e.core}`] = e; break
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
    case 'Lookup': return `${c}${e.cache} lookup ${hex(e.pa)}: set ${e.set}, tag ${e.tag.toString(16)}`
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
  /** This cycle: all its groups, all its events, the caches before it, each core's mode before it */
  const cycleGroups = shallowRef<Group[]>([])
  const cycleEvents = shallowRef<MemEvent[]>([])
  const cachesBefore = shallowRef<MpCaches | null>(null)
  const privAt = shallowRef<number[]>([3, 3])
  /** Events up to (and including) this index are revealed; Infinity = all */
  const cutoff = computed(() => phaseIdx.value >= 0 && phaseIdx.value < phases.value.length - 1 ? phases.value[phaseIdx.value]!.ei : Infinity)
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
          p = new mod.MemPanel()
          dis = mod.disasm
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
    cycleGroups.value = []
    cycleEvents.value = []
    cachesBefore.value = null
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
    const cachesPre = p.caches()
    const priv = before.map(c => c.priv_mode)
    const evs = p.step()
    // serde_wasm_bindgen sends Rust `None` as undefined: make it null
    for (const e of evs) {
      if (e.kind === 'TlbLookup') e.hit ??= null
      else if (e.kind === 'Sfence') { e.va ??= null; e.asid ??= null }
    }
    const roms = evs.filter((e): e is Extract<MemEvent, { kind: 'RomRead' }> => e.kind === 'RomRead')
    if (roms.length) romLog.value = [...roms.map(e => ({ cycle, core: e.core, pa: e.pa, val: e.val })).reverse(), ...romLog.value].slice(0, 10)
    // Phases of this cycle: MMU groups and cache groups, in event order
    const gs = groupsOf(evs)
    const nextMmu = [...mmu.value]
    for (const k of [0, 1]) {
      const mg = gs.filter(g => g.core === k && !g.cache)
      if (mg.length) nextMmu[k] = { cycle, groups: mg, before: before[k]! }
    }
    const list = gs
      .filter(g => !(skipHits.value && !g.sfence && (g.hit || g.phases[0]?.kind === 'bare') && !g.phases.some(x => x.kind === 'fault' || x.kind === 'ad')))
      .flatMap(g => g.phases)
      .sort((a, b) => a.ei - b.ei)
    list.forEach((x, i) => { x.gi = i })
    mmu.value = nextMmu
    cycleGroups.value = gs
    cycleEvents.value = evs
    cachesBefore.value = cachesPre
    phases.value = list
    phaseIdx.value = list.length - 1
    phaseCycle.value = cycle
    const next = [...views.value]
    let touched = false
    for (const core of [0, 1]) {
      // memory write-backs of this cycle belong to whoever caused them
      const mine = mineOf(core, evs)
      if (mine.length) {
        touched = true
        next[core] = viewOf(core, cycle, mine, priv[core]!)
      }
    }
    views.value = next
    privAt.value = priv
    const lines = evs.filter(e => e.kind !== 'Stall' && e.kind !== 'Bare' && e.kind !== 'Lookup' && !(e.kind === 'Access' && e.port === 'Fetch' && (e.outcome === 'L1Hit' || e.outcome === 'Rom')))
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
    const d = dis?.(w >>> 0)
    return d ? `${d.mnemonic} ${d.operands}`.trim().toLowerCase() : ''
  }

  const snap = computed(() => {
    void tick.value
    if (!p) return null
    return { cycle: p.cycle, cores: p.cores(), caches: p.caches(), stats: p.stats() }
  })

  function memWords(pa: number, n: number) { return p ? { cached: Array.from(p.mem(pa, n)), raw: Array.from(p.mem_raw(pa, n)) } : { cached: [], raw: [] } }

  return {
    ready, error, examples, exampleIndex, views, log, running, speed, snap, tick, csrChanged,
    mmu, phases, phaseIdx, romLog, cycleGroups, cycleEvents, cachesBefore, privAt, cutoff, phaseCycle, skipHits, playing, nextPhase, togglePlay,
    init, loadExample, step, nextAccess, toggleRun, asm, memWords, hex,
  }
})
