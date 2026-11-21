<template>
  <div class="ptw" :class="{ 'ptw--focus': focus, 'ptw--fault': has('fault') }">
    <div class="ptw__head">
      <span>PAGE TABLE WALKER</span>
      <span class="ptw__sub">{{ group && walking ? `walking for ${group.side === 'I' ? 'ITLB (fetch)' : 'DTLB (' + port + ')'}` : 'satp root ' + fh(root, 5) }}</span>
    </div>

    <!-- state machine (CVA6 Fig 20, Sv32 levels) -->
    <svg class="ptw__fsm" width="456" height="46">
      <defs><marker id="ptwah" viewBox="0 0 6 6" refX="5" refY="3" markerWidth="5" markerHeight="5" orient="auto"><path d="M0,0 L6,3 L0,6 z" fill="#94a3b8" /></marker></defs>
      <path v-for="(a, i) in ARROWS" :key="i" :d="a" fill="none" stroke="#94a3b8" stroke-width="1.2" marker-end="url(#ptwah)" />
      <g v-for="s in STATES" :key="s.id">
        <rect :x="s.x" :y="s.y" :width="s.w" height="17" rx="8.5" :class="'st st--' + stState(s.id)" />
        <text :x="s.x + s.w / 2" :y="s.y + 12" text-anchor="middle" :class="'stt stt--' + stState(s.id)">{{ s.label }}</text>
      </g>
      <text x="300" y="44" class="ptw__note">{{ note }}</text>
    </svg>

    <!-- one block per level: address (Fig 21) · read through L1D · PTE decode -->
    <div v-for="lv in [1, 0]" :key="lv" class="lvl" :class="{ 'lvl--on': levelOn(lv), 'lvl--cur': levelCur(lv) }">
      <div class="lvl__name">L{{ lv }}</div>
      <div class="lvl__rows">
        <div class="row" :class="{ 'row--cur': isCur('pteaddr', lv) }">
          <template v-if="addr(lv)">
            <span class="op">{{ lv ? 'satp.PPN' : 'PTE.PPN' }}</span><span class="v v--ppn">{{ fh(addr(lv)!.base >>> 12, 5) }}</span><span class="op">≪12</span>
            <span class="op">+</span>
            <span class="op">VPN[{{ lv }}]</span><span class="v v--vpn">{{ fh(addr(lv)!.vpn, 3) }}</span><span class="op">≪2</span>
            <span class="op">=</span><span class="v v--addr">PTE @ {{ fh(addr(lv)!.addr, 5) }}</span>
          </template>
          <span v-else class="dim">PTE address = {{ lv ? 'satp.PPN' : 'PTE.PPN' }} × 4096 + VPN[{{ lv }}] × 4</span>
        </div>
        <div class="row" :class="{ 'row--cur': isCur('pteread', lv) }">
          <template v-if="read(lv)">
            <span class="op">read via L1D:</span>
            <span v-if="read(lv)!.read" class="out" :class="'out--' + read(lv)!.read!.outcome">{{ OUT[read(lv)!.read!.outcome] }} · {{ read(lv)!.read!.latency }} cyc</span>
            <span class="op">→</span><span class="v">{{ fh(readPte(lv), 8) }}</span>
          </template>
          <span v-else class="dim">read the PTE through the data cache</span>
        </div>
        <div class="row" :class="{ 'row--cur': isCur('ptecheck', lv) }">
          <template v-if="check(lv)">
            <span class="op">PPN</span><span class="v v--ppn">{{ fh(check(lv)!.pte >>> 10, 5) }}</span>
            <span class="bits"><span v-for="b in BITS" :key="b.n" :class="(check(lv)!.pte >> b.n) & 1 ? 'b1' : 'b0'" :title="b.t">{{ b.l }}</span></span>
            <span class="verdict" :class="'verdict--' + check(lv)!.verdict">{{ VERDICT[check(lv)!.verdict] }}</span>
          </template>
          <span v-else class="dim">check V, R/W/X: pointer, leaf or fault</span>
        </div>
      </div>
    </div>

    <!-- leaf: permission, A/D, victim, update / fault -->
    <div class="tail">
      <div class="row" :class="{ 'row--cur': cur?.kind === 'perm' }">
        <span class="lbl">perm</span>
        <template v-if="perm"><span>{{ permTxt }}</span><span :class="perm.ok ? 'ok' : 'bad'">{{ perm.ok ? '✓' : '✗ page fault' }}</span></template>
        <span v-else class="dim">R / W / X for the access, U with SUM, MXR</span>
      </div>
      <div v-if="ad" class="row" :class="{ 'row--cur': cur?.kind === 'ad' }">
        <span class="lbl">A/D</span><span>PTE @{{ fh(ad.pte_addr, 5) }} {{ fh(ad.old, 8) }} → {{ fh(ad.new, 8) }} (write through L1D)</span>
      </div>
      <div class="row" :class="{ 'row--cur': cur?.kind === 'victim' }">
        <span class="lbl">victim</span>
        <template v-if="victim">entry <b>{{ victim.idx }}</b> — {{ victim.was_invalid ? 'first invalid entry' : `all valid → PLRU tree ${b3(victim.plru_before)} points at it` }}</template>
        <span v-else class="dim">first invalid entry, else the PLRU tree</span>
      </div>
      <div class="row" :class="{ 'row--cur': cur?.kind === 'fill' || cur?.kind === 'fault' }">
        <span class="lbl">{{ fault ? 'FAULT' : 'update' }}</span>
        <template v-if="fault"><span class="bad">page fault, cause {{ fault.cause }} → trap: mcause, mepc, mtval = {{ fh(fault.va, 8) }}</span></template>
        <template v-else-if="fill">{{ group?.side }}TLB[{{ fill.idx }}] ← VPN {{ fh(fill.va >>> 12, 5) }} → PPN {{ fh(fill.pa >>> 12, 5) }} · PA <b>{{ fh(fill.pa, 8) }}</b></template>
        <span v-else class="dim">write the new entry, send the PA</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { Group, Phase, PhaseKind, MemEvent, Outcome, PteVerdict } from '../../stores/memPanel'

