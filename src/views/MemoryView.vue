<template>
  <div ref="scroller" class="mv">
    <div v-if="!mp.ready" class="mv__loading">{{ mp.error ? `Rust core not loaded (reload the page): ${mp.error}` : 'Loading the Rust core…' }}</div>

    <div v-else-if="s" class="canvas" :style="{ width: W + 'px', height: H + barH + 'px' }">
      <!-- ══ Page 1: MMU ═══════════════════════════════════════════════════ -->
      <!-- phase strip: every phase of this cycle's translations, current one lit -->
      <div class="strip" :style="box({ x: 10, y: 4 }, 1490, 40)">
        <div class="strip__chips">
          <span class="strip__cyc">cycle {{ mp.phaseCycle >= 0 ? mp.phaseCycle : '—' }}</span>
          <template v-for="(ph, i) in mp.phases" :key="i">
            <span v-if="i === 0 || newGroup(i)" class="strip__grp">{{ groupLabel(ph) }}</span>
            <span class="chip" :class="['chip--' + chipColor(ph), { 'chip--cur': i === mp.phaseIdx, 'chip--done': i < mp.phaseIdx }]"
                  :title="phaseText(ph)" @click="mp.phaseIdx = i">{{ ph.cache && ph.kind.startsWith('c-') ? ph.cache + ' ' : '' }}{{ CHIP[ph.kind] }}{{ lvTxt(ph) }}</span>
          </template>
          <span v-if="!mp.phases.length" class="strip__none">no MMU phase this cycle — press PHASE ▶ to run to the next translation{{ mp.skipHits ? ' miss / fault / flush' : '' }}</span>
        </div>
        <div class="strip__text">{{ curPhase ? `${mp.phaseIdx + 1}/${mp.phases.length} · ${phaseText(curPhase)}` : '' }}</div>
      </div>

      <div v-for="core in [0, 1]" :key="'f' + core" class="mmu-frame" :class="{ 'mmu-frame--fault': faultShown(core) }" :style="box(FRAME[core]!, FW, FH)">
        <span class="mmu-frame__title">MMU · Sv32 · core {{ core }}</span>
      </div>
      <div class="page-div" :style="{ left: P2 - 12 + 'px', height: H + 'px' }"></div>
      <div class="page-title" :style="pos({ x: P2 + 40, y: 4 })">CACHES · BUS · MEMORY</div>

      <!-- ══ Wires ══ -->
      <svg class="wires" :width="W" :height="H">
        <template v-for="core in [0, 1]" :key="'w' + core">
          <!-- VIPT: VA[5:4] set index straight to the L1s -->
          <MemWire :points="wp(core, 'index')" :color="indexColor(core)" :arrow="false" :width="2" idle="#0099cc"
                :label="v(core).iSet !== null || v(core).dSet !== null ? `VA[5:4] = L1 set ${setTxt(core)}` : 'VA[5:4] = L1 set index'" :label-idx="2" />
          <MemWire :points="wp(core, 'indexI')" :color="indexColor(core)" :width="2" idle="#0099cc" />
          <MemWire :points="wp(core, 'indexD')" :color="indexColor(core)" :width="2" idle="#0099cc" />
          <!-- CSRs → MMU -->
          <MemWire :points="wp(core, 'csr')" :color="csrColor(core)" :width="2" idle="#64748b" label="satp: MODE · ASID · PPN (root)" :label-idx="2" />
          <MemWire :points="wp(core, 'csrI')" :color="csrColor(core)" :width="1.5" idle="#64748b" />
          <MemWire :points="wp(core, 'csrD')" :color="csrColor(core)" :width="1.5" idle="#64748b" />
          <template v-for="sd in SIDES" :key="sd">
            <MemWire :points="wp(core, 'va' + sd)" :color="xg(core, sd) ? '#0099cc' : null" idle="#0099cc" :width="2" :label="sd === 'I' ? 'VA·IF' : 'VA·MA'" :label-idx="2" />
            <MemWire :points="wp(core, 'look' + sd)" :color="has(xg(core, sd), 'split') ? '#0099cc' : null" idle="#0099cc" :width="2" />
            <MemWire :points="wp(core, 'hit' + sd)" :color="has(xg(core, sd), 'hit') ? '#2563eb' : null" idle="#2563eb" :width="2" />
            <MemWire :points="wp(core, 'miss' + sd)" :color="has(xg(core, sd), 'miss') ? '#7c3aed' : null" idle="#7c3aed" :width="2" label="miss" />
            <MemWire :points="wp(core, 'upd' + sd)" :color="has(xg(core, sd), 'fill') ? '#7c3aed' : null" idle="#7c3aed" :width="2" label="update" :label-dy="11" />
            <MemWire :points="wp(core, 'tag' + sd)" :color="tagColor(core, sd)" idle="#0099cc" :label="'PA tag → L1' + sd" :label-idx="2"
                  :label-dy="12" />
          </template>
          <MemWire :points="wp(core, 'pte')" :color="ptwRead(core) ? '#7c3aed' : null" idle="#7c3aed" :width="2" label="PTE read → L1D" :label-idx="1" />
          <!-- L1 → system bus, snoop -->
          <MemWire :points="wp(core, 'busI')" :color="missColor(v(core).fetch)" :arrow="false" />
          <MemWire :points="wp(core, 'busD')" :color="wbColor(core) ?? missColor(v(core).data)" :arrow="false" />
          <MemWire :points="wp(core, 'snoop')" :color="snoopNow ? '#0891b2' : null" :arrow="false" :width="2" idle="#0891b2" />
        </template>
        <MemWire :points="[{ x: BUS, y: BUS_Y[0]! }, { x: BUS, y: BUS_Y[1]! }]" :color="busColor ?? (romNow ? ROM_C : null)" :arrow="false" :width="4" label="BUS" :label-idx="0" />
        <MemWire :points="[{ x: SNOOP, y: SNOOP_Y[0]! }, { x: SNOOP, y: SNOOP_Y[1]! }]" :color="snoopNow ? '#0891b2' : null" :arrow="false" :width="3" idle="#0891b2" label="SNOOP" :label-idx="0" />
        <MemWire :points="[{ x: BUS, y: L2Y + 24 }, { x: (BUS + TC2X) / 2, y: L2Y + 24 }, { x: TC2X, y: L2Y + 24 }]" :color="busColor" label="PA" :label-idx="1" />
        <!-- TAG CHECK ↔ cache: index selects the set; the set's tags go back to the comparators -->
        <!-- always drawn (faint); the selected set's wires light up as the lookup reaches them -->
        <template v-for="id in CACHE_IDS" :key="'sw' + id">
          <template v-for="r in (id === 'L2' ? 8 : 4)" :key="id + r">
            <MemWire v-if="!rowLit(id, r - 1, 2)" :points="setWire(id, r - 1)" idle="#d97706" :width="1.5" />
            <MemWire v-if="!rowLit(id, r - 1, 3)" :points="tagsWire(id, r - 1)" idle="#4338ca" :width="1.5" dashed :arrow="false" />
          </template>
          <template v-if="look(id).lookup">
            <MemWire v-if="rowLit(id, look(id).lookup!.set, 2)" :points="setWire(id, look(id).lookup!.set)" color="#d97706" :width="2.5" :label="'set ' + look(id).lookup!.set" :label-idx="3" />
            <MemWire v-if="rowLit(id, look(id).lookup!.set, 3)" :points="tagsWire(id, look(id).lookup!.set)" color="#4338ca" :width="2.5" dashed label="tags" :label-idx="3" :label-dy="11" />
          </template>
        </template>
        <!-- boot ROM ↔ bus (uncached reads) -->
        <MemWire :points="[{ x: L2X, y: 120 }, { x: (L2X + BUS) / 2, y: 120 }, { x: BUS + 2, y: 120 }]" :color="romNow ? ROM_C : null" idle="#92400e" label="ROM" :label-idx="1" :label-dy="12" />
        <MemWire :points="[{ x: BUS, y: 120 }, { x: BUS, y: BUS_Y[0]! }]" :color="romNow ? ROM_C : null" idle="#92400e" :arrow="false" :width="4" />
        <MemWire :points="[{ x: L2X + 230, y: 512 }, { x: L2X + 230, y: MEM_Y }]" :color="memRead ? '#dc2626' : null" label="read" :label-idx="0" />
        <MemWire :points="[{ x: L2X + 290, y: MEM_Y }, { x: L2X + 290, y: 512 }]" :color="memWrite ? '#ea580c' : null" label="write-back" :label-idx="0" />
      </svg>

      <template v-for="core in [0, 1]" :key="'b' + core">
        <!-- CPU: stages + CSRs -->
        <div class="blk cpu" :class="{ 'cpu--stall': c(core).stalled }" :style="pos(CPU[core]!)">
          <div class="blk__head"><span>CORE {{ core }}</span><span class="blk__sub">{{ MODE[cs(core).priv_mode] }}</span></div>
          <div v-for="(sg, i) in c(core).stages" :key="i" class="cpu__stage">
            <span class="cpu__name">{{ STAGES[i] }}</span>
            <span v-if="sg.valid && !sg.bubble" class="cpu__asm" :title="'pc 0x' + mp.hex(sg.pc)">{{ mp.asm(sg.instr) || '0x' + mp.hex(sg.instr) }}</span>
            <span v-else class="cpu__bubble">—</span>
          </div>
          <div class="csr__head">CSR</div>
          <div class="csr__row">
            <span class="csr" :class="chg(core, 'priv_mode')"><b>priv</b> {{ MODE1[cs(core).priv_mode] }}</span>
            <span class="csr"><b>hart</b> {{ cs(core).mhartid }}</span>
          </div>
          <div class="csr__row">
            <span class="csr csr--wide" :class="chg(core, 'satp')" :title="'satp 0x' + mp.hex(cs(core).satp)">
              <b>satp</b> {{ cs(core).satp >>> 31 ? 'Sv32' : 'Bare' }} · ASID {{ (cs(core).satp >>> 22) & 0x1ff }} · PPN {{ fh(cs(core).satp & 0x3fffff, 5) }}
            </span>
          </div>
          <div class="csr__row">
            <span class="csr csr--wide" :class="chg(core, 'mstatus')"><b>mstatus</b> MPP {{ MODE1[(cs(core).mstatus >>> 11) & 3] }} · SUM {{ (cs(core).mstatus >>> 18) & 1 }} · MXR {{ (cs(core).mstatus >>> 19) & 1 }}</span>
          </div>
          <div class="csr__row">
            <span class="csr" :class="chg(core, 'mtvec')"><b>mtvec</b> {{ mp.hex(cs(core).mtvec) }}</span>
            <span class="csr" :class="chg(core, 'mepc')"><b>mepc</b> {{ mp.hex(cs(core).mepc) }}</span>
          </div>
          <div class="csr__row">
            <span class="csr" :class="chg(core, 'mcause')" :title="CAUSE[cs(core).mcause] ?? ''"><b>mcause</b> {{ cs(core).mcause }}</span>
            <span class="csr" :class="chg(core, 'mtval')"><b>mtval</b> {{ mp.hex(cs(core).mtval) }}</span>
          </div>
          <div class="cpu__foot">
            <span v-if="c(core).stalled" class="cpu__wait">waiting for memory · {{ c(core).stall_left + 1 }}</span>
            <span v-else class="cpu__run">running</span>
          </div>
        </div>

        <template v-for="sd in SIDES" :key="'m' + sd">
          <!-- instruction / data interface -->
          <div class="blk ifc" :class="['ifc--' + ifcState(core, sd), { 'ifc--focus': ifcFocus(core, sd) }]" :style="pos(geo(core).ifc[sd])">
            <div class="blk__head blk__head--mmu"><span>{{ sd === 'I' ? 'INSTRUCTION' : 'DATA' }} INTERFACE</span><span class="blk__sub">{{ portTxt(core, sd) }}</span></div>
            <template v-if="xva(core, sd) !== null">
              <div class="ifc__row">
                <span class="ifc__lbl">VA</span>
                <template v-if="!has(xg(core, sd), 'bare')">
                  <span class="ifc__f ifc__f--vpn1" title="VPN[1]: root table index">{{ fh(xva(core, sd)! >>> 22, 3) }}</span>
                  <span class="ifc__f ifc__f--vpn0" title="VPN[0]: leaf table index">{{ fh((xva(core, sd)! >>> 12) & 0x3ff, 3) }}</span>
                  <span class="ifc__f ifc__f--off" title="page offset (bits 5:4 = L1 set)">{{ fh(xva(core, sd)! & 0xfff, 3) }}</span>
                </template>
                <span v-else class="ifc__bare">{{ mp.hex(xva(core, sd)!) }}</span>
              </div>
              <div class="ifc__row">
                <span class="ifc__lbl">PA</span>
                <span v-if="has(xg(core, sd), 'fault')" class="ifc__fault">PAGE FAULT</span>
                <span v-else-if="xpa(core, sd) !== null" class="ifc__pa">{{ mp.hex(xpa(core, sd)!) }}</span>
                <span v-else class="ifc__wait">…</span>
              </div>
              <div class="ifc__row ifc__perm">{{ permLine(core, sd) }}</div>
            </template>
            <div v-else class="ifc__row ifc__none">—</div>
          </div>

          <!-- ITLB / DTLB (CVA6 Figs 6, 7) -->
          <div class="cpu-block" :style="pos(geo(core).tlb[sd])">
            <TlbDetail v-bind="tlbProps(core, sd)" />
          </div>
        </template>

        <!-- page table walker -->
        <div class="cpu-block" :style="pos(geo(core).ptw)">
          <PtwDetail :group="ptwGroup(core)" :shown="shown" :cur="curPhase" :root="(cs(core).satp & 0x3fffff) << 12" :focus="ptwFocus(core)" />
        </div>

        <!-- ══ Page 2: caches — TAG CHECK (left) feeds each cache ══ -->
        <template v-for="sd in SIDES" :key="'c' + sd">
          <div class="cpu-block" :style="pos({ x: TCX, y: L1[core]![sd].y })">
            <TagCheck v-bind="tcProps(`L1${sd}:${core}`)" :title="`L1${sd} · core ${core}`" :ways="2" :sets="4" miss-to="L2" />
          </div>
          <div class="cpu-block" :style="pos(L1[core]![sd])">
            <CacheTable :title="`L1${sd} · core ${core}`" :cache="cacheShown(`L1${sd}:${core}`)" :marks="allMarks" :miss-sets="allMissSets"
              :key-of="(se, w) => `L1${sd}:${core}:${se}:${w}`" :set-key="se => `L1${sd}:${core}:${se}`"
              :lookup="look(`L1${sd}:${core}`).lookup" :stage="look(`L1${sd}:${core}`).stage"
              :active="(sd === 'I' ? !!v(core).fetch : !!v(core).data) && v(core).cycle === lastCycle" />
          </div>
        </template>
      </template>

      <div class="cpu-block" :style="pos({ x: TC2X, y: L2Y })">
        <TagCheck v-bind="tcProps('L2')" title="L2 · shared" :ways="4" :sets="8" miss-to="memory" l2 />
      </div>
      <div class="cpu-block" :style="pos({ x: L2X, y: L2Y })">
        <CacheTable title="L2 · shared, inclusive" :cache="cacheShown('L2')" :marks="allMarks" :miss-sets="allMissSets"
          :lookup="look('L2').lookup" :stage="look('L2').stage"
          :key-of="(se, w) => `L2:${se}:${w}`" :set-key="se => `L2:${se}`" :active="l2Active" />
      </div>
      <!-- boot ROM: uncached, every word a bus read -->
      <div class="blk rom" :class="{ 'rom--active': romNow }" :style="pos({ x: L2X, y: 22 })">
        <div class="blk__head blk__head--rom"><span>BOOT ROM · 0xF000_0000</span><span class="blk__sub">uncached · {{ lat.rom }} cycles / word · read-only</span></div>
        <div class="rom__hint">reset vector — both harts start here in M-mode; nothing from ROM is ever cached</div>
        <div v-for="(r, i) in mp.romLog" :key="i" class="rom__line" :class="{ 'rom__line--now': r.cycle === mp.romLog[0]?.cycle }">
          <span class="rom__c">C{{ r.core }}</span><span class="rom__addr">{{ mp.hex(r.pa) }}</span><span class="rom__w">{{ mp.hex(r.val) }}</span><span class="rom__asm">{{ mp.asm(r.val) }}</span>
        </div>
        <div v-if="!mp.romLog.length" class="rom__hint">no ROM reads yet</div>
      </div>
      <div class="blk mem" :class="{ 'mem--active': memRead || memWrite }" :style="pos({ x: L2X, y: MEM_Y })">
        <div class="blk__head"><span>MEMORY · RAM</span><span class="blk__sub">L1 {{ lat.l1 }} · L2 +{{ lat.l2 }} · memory +{{ lat.mem }} cycles</span></div>
        <div class="mem__hint">recent lines — <b>red</b> word = memory is stale (a cache has newer data)</div>
        <div v-for="l in memLines" :key="l.pa" class="mem__line" :class="{ 'mem__line--now': l.now }">
          <span class="mem__addr">{{ mp.hex(l.pa, 4) }}</span>
          <span class="mem__dir">{{ l.write ? 'W' : 'R' }}</span>
          <span v-for="(w, i) in l.raw" :key="i" class="mem__w" :class="{ 'mem__w--stale': w !== l.cached[i] }">{{ mp.hex(w) }}</span>
        </div>
      </div>

      <div class="about" :style="pos({ x: L2X + 540, y: MEM_Y })">
        <b>{{ mp.examples[mp.exampleIndex]?.name }}</b> — {{ mp.examples[mp.exampleIndex]?.about }}
      </div>
      <div class="legend" :style="pos({ x: L2X + 540, y: MEM_Y + 120 })">
        <span class="lg" style="border-color:#16a34a">L1 hit</span>
        <span class="lg" style="border-color:#d97706">L1 miss · L2 hit</span>
        <span class="lg" style="border-color:#dc2626">total miss / fault</span>
        <span class="lg" style="border-color:#2563eb">TLB hit</span>
        <span class="lg" style="border-color:#7c3aed">TLB miss · walk</span>
        <span class="lg" style="border-color:#0f766e">sfence</span>
        <span class="lg" style="border-color:#0891b2">snoop</span>
        <span class="lg" style="border-color:#ea580c">write-back</span>
        <span class="lg" style="border-color:#f59e0b; background:#fef3c7">changed / current</span>
      </div>
      <div class="blk log" :style="pos({ x: P2 + 40, y: 736 })">
        <div class="blk__head"><span>EVENTS</span><span class="blk__sub">cycle {{ s.cycle }}</span></div>
        <div class="log__body">
          <div v-for="(l, i) in mp.log" :key="i" :class="'log--c' + l.core"><span class="log__c">{{ l.cycle }}</span>{{ l.text }}</div>
        </div>
      </div>
    </div>

    <!-- ══ Controls ══ -->
    <div ref="barEl" class="bar">
      <button class="btn btn--nav" @click="emit('switch-cpu')">CPU</button>
      <button class="btn btn--page" title="scroll to the MMU page" @click="goPage(0)">◀ MMU</button>
      <button class="btn btn--page" title="scroll to the caches page" @click="goPage(1)">Caches ▶</button>
      <span class="bar__div">|</span>
      <select class="bar__sel" :value="mp.exampleIndex" @change="mp.loadExample(+($event.target as HTMLSelectElement).value)">
        <option v-for="(e, i) in mp.examples" :key="i" :value="i">{{ e.name }}</option>
      </select>
      <button class="btn btn--phase" :disabled="!mp.ready || mp.running" title="next MMU phase (runs to the next translation at the end)" @click="mp.nextPhase()">Phase ▶</button>
      <button class="btn btn--phase" :disabled="!mp.ready || mp.running" @click="mp.togglePlay()">{{ mp.playing ? 'Pause' : 'Play phases' }}</button>
      <label class="bar__chk" title="step only TLB misses, page faults and SFENCE.VMA"><input v-model="mp.skipHits" type="checkbox" /> skip TLB hits</label>
      <span class="bar__div">|</span>
      <button class="btn" :disabled="!mp.ready || mp.running || mp.playing" title="one clock (frozen cycles too)" @click="mp.step()">Step</button>
      <button class="btn" :disabled="!mp.ready || mp.running || mp.playing" title="run to the next cycle that touches memory" @click="mp.nextAccess()">Next access</button>
      <button class="btn" :disabled="!mp.ready || mp.playing" @click="mp.toggleRun()">{{ mp.running ? 'Stop' : 'Run' }}</button>
      <button class="btn btn--reset" :disabled="!mp.ready" @click="mp.loadExample(mp.exampleIndex)">Reset</button>
      <input v-model.number="mp.speed" type="range" min="20" max="1000" step="10" class="bar__speed" title="delay (ms)" />
      <span class="bar__lbl">{{ mp.speed }} ms · cycle {{ s?.cycle ?? 0 }}</span>
      <template v-if="s">
        <span v-for="core in [0, 1]" :key="core" class="bar__lbl">C{{ core }}: TLB {{ pct(st(core).tlb_hits, st(core).tlb_misses) }} · L1I {{ pct(st(core).l1i_hits, st(core).l1i_misses) }} · L1D {{ pct(st(core).l1d_hits, st(core).l1d_misses) }} · L2 {{ pct(st(core).l2_hits, st(core).l2_misses) }}</span>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import CacheTable from '../components/memory/CacheTable.vue'
import MemWire from '../components/memory/MemWire.vue'
import TlbDetail from '../components/memory/TlbDetail.vue'
import TagCheck from '../components/memory/TagCheck.vue'
import PtwDetail from '../components/memory/PtwDetail.vue'
import {
  useMemPanel, phaseText, viewOf, mineOf, changesCache, type CoreView, type LookupEv, type MpCache, type CsrName, type Outcome, type MemEvent, type LineMark,
  type Group, type Phase, type PhaseKind, type Side, type MpCore,
} from '../stores/memPanel'

const emit = defineEmits<{ 'switch-cpu': [] }>()
const mp = useMemPanel()
/** The canvas is taller by the fixed bar's height, so the page scrolls only when it does not fit */
const barEl = ref<HTMLDivElement | null>(null)
const barH = ref(65)
let barObs: ResizeObserver | null = null
onMounted(() => {
  if (!barEl.value) return
  barObs = new ResizeObserver(() => { barH.value = barEl.value?.offsetHeight ?? 65 })
  barObs.observe(barEl.value)
})
onBeforeUnmount(() => barObs?.disconnect())
onMounted(() => { mp.init().catch(() => { /* shown in the panel */ }) })
const scroller = ref<HTMLElement | null>(null)
/** The page scrolls sideways (the canvas is two pages wide): jump to the MMU or the caches */
const goPage = (n: number) => window.scrollTo({ left: n ? (scroller.value?.offsetLeft ?? 0) + P2 - 20 : 0, behavior: 'smooth' })