/**
 * The page-table walker of one core: state machine, then per level the
 * PTE address (CVA6 Fig 21), the read through L1D and the PTE decode,
 * then permission, A/D, victim and update. Rows fill in as the phases
 * are revealed (`shown`); the current phase is outlined.
 */
const props = defineProps<{
  group: Group | null
  shown: (p: Phase) => boolean
  cur: Phase | null
  root: number
  focus?: boolean
}>()

type Ev<K extends MemEvent['kind']> = Extract<MemEvent, { kind: K }>
const fh = (n: number, w: number) => (n >>> 0).toString(16).padStart(w, '0')
const b3 = (n: number) => n.toString(2).padStart(3, '0')
const OUT: Record<Outcome, string> = { L1Hit: 'L1D hit', L2Hit: 'L1D miss · L2 hit', Memory: 'L1D + L2 miss · memory', Rom: 'boot ROM' }
const VERDICT: Record<PteVerdict, string> = {
  Pointer: 'pointer → next level', Leaf: 'leaf ✓', Invalid: 'V=0 → fault', Reserved: 'W without R → fault',
  Misaligned: 'misaligned superpage → fault', NoLeaf: 'pointer at L0 → fault',
}
const BITS = [{ n: 7, l: 'D', t: 'dirty' }, { n: 6, l: 'A', t: 'accessed' }, { n: 5, l: 'G', t: 'global' }, { n: 4, l: 'U', t: 'user' },
  { n: 3, l: 'X', t: 'execute' }, { n: 2, l: 'W', t: 'write' }, { n: 1, l: 'R', t: 'read' }, { n: 0, l: 'V', t: 'valid' }]