const STAGES = ['IF', 'ID', 'EX', 'MA', 'WB']
const SIDES = ['I', 'D'] as const
const MODE: Record<number, string> = { 0: 'U-mode', 1: 'S-mode', 3: 'M-mode' }
const MODE1: Record<number, string> = { 0: 'U', 1: 'S', 2: '?', 3: 'M' }
const CAUSE: Record<number, string> = { 12: 'instruction page fault', 13: 'load page fault', 15: 'store page fault' }
const CHIP: Record<PhaseKind, string> = {
  bare: 'BARE', split: 'VA', compare: 'CMP', perm: 'PERM', hit: 'PA', miss: 'MISS', pteaddr: 'ADDR', pteread: 'READ',
  ptecheck: 'CHECK', ad: 'A/D', victim: 'VICTIM', fill: 'UPDATE', fault: 'FAULT', 'sfence-gates': 'SFENCE', 'sfence-inval': 'INVAL',
  'c-split': 'ADDR', 'c-set': 'SET', 'c-cmp': 'TAG=', 'c-hit': 'HIT', 'c-miss': 'MISS', 'c-snoop': 'SNOOP', 'c-wb': 'WB',
  'c-evict': 'EVICT', 'c-memrd': 'MEM RD', 'c-memwr': 'MEM WR', 'c-fill': 'FILL', 'c-state': 'MESI',
}

// ── Geometry: page 1 (MMU) x 0…1500, page 2 (caches) from P2 ─────────────
type P = { x: number; y: number }
const W = 3440, H = 892, P2 = 1520
const FW = 1275, FH = 376
const IFC_W = 170, IFC_H = 66, TLB_W = 560, PTW_W = 465, CPU_H = 252
/** TAG CHECK blocks: left of the L1s, and between the bus and L2 */
const TCX = P2 + 20, TCW = 230
const L1X = TCX + TCW + 30, L1R = L1X + 391, SNOOP = L1R + 18, BUS = L1R + 34, TC2X = BUS + 24, L2X = TC2X + TCW + 30, L2Y = 250
/** Cache table rows: header height and row pitch (for the set-select wire) */
const CT_HEAD = 21.5, CT_ROW = 28.6
const CACHE_IDS = ['L1I:0', 'L1D:0', 'L1I:1', 'L1D:1', 'L2']
const Y0 = [70, 476]
const CPU: P[] = Y0.map(y => ({ x: 10, y: y + 20 }))
const FRAME: P[] = Y0.map(y => ({ x: 225, y }))
const L1: Record<Side, P>[] = [{ I: { x: L1X, y: 22 }, D: { x: L1X, y: 196 } }, { I: { x: L1X, y: 562 }, D: { x: L1X, y: 384 } }]
/** System bus (L1I core 0 … L1I core 1) and snoop bus (L1D core 0 … L1D core 1) */
const BUS_Y = [L1[0]!.I.y + 70, L1[1]!.I.y + 70], SNOOP_Y = [L1[0]!.D.y + 84, L1[1]!.D.y + 70]
const MEM_Y = 580

function geo(k: number) {
  const y0 = Y0[k]!
  const top: Side = k === 0 ? 'I' : 'D'
  const rowY = (sd: Side) => sd === top ? y0 + 22 : y0 + 206
  return {
    top, midY: y0 + 194,
    ifc: { I: { x: 240, y: rowY('I') + 40 }, D: { x: 240, y: rowY('D') + 40 } },
    tlb: { I: { x: 425, y: rowY('I') }, D: { x: 425, y: rowY('D') } },
    ptw: { x: 1020, y: y0 + 22 },
  }
}