const visible = computed(() => props.group?.phases.filter(p => props.shown(p)) ?? [])
const find = (kind: PhaseKind, lv?: number) =>
  visible.value.find(p => p.kind === kind && (lv === undefined || (p.ev as { level?: number }).level === lv))
const has = (kind: PhaseKind) => !!find(kind)
const walking = computed(() => has('miss'))
const port = computed(() => { const e = props.group?.phases[0]?.ev; return e && 'port' in e ? e.port.toLowerCase() : '' })
const addr = (lv: number) => find('pteaddr', lv)?.ev as Ev<'PteAddr'> | undefined
const read = (lv: number) => find('pteread', lv)
const readPte = (lv: number) => (read(lv)!.ev as Ev<'Walk'>).pte
const check = (lv: number) => find('ptecheck', lv)?.ev as Ev<'PteCheck'> | undefined
const perm = computed(() => walking.value ? find('perm')?.ev as Ev<'PermCheck'> | undefined : undefined)
const ad = computed(() => walking.value ? find('ad')?.ev as Ev<'AdUpdate'> | undefined : undefined)
const victim = computed(() => find('victim')?.ev as Ev<'TlbVictim'> | undefined)
const fill = computed(() => find('fill')?.ev as Ev<'TlbFill'> | undefined)
const fault = computed(() => walking.value ? find('fault')?.ev as Ev<'PageFault'> | undefined : undefined)
const permTxt = computed(() => {
  const x = perm.value!
  const need = props.group?.side === 'I' ? 'X' : port.value === 'store' ? 'W' : x.mxr ? 'R or X' : 'R'
  return `${port.value} needs ${need} · ${['U', 'S', '?', 'M'][x.priv_mode]}-mode, U=${(x.pte >> 4) & 1}${x.sum ? ' SUM' : ''}`
})
const isCur = (kind: PhaseKind, lv: number) => props.cur?.kind === kind && (props.cur.ev as { level?: number }).level === lv && props.group?.phases.includes(props.cur)
const levelOn = (lv: number) => !!addr(lv)
const levelCur = (lv: number) => !!props.cur && props.group?.phases.includes(props.cur) && (props.cur.ev as { level?: number }).level === lv

// ── FSM ──
type St = 'idle' | 'l1' | 'l0' | 'upd' | 'fault'
const STATES: { id: St; label: string; x: number; y: number; w: number }[] = [
  { id: 'idle', label: 'IDLE', x: 4, y: 4, w: 60 },
  { id: 'l1', label: 'L1 PTE', x: 96, y: 4, w: 70 },
  { id: 'l0', label: 'L0 PTE', x: 198, y: 4, w: 70 },
  { id: 'upd', label: 'UPDATE', x: 300, y: 4, w: 70 },
  { id: 'fault', label: 'FAULT', x: 390, y: 4, w: 60 },
]
const ARROWS = ['M64,12 L95,12', 'M166,12 L197,12', 'M268,12 L299,12', 'M131,21 C131,40 270,40 335,22', 'M233,21 C240,34 400,36 418,22', 'M370,12 L389,12']
const reached = computed<St[]>(() => {
  const r: St[] = []
  if (!props.group || props.group.sfence) return r
  if (visible.value.some(p => p.kind === 'compare')) r.push('idle')
  if (has('miss')) r.push('l1')
  if (find('pteaddr', 0)) r.push('l0')
  if (has('victim') || has('fill')) r.push('upd')
  if (fault.value) r.push('fault')
  return r
})
function stState(id: St): string {
  const r = reached.value
  if (!r.includes(id)) return 'off'
  if (id === 'fault') return 'fault'
  if (id === 'idle' && !walking.value) return 'hit'
  return r[r.length - 1] === id ? 'cur' : 'on'
}
const note = computed(() => !props.group ? '' : props.group.sfence ? 'SFENCE.VMA: walker idle' : !walking.value && reached.value.length ? 'TLB hit: walker stays IDLE' : '')
</script>