function wires(k: number): Record<string, P[]> {
  const g = geo(k), y0 = Y0[k]!, cpu = CPU[k]!, l1 = L1[k]!
  const stageY = (i: number) => cpu.y + 16 + i * 19 + 10
  const idxY = k === 0 ? y0 - 12 : y0 + FH + 18
  const w: Record<string, P[]> = {
    index: [{ x: 110, y: k === 0 ? cpu.y : cpu.y + CPU_H }, { x: 110, y: idxY }, { x: 700, y: idxY }, { x: TCX - 10, y: idxY }],
    indexI: [{ x: TCX - 10, y: idxY }, { x: TCX - 10, y: l1.I.y + 38 }, { x: TCX, y: l1.I.y + 38 }],
    indexD: [{ x: TCX - 10, y: idxY }, { x: TCX - 10, y: l1.D.y + 38 }, { x: TCX, y: l1.D.y + 38 }],
    csr: [{ x: 210, y: g.midY }, { x: 300, y: g.midY }, { x: 700, y: g.midY }, { x: g.ptw.x, y: g.midY }],
    csrI: [{ x: 300, y: g.midY }, { x: 300, y: g.top === 'I' ? g.ifc.I.y + IFC_H : g.ifc.I.y }],
    csrD: [{ x: 300, y: g.midY }, { x: 300, y: g.top === 'D' ? g.ifc.D.y + IFC_H : g.ifc.D.y }],
    pte: [{ x: g.ptw.x + PTW_W, y: g.ptw.y + 150 }, { x: TCX - 30, y: g.ptw.y + 150 }, { x: TCX - 30, y: l1.D.y + 52 }, { x: TCX, y: l1.D.y + 52 }],
    busI: [{ x: L1R, y: l1.I.y + 70 }, { x: BUS, y: l1.I.y + 70 }],
    busD: [{ x: L1R, y: l1.D.y + 70 }, { x: BUS, y: l1.D.y + 70 }],
    snoop: [{ x: L1R, y: l1.D.y + (k === 0 ? 84 : 70) }, { x: SNOOP, y: l1.D.y + (k === 0 ? 84 : 70) }],
  }
  for (const sd of SIDES) {
    const ifc = g.ifc[sd], tlb = g.tlb[sd]
    const ys = stageY(sd === 'I' ? 0 : 3), yt = ifc.y + 30, xv = sd === 'I' ? 230 : 220
    w['va' + sd] = [{ x: 210, y: ys }, { x: xv, y: ys }, { x: xv, y: yt }, { x: ifc.x, y: yt }]
    w['look' + sd] = [{ x: ifc.x + IFC_W, y: ifc.y + 16 }, { x: tlb.x, y: ifc.y + 16 }]
    w['hit' + sd] = [{ x: tlb.x, y: ifc.y + 44 }, { x: ifc.x + IFC_W, y: ifc.y + 44 }]
    const tx = tlb.x + TLB_W, mx = (tx + g.ptw.x) / 2
    w['miss' + sd] = [{ x: tx, y: tlb.y + 50 }, { x: mx, y: tlb.y + 50 }, { x: g.ptw.x, y: tlb.y + 50 }]
    w['upd' + sd] = [{ x: g.ptw.x, y: tlb.y + 100 }, { x: mx, y: tlb.y + 100 }, { x: tx, y: tlb.y + 100 }]
    const up = g.top === sd
    const out = up ? y0 - 6 : y0 + FH + 8
    w['tag' + sd] = [{ x: ifc.x + 120, y: up ? ifc.y : ifc.y + IFC_H }, { x: ifc.x + 120, y: out }, { x: 900, y: out }, { x: TCX - 20, y: out },
      { x: TCX - 20, y: l1[sd].y + 24 }, { x: TCX, y: l1[sd].y + 24 }]
  }
  return w
}
const G = [wires(0), wires(1)]
const wp = (k: number, name: string): P[] => G[k]![name] ?? []
const pos = (p: P) => ({ left: `${p.x}px`, top: `${p.y}px` })
const box = (p: P, w: number, h: number) => ({ ...pos(p), width: `${w}px`, height: `${h}px` })

// ── State ────────────────────────────────────────────────────────────────
const s = computed(() => mp.snap)
/** Each core's view, built only from the events revealed so far (phase stepping) */
const vvs = computed<CoreView[]>(() => [0, 1].map(k => {
  const x = mp.views[k]!
  if (mp.cutoff === Infinity || x.cycle !== mp.phaseCycle) return x
  return viewOf(k, x.cycle, mineOf(k, mp.cycleEvents.filter((_, i) => i <= mp.cutoff)), mp.privAt[k]!)
}))
const v = (core: number): CoreView => vvs.value[core]!

// ── Cache lookups: what each TAG CHECK / cache shows ─────────────────────
const cacheName = (id: string) => id.split(':')[0]!
/** The lookup a cache shows and how far it is revealed (1 split, 2 set, 3 compare, 4 result); cur = step shown now */
const lookState = computed(() => {
  const out: Record<string, { lookup: LookupEv | null; stage: number; cur: number | null }> = {}
  const all = mp.cycleGroups.flatMap(g => g.phases)
  for (const id of CACHE_IDS) {
    const name = cacheName(id), core = id.includes(':') ? +id.split(':')[1]! : null
    const splits = all.filter(p => p.kind === 'c-split' && p.cache === name && (core === null || p.core === core))
    if (mp.cutoff !== Infinity && splits.length) {
      const sp = [...splits].reverse().find(shownP)
      if (!sp) { out[id] = { lookup: null, stage: 0, cur: null }; continue }
      const g = groupOf(sp)!.phases, i = g.indexOf(sp)
      const steps = [sp, g[i + 1], g[i + 2], g.slice(i + 3).find(p => (p.kind === 'c-hit' || p.kind === 'c-miss') && p.cache === name)]
      const stage = steps.filter(p => p && shownP(p)).length
      const ci = steps.findIndex(p => !!p && p === curPhase.value)
      out[id] = { lookup: sp.ev as LookupEv, stage, cur: ci >= 0 ? ci + 1 : null }
    } else {
      const lk = name === 'L2'
        ? [0, 1].map(k => v(k)).filter(x => x.lookups.L2).sort((a, b) => b.cycle - a.cycle)[0]?.lookups.L2
        : v(core!).lookups[id]
      out[id] = { lookup: lk ?? null, stage: lk ? 4 : 0, cur: null }
    }
  }
  return out
})
const look = (id: string) => lookState.value[id]!
/** Cache contents to draw: as before this cycle while a change to it is not revealed yet */
function cacheShown(id: string): MpCache {
  const k = +id.slice(4)
  const after = id === 'L2' ? s.value!.caches.l2 : id.startsWith('L1I') ? s.value!.caches.l1i[k]! : s.value!.caches.l1d[k]!
  const b = mp.cachesBefore
  if (mp.cutoff === Infinity || !b || !mp.cycleEvents.some((e, i) => i > mp.cutoff && changesCache(e, id))) return after
  return id === 'L2' ? b.l2 : id.startsWith('L1I') ? b.l1i[k]! : b.l1d[k]!
}
function tcProps(id: string) {
  const st = look(id)
  const c = cacheShown(id)
  let word: number | null = null
  if (st.lookup && st.stage >= 4) {
    const hw = st.lookup.ways.findIndex(w => w.hit)
    if (hw >= 0) word = c.lines[st.lookup.set * c.ways + hw]?.data[(st.lookup.pa >> 2) & 3] ?? null
  }
  return { lookup: st.lookup, stage: st.stage, cur: st.cur, word, focus: st.cur !== null }
}
/** Where a cache table is on the canvas */
const cachePos = (id: string): P => id === 'L2' ? { x: L2X, y: L2Y } : L1[+id.slice(4)]![id[2] as Side]
const tcRight = (id: string) => (id === 'L2' ? TC2X : TCX) + TCW
/** Middle of row `r` of a cache table */
const rowY = (id: string, r: number) => cachePos(id).y + CT_HEAD + (r + 0.5) * CT_ROW
/** Is row r's wire lit? (it is the looked-up set and the lookup reached `stage`) */
const rowLit = (id: string, r: number, stage: number) => { const st = look(id); return !!st.lookup && st.lookup.set === r && st.stage >= stage }
/** Index decoder: from TAG CHECK step 2 to every row (one lit when selected) */
function setWire(id: string, r: number): P[] {
  const x0 = tcRight(id), xa = x0 + 8, x1 = cachePos(id).x, y0 = cachePos(id).y + 46, y = rowY(id, r) - 5
  return [{ x: x0, y: y0 }, { x: xa, y: y0 }, { x: xa, y }, { x: (xa + x1) / 2, y }, { x: x1, y }]
}
/** The set's tags back to the comparators (step 3) */
function tagsWire(id: string, r: number): P[] {
  const x0 = cachePos(id).x, xb = tcRight(id) + 18, x1 = tcRight(id), y = rowY(id, r) + 6, yIn = cachePos(id).y + 76
  return [{ x: x0, y }, { x: (x0 + xb) / 2, y }, { x: xb, y }, { x: xb, y: yIn }, { x: x1, y: yIn }]
}

const c = (core: number) => s.value!.cores[core]!
const st = (core: number) => s.value!.stats[core]!
const lastCycle = computed(() => Math.max(v(0).cycle, v(1).cycle))
const fh = (n: number, w: number) => (n >>> 0).toString(16).padStart(w, '0')
const pct = (h: number, m: number) => h + m ? `${Math.round(100 * h / (h + m))}%` : '—'

// ── Phases ───────────────────────────────────────────────────────────────
type Ev<K extends MemEvent['kind']> = Extract<MemEvent, { kind: K }>
const curPhase = computed<Phase | null>(() => mp.phases[mp.phaseIdx] ?? null)
const isNow = (k: number) => mp.mmu[k]!.cycle === mp.phaseCycle
/** Is a phase of this cycle revealed? Phases go in event order; phases of one event (e.g. split / compare) by their index */
const shownP = (ph: Phase) => mp.cutoff === Infinity || ph.ei < mp.cutoff || (ph.ei === mp.cutoff && ph.gi >= 0 && ph.gi <= mp.phaseIdx)
/** …and for MMU phases kept from an older cycle: yes */
const shown = (ph: Phase) => !isNow(ph.core) || shownP(ph)
const has = (g: Group | null, kind: PhaseKind) => !!g?.phases.some(p => p.kind === kind && shown(p))
const ev = <K extends MemEvent['kind']>(g: Group | null, kind: PhaseKind) => g?.phases.find(p => p.kind === kind && shown(p))?.ev as Ev<K> | undefined
const inGroup = (g: Group | null) => !!g && !!curPhase.value && g.phases.includes(curPhase.value)
const groupOf = (ph: Phase) => mp.cycleGroups.find(g => g.phases.includes(ph)) ?? mp.mmu[ph.core]!.groups.find(g => g.phases.includes(ph))
const groupLabel = (ph: Phase) => { const g = groupOf(ph); return `C${ph.core}` + (g?.cache ? '·' + g.cache : ph.side ? '·' + ph.side : '') }
const newGroup = (i: number) => { const a = mp.phases[i - 1], b = mp.phases[i]; return !!a && !!b && groupOf(a) !== groupOf(b) }
const lvTxt = (ph: Phase) => ph.kind === 'pteaddr' || ph.kind === 'pteread' || ph.kind === 'ptecheck' ? ' L' + (ph.ev as { level: number }).level : ''
const chipColor = (ph: Phase) => ph.kind === 'c-hit' ? 'green' : ph.kind === 'c-miss' || ph.kind === 'c-fill' ? (ph.cache === 'L2' ? 'red' : 'amber')
  : ph.kind === 'c-memrd' || ph.kind === 'c-memwr' ? 'red' : ph.kind.startsWith('c-') ? 'slate'
  : ph.kind === 'fault' || (ph.kind === 'perm' && !(ph.ev as Ev<'PermCheck'>).ok) ? 'red'
  : ph.kind.startsWith('sfence') ? 'teal' : groupOf(ph)?.hit || ph.kind === 'split' || ph.kind === 'compare' || ph.kind === 'bare' ? 'blue' : 'purple'

/** The translation group of core k, side sd: the one with the current phase, else the last one */
function xg(k: number, sd: Side): Group | null {
  const gs = mp.mmu[k]!.groups.filter(g => !g.sfence && g.side === sd)
  return gs.find(inGroup) ?? gs[gs.length - 1] ?? null
}
/** The group a TLB shows: an SFENCE.VMA while it is current or the only work, else the translation */
function tg(k: number, sd: Side): Group | null {
  const sf = mp.mmu[k]!.groups.find(g => g.sfence) ?? null
  const x = xg(k, sd)
  if (sf && (inGroup(sf) || !x || (!inGroup(x) && !x.phases.some(shown)))) return sf
  return x
}
/** CPU state to show: before this cycle while its trap phase is not revealed yet */
function cs(k: number): MpCore {
  const m = mp.mmu[k]!
  const pending = isNow(k) && m.groups.some(g => g.phases.some(p => p.kind === 'fault' && !shown(p)))
  return pending && m.before ? m.before : c(k)
}
const faultShown = (k: number) => mp.mmu[k]!.groups.some(g => has(g, 'fault'))

function tlbProps(k: number, sd: Side) {
  const g = tg(k, sd), m = mp.mmu[k]!
  const now = isNow(k) && !!g && !!m.before
  const before = m.before
  const pending = now && g!.phases.some(p => (p.kind === 'fill' || p.kind === 'sfence-inval') && !shown(p))
  const entries = pending ? (sd === 'I' ? before!.itlb : before!.dtlb) : (sd === 'I' ? c(k).itlb : c(k).dtlb)
  const look = ev<'TlbLookup'>(g, 'split')
  const cmp = ev<'TlbLookup'>(g, 'compare')
  const sf = ev<'Sfence'>(g, 'sfence-gates')
  const vic = ev<'TlbVictim'>(g, 'victim')
  const fill = ev<'TlbFill'>(g, 'fill')
  const hit = ev<'TlbHit'>(g, 'hit')
  const plruNow = sd === 'I' ? c(k).itlb_plru : c(k).dtlb_plru
  const plruBefore = now ? (sd === 'I' ? before!.itlb_plru : before!.dtlb_plru) : plruNow
  const plruNext = hit && cmp ? cmp.plru_after : vic && fill ? vic.plru_after : null
  const cur = curPhase.value
  return {
    title: sd === 'I' ? 'ITLB' : 'DTLB',
    entries, plru: plruBefore, plruNext: now ? plruNext : null,
    lookup: look ? { va: look.va, asid: look.asid } : null,
    cmp: cmp ? cmp.cmp : null,
    hitIdx: cmp ? cmp.hit : null,
    flush: sf ? { va: sf.va, asid: sf.asid, f: sd === 'I' ? sf.itlb : sf.dtlb } : null,
    flushDone: has(g, 'sfence-inval'),
    victim: vic ? { idx: vic.idx, free: vic.was_invalid } : null,
    fillIdx: fill ? fill.idx : null,
    touched: hit ? hit.idx : fill ? fill.idx : null,
    status: has(g, 'fault') ? 'FAULT' : sf ? 'FLUSH' : fill ? 'UPDATE' : cmp ? (cmp.hit !== null ? 'HIT' : 'MISS') : '',
    focus: !!cur && inGroup(g) && ['split', 'compare', 'hit', 'victim', 'fill', 'sfence-gates', 'sfence-inval'].includes(cur.kind),
  }
}

function ptwGroup(k: number): Group | null {
  const gs = mp.mmu[k]!.groups.filter(g => !g.sfence)
  return gs.find(inGroup) ?? [...gs].reverse().find(g => g.phases.some(p => p.kind === 'miss')) ?? gs[gs.length - 1] ?? null
}
const ptwFocus = (k: number) => { const cur = curPhase.value; return !!cur && cur.core === k && ['miss', 'pteaddr', 'pteread', 'ptecheck', 'perm', 'ad', 'victim', 'fill', 'fault'].includes(cur.kind) && !groupOf(cur)?.hit }
const ptwRead = (k: number) => has(ptwGroup(k), 'pteread')