<style scoped>
.ptw { width: 465px; height: 336px; background: #fff; border: 1.5px solid #2aacbe; border-radius: 3px; overflow: hidden; font-family: 'JetBrains Mono', monospace; font-size: 8px; }
.ptw--focus { box-shadow: 0 0 0 3px #f59e0b; }
.ptw--fault { border-color: #dc2626; }
.ptw__head { display: flex; align-items: baseline; gap: 8px; background: #2aacbe; color: #fff; padding: 2px 6px; font-size: 10px; font-weight: 700; letter-spacing: 1px; }
.ptw__sub { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; margin-left: auto; font-size: 8px; font-weight: 400; letter-spacing: 0; }
.ptw__fsm { display: block; margin: 2px 4px 0; }
.ptw__note { font-size: 9px; font-weight: 600; fill: #1d4ed8; font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; }
.st { fill: #fff; stroke: #94a3b8; stroke-width: 1.2; }
.st--on { fill: #ede9fe; stroke: #7c3aed; stroke-width: 2; }
.st--cur { fill: #7c3aed; stroke: #7c3aed; stroke-width: 2; }
.st--hit { fill: #dbeafe; stroke: #2563eb; stroke-width: 2; }
.st--fault { fill: #dc2626; stroke: #dc2626; stroke-width: 2; }
.stt { font-size: 9px; font-weight: 700; fill: #374151; font-family: 'JetBrains Mono', monospace; }
.stt--on { fill: #6d28d9; }
.stt--hit { fill: #1d4ed8; }
.stt--cur, .stt--fault { fill: #fff; }
.lvl { display: flex; margin: 3px 4px; border: 1px solid #e2e8f0; border-radius: 3px; }
.lvl--on { border-color: #c4b5fd; }
.lvl--cur { border-color: #7c3aed; }
.lvl__name { width: 22px; display: flex; align-items: center; justify-content: center; font-size: 10px; font-weight: 700; color: #4b5563; background: #f8fafc; }
.lvl--on .lvl__name { color: #7c3aed; background: #f5f3ff; }
.lvl__rows { flex: 1; }
.row { display: flex; align-items: center; gap: 4px; padding: 1px 5px; min-height: 15px; border-left: 3px solid transparent; }
.row--cur { border-left-color: #f59e0b; background: #fffbeb; }
.op { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; color: #475569; }
.v { padding: 0 3px; border-radius: 2px; background: #f1f5f9; font-weight: 700; }
.v--ppn { background: #dcfce7; color: #166534; }
.v--vpn { background: #e0e7ff; color: #3730a3; }
.v--addr { background: #ede9fe; color: #6d28d9; }
.out { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; padding: 0 4px; border-radius: 2px; font-weight: 700; }
.out--L1Hit { background: #dcfce7; color: #15803d; }
.out--L2Hit { background: #fef3c7; color: #b45309; }
.out--Memory { background: #fee2e2; color: #b91c1c; }
.bits span { display: inline-block; width: 10px; text-align: center; margin-right: 1px; border-radius: 2px; }
.b1 { background: #334155; color: #fff; font-weight: 700; }
.b0 { background: #f1f5f9; color: #6b7280; }
.verdict { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; margin-left: auto; font-weight: 700; }
.verdict--Pointer { color: #7c3aed; }
.verdict--Leaf { color: #15803d; }
.verdict--Invalid, .verdict--Reserved, .verdict--Misaligned, .verdict--NoLeaf { color: #dc2626; }
.dim { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; color: #6b7280; }
.tail { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; margin: 3px 4px; border: 1px solid #e2e8f0; border-radius: 3px; }
.lbl { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; width: 40px; color: #7c3aed; font-weight: 700; }
.ok { color: #15803d; font-weight: 700; }
.bad { color: #dc2626; font-weight: 700; }
</style>