// interface
const xva = (k: number, sd: Side) => { const g = xg(k, sd); return ev<'TlbLookup'>(g, 'split')?.va ?? ev<'Bare'>(g, 'bare')?.va ?? null }
const xpa = (k: number, sd: Side) => { const g = xg(k, sd); return ev<'TlbHit'>(g, 'hit')?.pa ?? ev<'TlbFill'>(g, 'fill')?.pa ?? ev<'Bare'>(g, 'bare')?.va ?? null }
function ifcState(k: number, sd: Side): string {
  const g = xg(k, sd)
  return has(g, 'fault') ? 'fault' : has(g, 'bare') ? 'bare' : has(g, 'fill') || has(g, 'miss') ? 'walk' : has(g, 'hit') ? 'hit' : ''
}
const ifcFocus = (k: number, sd: Side) => { const cur = curPhase.value; return !!cur && inGroup(xg(k, sd)) && (cur.kind === 'split' || cur.kind === 'hit' || cur.kind === 'bare' || cur.kind === 'fill') }
const portTxt = (k: number, sd: Side) => { const e = xg(k, sd)?.phases[0]?.ev; return e && 'port' in e ? e.port.toLowerCase() : sd === 'I' ? 'fetch' : 'load / store' }
function permLine(k: number, sd: Side): string {
  const g = xg(k, sd)
  if (has(g, 'bare')) return 'M-mode or Bare: no translation'
  const p = ev<'PermCheck'>(g, 'perm')
  if (!p) return ''
  const need = sd === 'I' ? 'X' : portTxt(k, sd) === 'store' ? 'W' : p.mxr ? 'R|X' : 'R'
  return `check ${need} · ${MODE1[p.priv_mode]}: U=${(p.pte >> 4) & 1} ${p.ok ? '✓' : '✗'}`
}

// ── Colours ──────────────────────────────────────────────────────────────
const ORDER: Outcome[] = ['L1Hit', 'L2Hit', 'Memory']
const worse = (a: Outcome | null, b: Outcome | null) => !a ? b : !b ? a : ORDER.indexOf(b) > ORDER.indexOf(a) ? b : a
const setTxt = (core: number) => [v(core).iSet !== null ? `I${v(core).iSet}` : '', v(core).dSet !== null ? `D${v(core).dSet}` : ''].filter(Boolean).join(' ')
const indexColor = (core: number) => v(core).iSet !== null || v(core).dSet !== null ? '#0099cc' : null
const csrColor = (k: number) => has(ptwGroup(k), 'pteaddr') ? '#7c3aed' : SIDES.some(sd => has(xg(k, sd), 'split')) ? '#475569' : null
function tagColor(k: number, sd: Side) {
  const g = xg(k, sd)
  return has(g, 'fault') ? '#dc2626' : has(g, 'fill') ? '#7c3aed' : has(g, 'hit') ? '#2563eb' : has(g, 'bare') ? '#64748b' : null
}
const missColor = (o: Outcome | null) => o === 'L2Hit' ? '#d97706' : o === 'Memory' ? '#dc2626' : null
const wbColor = (core: number) => v(core).events.some(e => e.kind === 'WriteBack' && e.core === core) ? '#ea580c' : null
const busColor = computed(() => missColor([0, 1].reduce<Outcome | null>((a, k) => worse(a, worse(v(k).fetch, v(k).data)), null)))
const chg = (core: number, n: CsrName) => ({ 'csr--chg': mp.csrChanged[core]?.has(n) && cs(core) === c(core) })

const allMarks = computed(() => {
  const m = new Map<string, LineMark>()
  for (const core of [0, 1]) for (const [k, x] of v(core).lines) if (!m.has(k) || v(core).cycle === lastCycle.value) m.set(k, x)
  return m
})
const allMissSets = computed(() => new Set([...v(0).missSets, ...v(1).missSets]))
const l2Active = computed(() => [0, 1].some(k => { const o = worse(v(k).fetch, v(k).data); return o === 'L2Hit' || o === 'Memory' }))
const evNow = (pred: (e: MemEvent) => boolean) => [0, 1].some(k => v(k).cycle === lastCycle.value && v(k).events.some(pred))
const memRead = computed(() => evNow(e => e.kind === 'MemRead'))
const memWrite = computed(() => evNow(e => e.kind === 'MemWrite'))
const snoopNow = computed(() => evNow(e => e.kind === 'Snoop'))
const romNow = computed(() => evNow(e => e.kind === 'RomRead'))
/** The newest L2 lookup of either core */
const ROM_C = '#b45309'
const lat = { l1: 1, l2: 4, mem: 20, rom: 8 }
const memLines = computed(() => {
  void mp.tick
  const seen = new Map<number, { pa: number; write: boolean; now: boolean }>()
  for (const core of [0, 1]) for (const e of v(core).events) {
    if (e.kind === 'MemRead' || e.kind === 'MemWrite') seen.set(e.pa, { pa: e.pa, write: e.kind === 'MemWrite', now: v(core).cycle === lastCycle.value })
  }
  for (const l of mp.log) {
    const m = /memory (read|write-back) line ([0-9a-f]+)/.exec(l.text)
    if (m && seen.size < 7) { const pa = parseInt(m[2]!, 16); if (!seen.has(pa)) seen.set(pa, { pa, write: m[1] !== 'read', now: false }) }
  }
  return [...seen.values()].slice(0, 7).map(l => ({ ...l, ...mp.memWords(l.pa, 4) }))
})
</script>

<style scoped>
.mv { font-family: 'JetBrains Mono', 'Courier New', monospace; color: #222; }
.mv__loading { padding: 40px; color: #4b5563; }
.canvas { position: relative; background: #fafafa; }
.wires { position: absolute; top: 0; left: 0; pointer-events: none; z-index: 2; }
.cpu-block, .blk { position: absolute; z-index: 1; }
.page-div { position: absolute; top: 0; width: 0; border-left: 2px dashed #cbd5e1; }
.page-title { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; position: absolute; font-size: 9px; font-weight: 700; letter-spacing: 2px; color: #4b5563; }

.strip { position: absolute; z-index: 3; background: #1e293b; border-radius: 4px; color: #e2e8f0; padding: 3px 6px; box-sizing: border-box; overflow: hidden; }
.strip__chips { display: flex; align-items: center; gap: 3px; white-space: nowrap; overflow-x: auto; height: 16px; }
.strip__cyc { font-size: 8px; color: #cbd5e1; margin-right: 4px; }
.strip__grp { font-size: 8px; font-weight: 700; color: #fbbf24; margin: 0 2px 0 6px; }
.strip__none { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 9px; color: #e2e8f0; }
.chip { font-size: 8px; font-weight: 700; padding: 0 4px; border-radius: 2px; cursor: pointer; opacity: 0.6; border: 1px solid transparent; }
.chip--done { opacity: 0.8; }
.chip--cur { opacity: 1; border-color: #fbbf24; box-shadow: 0 0 0 1px #fbbf24; }
.chip--blue { background: #2563eb; }
.chip--purple { background: #7c3aed; }
.chip--red { background: #dc2626; }
.chip--teal { background: #0f766e; }
.chip--green { background: #16a34a; }
.chip--amber { background: #d97706; }
.chip--slate { background: #475569; }
.strip__text { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 10px; margin-top: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: #fde68a; }

.blk { background: #fff; border: 1.5px solid #2aacbe; border-radius: 3px; overflow: hidden; }
.blk__head { display: flex; align-items: baseline; gap: 8px; background: #2aacbe; color: #fff; padding: 2px 6px; font-size: 10px; font-weight: 700; letter-spacing: 1px; }
.blk__head--mmu { font-size: 8px; padding: 2px 5px; }
.blk__sub { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; margin-left: auto; font-size: 8px; font-weight: 400; letter-spacing: 0; }

.cpu { width: 200px; }
.cpu--stall { border-color: #d97706; box-shadow: 0 0 0 2px #fde68a; }
.cpu__stage { display: flex; gap: 6px; padding: 3px 6px; border-bottom: 1px solid #f0f0f0; font-size: 10px; height: 19px; box-sizing: border-box; }
.cpu__name { width: 18px; color: #2aacbe; font-weight: 700; }
.cpu__asm { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.cpu__bubble { color: #6b7280; }
.cpu__foot { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; padding: 3px 6px; font-size: 8px; }
.cpu__wait { color: #fff; background: #d97706; padding: 1px 5px; border-radius: 2px; font-weight: 700; }
.cpu__run { color: #16a34a; }
.csr__head { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; padding: 1px 6px; font-size: 8px; font-weight: 700; letter-spacing: 1px; color: #475569; background: #f1f5f9; border-top: 1px solid #e5e7eb; }
.csr__row { display: flex; gap: 4px; padding: 1px 4px; }
.csr { flex: 1; font-size: 8px; padding: 1px 3px; border: 1px solid #e5e7eb; border-radius: 2px; white-space: nowrap; overflow: hidden; }
.csr b { color: #2aacbe; font-weight: 700; }
.csr--wide { flex: 1 1 100%; }
.csr--chg { background: #fef3c7; border-color: #f59e0b; font-weight: 700; }

.mmu-frame { position: absolute; z-index: 0; background: #d5f5ef; border: 1.5px solid #2aacbe; border-radius: 4px; }
.mmu-frame--fault { border-color: #dc2626; box-shadow: 0 0 0 2px #fecaca; }
.mmu-frame__title { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; position: absolute; top: 3px; left: 8px; font-size: 10px; font-weight: 700; letter-spacing: 1px; color: #0f766e; }

.ifc { width: 170px; height: 66px; font-size: 9px; }
.ifc--focus { box-shadow: 0 0 0 3px #f59e0b; }
.ifc--hit { border-color: #2563eb; }
.ifc--walk { border-color: #7c3aed; }
.ifc--fault { border-color: #dc2626; background: #fef2f2; }
.ifc__row { display: flex; align-items: center; gap: 3px; padding: 1px 5px; min-height: 13px; }
.ifc__lbl { width: 14px; color: #4b5563; font-weight: 700; font-size: 8px; }
.ifc__f { padding: 0 3px; border-radius: 2px; font-weight: 700; }
.ifc__f--vpn1 { background: #ede9fe; color: #6d28d9; }
.ifc__f--vpn0 { background: #e0e7ff; color: #4338ca; }
.ifc__f--off { background: #f1f5f9; color: #475569; }
.ifc__bare { color: #475569; }
.ifc__pa { font-weight: 700; }
.ifc__wait { color: #6b7280; }
.ifc__fault { color: #fff; background: #dc2626; padding: 0 4px; border-radius: 2px; font-weight: 700; font-size: 8px; }
.ifc__perm { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 8px; color: #475569; }
.ifc__none { color: #6b7280; }

.mem { width: 518px; font-size: 9px; }
.rom { width: 518px; height: 212px; font-size: 9px; border-color: #b45309; }
.rom--active { box-shadow: 0 0 0 2px #fcd34d; }
.blk__head--rom { background: #b45309; }
.rom__hint { padding: 2px 6px; font-size: 8px; color: #4b5563; font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; }
.rom__line { display: flex; gap: 10px; padding: 1px 6px; }
.rom__line--now { background: #fef3c7; font-weight: 700; }
.rom__c { width: 18px; color: #92400e; font-weight: 700; }
.rom__addr { width: 62px; }
.rom__w { width: 62px; color: #374151; }
.rom__asm { color: #1e3a8a; }
.mem--active { border-color: #dc2626; box-shadow: 0 0 0 2px #fecaca; }
.mem__hint { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; padding: 2px 6px; font-size: 8px; color: #4b5563; }
.mem__line { display: flex; gap: 10px; padding: 1px 6px; font-size: 9px; }
.mem__line--now { background: #fee2e2; }
.mem__addr { font-weight: 700; width: 34px; }
.mem__dir { color: #4b5563; width: 10px; }
.mem__w { width: 70px; color: #374151; }
.mem__w--stale { font-weight: 700; color: #b91c1c; }

.about { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; position: absolute; width: 360px; font-size: 9px; color: #444; line-height: 1.35; }
.legend { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; position: absolute; width: 360px; display: flex; flex-wrap: wrap; gap: 4px; font-size: 8px; }
.lg { padding: 1px 5px; border: 2px solid; border-radius: 2px; background: #fff; }
.log { width: 560px; height: 150px; display: flex; flex-direction: column; }
.log__body { overflow-y: auto; padding: 2px 6px; font-size: 8px; line-height: 11px; }
.log__body > div { white-space: nowrap; }
.log--c0 { color: #1e3a8a; }
.log--c1 { color: #7c2d12; }
.log__c { display: inline-block; width: 30px; color: #4b5563; }

.bar { position: fixed; bottom: 0; left: 0; right: 0; z-index: 10; display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; padding: 6px 12px; background: #1a1a2e; border-top: 1px solid #333; }
.btn { font: inherit; font-size: 11px; font-weight: 700; letter-spacing: 1px; text-transform: uppercase; padding: 5px 10px; border: 1px solid #555; border-radius: 3px; background: #2a2a3e; color: #ddd; cursor: pointer; }
.btn:disabled { opacity: 0.4; cursor: default; }
.btn--nav { border-color: #7c7cff; color: #b8b8ff; }
.btn--page { border-color: #475569; color: #cbd5e1; }
.btn--phase { border-color: #f59e0b; color: #fde68a; }
.btn--reset { border-color: #c44; color: #f88; }
.bar__div { color: #555; }
.bar__sel { font: inherit; font-size: 11px; padding: 4px; background: #2a2a3e; color: #ddd; border: 1px solid #555; max-width: 260px; }
.bar__chk { font-size: 10px; color: #fde68a; display: flex; align-items: center; gap: 3px; }
.bar__speed { width: 80px; }
.bar__lbl { font-size: 10px; color: #d1d5db; }
</style>
