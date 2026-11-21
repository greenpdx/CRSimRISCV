<template>
  <div class="ov">
    <div v-if="!o.ready" class="ov__loading">{{ o.error ? `Rust core not loaded (reload the page): ${o.error}` : 'Loading the Rust core…' }}</div>

    <div v-else-if="s" class="canvas" :style="{ width: W + 'px', height: H + 'px' }">
      <!-- ══ stage strip: the 7 stages of one cycle ══ -->
      <div class="strip" :style="box(10, 4, 1700, 42)">
        <div class="strip__row">
          <span class="strip__cyc">cycle {{ shownCycle }}</span>
          <span v-for="(st, i) in STAGES" :key="st" class="chip" :class="chipClass(i)">{{ i + 1 }} {{ st.toUpperCase() }}</span>
          <span class="strip__state">{{ s.halted ? 'program done (j . committed)' : s.serializing ? 'serializing: waiting for an empty ROB' : '' }}</span>
        </div>
        <div class="strip__text">{{ stageCaption }}</div>
      </div>

      <!-- ══ wires ══ -->
      <svg class="wires" :width="W" :height="H">
        <!-- predictor → fetch -->
        <MemWire :points="[{ x: 10, y: 345 }, { x: 4, y: 345 }, { x: 4, y: 100 }, { x: 10, y: 100 }]" :color="has('Fetch') ? C.fetch : null" idle="#64748b" :width="2" />
        <!-- fetch → IQ -->
        <MemWire :points="[{ x: 160, y: 148 }, { x: 160, y: 158 }]" :color="has('Fetch') ? C.fetch : null" idle="#64748b" :width="2" />
        <!-- IQ → rename → dispatch bus -->
        <MemWire :points="[{ x: 310, y: 200 }, { x: 320, y: 200 }, { x: 320, y: 140 }, { x: 330, y: 140 }]" :color="has('Dispatch') ? C.disp : null" idle="#64748b" :width="2" />
        <MemWire :points="[{ x: 630, y: 140 }, { x: 645, y: 140 }, { x: 645, y: 596 }]" :color="has('Dispatch') ? C.disp : null" idle="#64748b" :width="2.5" :arrow="false" label="dispatch" :label-idx="1" />
        <MemWire v-for="y in [80, 200, 320, 512]" :key="'d' + y" :points="[{ x: 645, y }, { x: 660, y }]" :color="dispatchTo(y) ? C.disp : null" idle="#64748b" :width="2" />
        <MemWire :points="[{ x: 645, y: 140 }, { x: 645, y: 50 }, { x: 1000, y: 50 }, { x: 1520, y: 50 }, { x: 1520, y: 56 }]" :color="has('Dispatch') ? C.disp : null" idle="#64748b" :width="2" />
        <!-- RS → units -->
        <MemWire :points="[{ x: 1090, y: 90 }, { x: 1100, y: 90 }, { x: 1100, y: 80 }, { x: 1110, y: 80 }]" :color="issuedTo('Alu', 0) ? C.issue : null" idle="#64748b" :width="2" />
        <MemWire :points="[{ x: 1100, y: 90 }, { x: 1100, y: 136 }, { x: 1110, y: 136 }]" :color="issuedTo('Alu', 1) ? C.issue : null" idle="#64748b" :width="2" />
        <MemWire :points="[{ x: 1090, y: 226 }, { x: 1110, y: 226 }]" :color="issuedTo('Mul') ? C.issue : null" idle="#64748b" :width="2" />
        <MemWire :points="[{ x: 1090, y: 340 }, { x: 1110, y: 340 }]" :color="issuedTo('Br') ? C.issue : null" idle="#64748b" :width="2" />
        <MemWire :points="[{ x: 1090, y: 540 }, { x: 1110, y: 540 }]" :color="issuedTo('Lsu') || has('MemRead') || has('Forward') ? C.issue : null" idle="#64748b" :width="2" />
        <!-- units → CDBs -->
        <MemWire v-for="y in [80, 136, 226, 340, 540]" :key="'r' + y" :points="[{ x: 1310, y }, { x: 1324, y }]" :color="cdbFrom(y) ? C.cdb : null" idle="#d97706" :width="2" :arrow="false" />
        <MemWire :points="[{ x: 1324, y: 80 }, { x: 1324, y: 654 }]" :color="has('Broadcast') ? C.cdb : null" idle="#d97706" :width="2.5" :arrow="false" />
        <!-- the two CDBs -->
        <MemWire :points="[{ x: 1330, y: 640 }, { x: 652, y: 640 }]" :color="bus(0) ? C.cdb : null" idle="#d97706" :width="4" :arrow="false" />
        <MemWire :points="[{ x: 1330, y: 654 }, { x: 652, y: 654 }]" :color="bus(1) ? C.cdb : null" idle="#d97706" :width="4" :arrow="false" />
        <!-- CDB → RS / LSQ (wake-up) and → ROB -->
        <MemWire :points="[{ x: 655, y: 640 }, { x: 655, y: 100 }]" :color="has('Wakeup') ? C.cdb : null" idle="#d97706" :width="2" :arrow="false" />
        <MemWire v-for="y in [100, 220, 340, 560]" :key="'w' + y" :points="[{ x: 655, y }, { x: 660, y }]" :color="has('Wakeup') ? C.cdb : null" idle="#d97706" :width="2" />
        <MemWire :points="[{ x: 1330, y: 647 }, { x: 1440, y: 647 }, { x: 1440, y: 626 }]" :color="has('Broadcast') ? C.cdb : null" idle="#d97706" :width="2.5" label="value, done" :label-idx="1" :label-dy="13" />
        <!-- commit: ROB head → register file -->
        <MemWire :points="[{ x: 1350, y: 626 }, { x: 1350, y: 672 }, { x: 900, y: 672 }, { x: 638, y: 672 }, { x: 638, y: 330 }, { x: 630, y: 330 }]" :color="has('Commit') ? C.commit : null" idle="#15803d" :width="2.5" label="commit (ROB head, in order)" :label-idx="2" :label-dy="13" />
        <!-- branch unit: mispredict → flush, PC ← target -->
        <MemWire :points="[{ x: 1310, y: 330 }, { x: 1316, y: 330 }, { x: 1316, y: 50 }, { x: 1316, y: 46 }, { x: 160, y: 46 }, { x: 160, y: 56 }]" :color="mispredict ? C.flush : null" idle="#dc2626" :width="2" dashed label="mispredict: flush younger, PC ← target" :label-idx="4" :label-dy="-3" />
      </svg>

      <!-- ══ FETCH ══ -->
      <div class="blk" :class="{ 'blk--on': has('Fetch') }" :style="box(10, 56, 300, 92)">
        <div class="blk__head"><span>FETCH</span><span class="blk__sub">{{ s.caches ? 'L1I → L2 → memory' : 'memory: 2 cycles' }}</span></div>
        <div class="kv"><b>PC</b> {{ o.hex(s.pc) }}</div>
        <div v-for="(f, i) in s.fetching" :key="i" class="kv kv--sm">in flight {{ o.hex(f.pc) }} {{ o.asm(f.instr) }} · arrives c{{ f.ready }}</div>
        <div v-if="!s.fetching.length" class="kv kv--sm dim">no fetch in flight</div>
      </div>

      <!-- ══ instruction queue ══ -->
      <div class="blk" :class="{ 'blk--focus': stageIs('Dispatch') }" :style="box(10, 158, 300, 166)">
        <div class="blk__head"><span>INSTRUCTION QUEUE</span><span class="blk__sub">{{ s.iq.length }} / 8 · oldest first</span></div>
        <div v-for="n in 8" :key="n" class="row" :class="{ 'row--free': !s.iq[n - 1] }">
          <template v-if="s.iq[n - 1]">
            <span class="c-pc">{{ o.hex(s.iq[n - 1]!.pc, 4) }}</span><span class="c-asm">{{ o.asm(s.iq[n - 1]!.instr) }}</span>
            <span v-if="s.iq[n - 1]!.pred.taken" class="tag tag--pred">→ {{ o.hex(s.iq[n - 1]!.pred.next, 4) }}</span>
          </template>
          <span v-else class="dim">—</span>
        </div>
      </div>

      <!-- ══ predictor ══ -->
      <div class="blk" :class="{ 'blk--on': has('Resolve') }" :style="box(10, 334, 300, 216)">
        <div class="blk__head"><span>BTB</span><span class="blk__sub">16 · PC → target</span></div>
        <div class="btb">
          <div v-for="(e, i) in s.btb" :key="i" class="btb__e" :class="{ 'btb__e--on': e.valid }">
            <span class="dim">{{ i.toString(16) }}</span>
            <template v-if="e.valid">{{ o.hex(e.pc, 4) }} → {{ o.hex(e.target, 4) }} <b>{{ e.jump ? 'J' : 'B' }}</b></template>
          </div>
        </div>
      </div>
      <div class="blk" :class="{ 'blk--on': has('Resolve') }" :style="box(10, 560, 300, 118)">
        <div class="blk__head"><span>BHT</span><span class="blk__sub">64 × 2-bit · ≥ 2 = taken</span></div>
        <div class="bht">
          <span v-for="(c, i) in s.bht" :key="i" class="bht__c" :class="'bht__c--' + c" :title="`entry ${i}: ${c}`">{{ c }}</span>
        </div>
      </div>

      <!-- ══ RAT + register file ══ -->
      <div class="blk" :class="{ 'blk--focus': stageIs('Dispatch') }" :style="box(330, 56, 300, 170)">
        <div class="blk__head"><span>RAT</span><span class="blk__sub">register → ROB tag (blank = in RF)</span></div>
        <div class="grid">
          <span v-for="r in 32" :key="r" class="g" :class="{ 'g--tag': s.rat[r - 1] != null, 'g--hl': ratHl.has(r - 1) }">
            <b>x{{ r - 1 }}</b> {{ s.rat[r - 1] != null ? 'ROB ' + s.rat[r - 1] : '' }}
          </span>
        </div>
      </div>
      <div class="blk" :class="{ 'blk--focus': stageIs('Commit') }" :style="box(330, 236, 300, 170)">
        <div class="blk__head"><span>REGISTER FILE</span><span class="blk__sub">written only at commit</span></div>
        <div class="grid">
          <span v-for="r in 32" :key="r" class="g" :class="{ 'g--hl': rfHl.has(r - 1) }"><b>x{{ r - 1 }}</b> {{ o.hex(s.rf[r - 1]!) }}</span>
        </div>
      </div>

      <!-- ══ stats + events ══ -->
      <div class="blk" :style="box(330, 416, 300, 104)">
        <div class="blk__head"><span>STATS</span><span class="blk__sub">IPC {{ ipc }}</span></div>
        <div class="kv kv--sm">cycles {{ s.stats.cycles }} · committed {{ s.stats.committed }} · traps {{ s.stats.traps }}</div>
        <div class="kv kv--sm">branches {{ s.stats.branches }} · mispredicts {{ s.stats.mispredicts }} · squashed {{ s.stats.squashed }}</div>
        <div class="kv kv--sm">loads {{ s.stats.loads }} (forwarded {{ s.stats.forwards }}) · stores {{ s.stats.stores }}</div>
        <div class="kv kv--sm">stalls: ROB {{ s.stats.stall_rob }} · RS {{ s.stats.stall_rs }} · serialize {{ s.stats.stall_serialize }} · CDB waits {{ s.stats.cdb_waits }}</div>
      </div>
      <div class="blk" :style="box(330, 530, 300, 130)">
        <div class="blk__head"><span>THIS CYCLE</span><span class="blk__sub">events by stage</span></div>
        <div class="evs">
          <div v-for="(x, i) in o.cycleEvents" :key="i" class="ev" :class="{ 'ev--cur': stageIndex(x.stage) === o.lastStage }">
            <span class="ev__st">{{ x.stage.slice(0, 4) }}</span>{{ describe(x.e) }}
          </div>
          <div v-if="!o.cycleEvents.length" class="dim">—</div>
        </div>
      </div>

      <!-- ══ reservation stations ══ -->
      <div class="pos" :style="box(660, 56)"><RsTable title="ALU RS" :entries="s.alu_rs" :size="4" :instr="robAsm" :dispatched="dispatched" :woken="woken" :focus="stageIs('Issue')" /></div>
      <div class="pos" :style="box(660, 176)"><RsTable title="MUL RS" :entries="s.mul_rs" :size="4" :instr="robAsm" :dispatched="dispatched" :woken="woken" :focus="stageIs('Issue')" /></div>
      <div class="pos" :style="box(660, 296)"><RsTable title="BRANCH RS" :entries="s.br_rs" :size="8" :instr="robAsm" :dispatched="dispatched" :woken="woken" :focus="stageIs('Issue')" /></div>
      <!-- load / store queue -->
      <div class="blk" :class="{ 'blk--focus': stageIs('Memory') }" :style="box(660, 488, 430, 122)">
        <div class="blk__head"><span>LOAD / STORE QUEUE</span><span class="blk__sub">{{ s.lsq.length }} / 4 · program order</span></div>
        <table class="t">
          <thead><tr><th>ROB</th><th>op</th><th>base</th><th>address</th><th>data</th><th>state</th></tr></thead>
          <tbody>
            <tr v-for="n in 4" :key="n" :class="lsqClass(s.lsq[n - 1])">
              <template v-if="s.lsq[n - 1]">
                <td class="c-tag">{{ s.lsq[n - 1]!.rob }}</td>
                <td>{{ s.lsq[n - 1]!.store ? 'store' : 'load' }} {{ s.lsq[n - 1]!.size }}B</td>
                <td><span :class="opClass(s.lsq[n - 1]!.base)">{{ opText(s.lsq[n - 1]!.base) }}</span></td>
                <td>{{ s.lsq[n - 1]!.addr != null ? o.hex(s.lsq[n - 1]!.addr!) : '—' }}</td>
                <td><span v-if="s.lsq[n - 1]!.store" :class="opClass(s.lsq[n - 1]!.data)">{{ opText(s.lsq[n - 1]!.data) }}</span></td>
                <td>{{ LSQ_STATE[s.lsq[n - 1]!.state] }}</td>
              </template>
              <td v-else colspan="6" class="dim">free</td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- ══ functional units ══ -->
      <div v-for="fu in [0, 1]" :key="'alu' + fu" class="blk fu" :class="{ 'fu--busy': unitOps('Alu', fu).length }" :style="box(1110, fu ? 112 : 56, 200, 48)">
        <div class="blk__head blk__head--fu"><span>ALU {{ fu }}</span><span class="blk__sub">1 cycle</span></div>
        <div v-for="x in unitOps('Alu', fu)" :key="x.seq" class="kv kv--sm">ROB {{ x.rob }} {{ robAsm(x.rob) }} → {{ o.hex(x.value) }}<span v-if="x.finish <= s.cycle" class="tag tag--cdb">waits CDB</span></div>
      </div>
      <div class="blk fu" :class="{ 'fu--busy': unitOps('Mul').length }" :style="box(1110, 176, 200, 100)">
        <div class="blk__head blk__head--fu"><span>MUL</span><span class="blk__sub">3 stages, pipelined</span></div>
        <div v-for="st in 3" :key="st" class="mul">
          <span class="mul__st">M{{ st }}</span>
          <template v-for="x in unitOps('Mul')" :key="x.seq"><span v-if="s.cycle - x.issued === st - 1 || (st === 3 && s.cycle - x.issued > 2)">ROB {{ x.rob }} {{ robAsm(x.rob) }}</span></template>
        </div>
      </div>
      <div class="blk fu" :class="{ 'fu--busy': unitOps('Br').length || has('Resolve'), 'fu--bad': mispredict }" :style="box(1110, 296, 200, 90)">
        <div class="blk__head blk__head--fu"><span>BRANCH UNIT</span><span class="blk__sub">1 cycle</span></div>
        <div v-for="x in unitOps('Br')" :key="x.seq" class="kv kv--sm">ROB {{ x.rob }} {{ robAsm(x.rob) }}</div>
        <div v-for="(e, i) in evs('Resolve')" :key="'rv' + i" class="kv kv--sm" :class="{ bad: e.mispredict }">
          {{ e.taken ? 'taken' : 'not taken' }} → {{ o.hex(e.next, 4) }} · {{ e.mispredict ? 'MISPREDICT' : 'predicted right' }}
        </div>
      </div>
      <div class="blk fu" :class="{ 'fu--busy': unitOps('Lsu').length || has('Address') }" :style="box(1110, 488, 200, 122)">
        <div class="blk__head blk__head--fu"><span>LOAD / STORE UNIT</span><span class="blk__sub">{{ s.caches ? 'L1D → L2' : '2-cycle memory' }}</span></div>
        <div v-for="(e, i) in evs('Address')" :key="'a' + i" class="kv kv--sm">address ROB {{ e.rob }} = {{ o.hex(e.addr) }}</div>
        <div v-for="x in unitOps('Lsu')" :key="x.seq" class="kv kv--sm">load ROB {{ x.rob }} → {{ o.hex(x.value) }} · {{ x.finish > s.cycle ? `${x.finish - s.cycle} cyc left` : 'done' }}</div>
        <div v-for="(e, i) in evs('Forward')" :key="'f' + i" class="kv kv--sm fwd">forwarded from store ROB {{ e.from_rob }}</div>
      </div>

      <!-- ══ CDB labels ══ -->
      <div v-for="b in [0, 1]" :key="'cdb' + b" class="cdb" :style="box(700, b ? 655 : 625)">
        <b>CDB {{ b }}</b>
        <template v-if="bus(b)">ROB {{ bus(b)!.rob }} = {{ o.hex(bus(b)!.value) }} → RS, LSQ, ROB</template>
        <span v-else class="dim">idle</span>
      </div>

      <!-- ══ reorder buffer (circular) ══ -->
      <div class="blk" :class="{ 'blk--focus': stageIs('Commit') || stageIs('Writeback') }" :style="box(1340, 56, 370, 570)">
        <div class="blk__head"><span>REORDER BUFFER</span><span class="blk__sub">{{ s.rob_len }} / 32 · head {{ s.rob_head }}</span></div>
        <table class="t t--rob">
          <thead><tr><th></th><th>#</th><th>instruction</th><th>rd</th><th>value</th><th>state</th></tr></thead>
          <tbody>
            <tr v-for="n in 32" :key="n" :class="robClass(n - 1)">
              <td class="c-ptr">{{ n - 1 === s.rob_head && s.rob_len ? 'H▶' : n - 1 === (s.rob_head + s.rob_len) % 32 ? 'T▶' : '' }}</td>
              <td class="c-tag">{{ n - 1 }}</td>
              <template v-if="rob(n - 1)">
                <td class="c-asm">{{ o.asm(rob(n - 1)!.instr) }}</td>
                <td>{{ rob(n - 1)!.d.rd ? 'x' + rob(n - 1)!.d.rd : '' }}</td>
                <td>{{ rob(n - 1)!.done && rob(n - 1)!.d.rd ? o.hex(rob(n - 1)!.value) : '' }}</td>
                <td>{{ robState(n - 1) }}</td>
              </template>
              <td v-else colspan="4"></td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- ══ timeline ══ -->
      <div class="blk" :style="box(10, 690, 1700, 240)">
        <div class="blk__head"><span>TIMELINE</span><span class="blk__sub">IF fetch · DI dispatch · wait (RS) · EX execute · WB broadcast · wait head · CM commit · grey = squashed</span></div>
        <div class="tl">
          <div class="tl__row tl__row--head">
            <span class="tl__ins"></span>
            <span v-for="c in cycles" :key="c" class="tl__c tl__c--h" :class="{ 'tl__c--now': c === shownCycle }">{{ c }}</span>
          </div>
          <div v-for="r in timeline" :key="r.seq" class="tl__row" :class="{ 'tl__row--sq': r.squashed }">
            <span class="tl__ins"><span class="dim">{{ o.hex(r.pc, 4) }}</span> {{ o.asm(r.instr) }}</span>
            <span v-for="c in cycles" :key="c" class="tl__c" :class="'k-' + cellKind(r, c)">{{ CELL[cellKind(r, c)] }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- ══ controls ══ -->
    <div class="bar">
      <button class="btn btn--nav" @click="emit('switch-cpu')">CPU</button>
      <span class="bar__div">|</span>
      <select class="bar__sel" :value="o.exampleIndex" @change="o.loadExample(+($event.target as HTMLSelectElement).value)">
        <option v-for="(e, i) in o.examples" :key="i" :value="i">{{ e.name }}</option>
      </select>
      <select class="bar__sel bar__sel--sm" :value="o.caches ? 1 : 0" @change="o.setCaches(($event.target as HTMLSelectElement).value === '1')">
        <option :value="0">memory: fixed 2 cycles</option>
        <option :value="1">memory: caches (L1 → L2)</option>
      </select>
      <button class="btn btn--stage" :disabled="!o.ready || o.running || !!s?.halted" title="run the next stage of the cycle" @click="o.stepStage()">Stage ▶</button>
      <button class="btn" :disabled="!o.ready || o.running || !!s?.halted" title="finish the cycle / one clock" @click="o.stepCycle()">Cycle ▶</button>
      <button class="btn" :disabled="!o.ready || !!s?.halted" @click="o.toggleRun()">{{ o.running ? 'Stop' : 'Run' }}</button>
      <button class="btn" :disabled="!o.ready || o.running || !!s?.halted" @click="o.runToEnd()">To end</button>
      <button class="btn btn--reset" :disabled="!o.ready" @click="o.loadExample(o.exampleIndex)">Reset</button>
      <input v-model.number="o.speed" type="range" min="50" max="1000" step="25" class="bar__speed" title="delay between cycles (ms)" />
      <span class="bar__lbl">{{ o.speed }} ms</span>
      <span class="bar__about">{{ o.examples[o.exampleIndex]?.about }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import MemWire from '../components/memory/MemWire.vue'
import RsTable from '../components/ooo/RsTable.vue'
import { useOoo, describe, STAGES, type OooEvent, type Operand, type LsqEntry, type Unit, type Stage, type Times } from '../stores/ooo'

const emit = defineEmits<{ 'switch-cpu': [] }>()
const o = useOoo()
onMounted(() => { o.init().catch(() => { /* shown in the panel */ }) })

const W = 1720, H = 940
const C = { fetch: '#0e7490', disp: '#0f766e', issue: '#2563eb', cdb: '#d97706', commit: '#15803d', flush: '#dc2626' }
const box = (x: number, y: number, w?: number, h?: number) => ({ left: `${x}px`, top: `${y}px`, ...(w ? { width: `${w}px` } : {}), ...(h ? { height: `${h}px` } : {}) })
const LSQ_STATE = { Address: 'needs address', Ready: 'address known', Memory: 'reading', Done: 'ready to commit' }

const s = computed(() => o.snap)
/** The cycle the shown events belong to (the snapshot's cycle counter moves on after stage 7) */
const shownCycle = computed(() => s.value ? (s.value.stage === 0 && o.lastStage === 6 ? s.value.cycle - 1 : s.value.cycle) : 0)
const stageIndex = (st: Stage) => STAGES.indexOf(st)
const stageIs = (st: Stage) => o.lastStage >= 0 && STAGES[o.lastStage] === st
const chipClass = (i: number) => ({ 'chip--done': o.lastStage >= 0 && i < o.lastStage, 'chip--cur': i === o.lastStage })
const stageCaption = computed(() => {
  if (o.lastStage < 0) return 'Stage ▶ runs one stage of the cycle at a time; Cycle ▶ a whole clock.'
  const st = STAGES[o.lastStage]!
  const evs = o.cycleEvents.filter(x => x.stage === st).map(x => describe(x.e))
  const what: Record<Stage, string> = {
    Commit: 'retire the ROB head if it is done (in order)', Resolve: 'branch unit results: train the predictor, flush on a mispredict',
    Memory: 'stores become ready; one load reads memory or takes a store\'s data', Issue: 'each unit takes its oldest ready RS entry',
    Writeback: 'up to 2 results on the CDBs, oldest first; waiting operands wake up', Dispatch: 'IQ head → ROB + RS / LSQ, sources through the RAT',
    Fetch: 'fetch along the predicted path',
  }
  return `${o.lastStage + 1} ${st.toUpperCase()} — ${what[st]}: ${evs.length ? evs.join(' · ') : 'nothing this cycle'}`
})

// ── events of this cycle ──
type Ev<K extends OooEvent['kind']> = Extract<OooEvent, { kind: K }>
const evs = <K extends OooEvent['kind']>(k: K) => o.cycleEvents.filter(x => x.e.kind === k).map(x => x.e as Ev<K>)
const has = (k: OooEvent['kind']) => o.cycleEvents.some(x => x.e.kind === k)
const bus = (b: number) => evs('Broadcast').find(e => e.bus === b) ?? null
const mispredict = computed(() => evs('Resolve').some(e => e.mispredict))
const dispatched = computed(() => new Set(evs('Dispatch').map(e => e.rob)))
const woken = computed(() => new Set(evs('Wakeup').map(e => e.rob)))
const issuedTo = (u: Unit, fu?: number) => evs('Issue').some(e => e.unit === u && (fu === undefined || e.fu === fu))
const dispatchTo = (y: number) => evs('Dispatch').some(e => ({ Alu: 80, Mul: 200, Br: 320, Lsu: 512, Sys: -1 })[e.unit] === y)
const ratHl = computed(() => new Set(evs('Dispatch').map(e => robE(e.rob)?.d.rd ?? 0).filter(r => r)))
const rfHl = computed(() => new Set(evs('Commit').map(e => e.rd).filter(r => r)))
/** Which unit a CDB result came from (by its row on the canvas) */
const cdbFrom = (y: number) => evs('Broadcast').some(e => {
  const x = s.value!.exec.find(q => q.rob === e.rob)
  const u = x?.unit ?? robE(e.rob)?.d.unit
  const fuY = u === 'Alu' ? (x?.fu ? 136 : 80) : u === 'Mul' ? 226 : u === 'Br' ? 340 : 540
  return fuY === y
})

// ── ROB ──
const robE = (tag: number) => s.value!.rob.find(r => r.tag === tag)?.e
const rob = (tag: number) => robE(tag) ?? null
const robAsm = (tag: number) => { const e = robE(tag); return e ? o.asm(e.instr) : '' }
function robState(tag: number): string {
  const e = robE(tag)!
  if (e.exc) return `trap ${e.exc[0]}`
  if (e.done) return e.d.unit === 'Sys' ? 'runs at commit' : 'done'
  const x = s.value!.exec.find(q => q.rob === tag)
  if (x) return x.finish > s.value!.cycle ? 'executing' : 'waits CDB'
  return e.t.issue != null ? 'in LSQ' : 'waiting'
}
function robClass(tag: number) {
  const e = robE(tag)
  if (!e) return 'r-free'
  return {
    'r-head': tag === s.value!.rob_head,
    'r-done': e.done,
    'r-new': dispatched.value.has(tag),
    'r-wb': evs('Broadcast').some(b => b.rob === tag),
    'r-exc': !!e.exc,
  }
}

// ── units ──
const unitOps = (u: Unit, fu?: number) => s.value!.exec.filter(x => x.unit === u && (fu === undefined || x.fu === fu))
const opText = (op: Operand) => op.tag != null ? `ROB ${op.tag}` : o.hex(op.value)
const opClass = (op: Operand) => op.tag != null ? 'opq' : ''
const lsqClass = (l?: LsqEntry) => !l ? 'r-free' : evs('Forward').some(e => e.rob === l.rob) ? 'r-fwd' : evs('MemRead').some(e => e.rob === l.rob) ? 'r-mem' : ''
const ipc = computed(() => s.value && s.value.stats.cycles ? (s.value.stats.committed / s.value.stats.cycles).toFixed(2) : '—')

// ── timeline ──
const cycles = computed(() => { const now = shownCycle.value; const from = Math.max(0, now - 23); return Array.from({ length: now - from + 1 }, (_, i) => from + i) })
const timeline = computed(() => {
  const inFlight = s.value!.rob.map(r => ({ seq: r.e.seq, pc: r.e.pc, instr: r.e.instr, t: r.e.t, squashed: false }))
  return [...s.value!.history, ...inFlight].sort((a, b) => a.seq - b.seq).slice(-12)
})
type CellKind = 'IF' | 'DI' | 'RS' | 'EX' | 'WB' | 'HD' | 'CM' | 'none'
const CELL: Record<CellKind, string> = { IF: 'IF', DI: 'DI', RS: 'wait', EX: 'EX', WB: 'WB', HD: 'head', CM: 'CM', none: '' }
function cellKind(r: { t: Times; squashed: boolean }, c: number): CellKind {
  const t = r.t
  const end = t.commit ?? (r.squashed ? Math.max(t.fetch, t.dispatch ?? 0, t.issue ?? 0, t.complete ?? 0) : shownCycle.value)
  if (c > end && !(t.commit === c)) return 'none'
  if (t.commit === c) return 'CM'
  if (c < t.fetch) return 'none'
  if (t.dispatch == null || c < t.dispatch) return 'IF'
  if (c === t.dispatch) return 'DI'
  if (t.issue == null || c < t.issue) return 'RS'
  if (t.complete == null || c < t.complete) return 'EX'
  if (c === t.complete) return 'WB'
  return 'HD'
}
</script>

<style scoped>
.ov { font-family: Inter, 'Segoe UI', Roboto, Arial, sans-serif; color: #1f2937; }
.ov__loading { padding: 40px; color: #4b5563; }
.canvas { position: relative; background: #fafafa; margin-bottom: 70px; }
.wires { position: absolute; top: 0; left: 0; pointer-events: none; z-index: 2; }
.blk, .pos, .cdb { position: absolute; z-index: 1; }
.blk { background: #fff; border: 1.5px solid #2aacbe; border-radius: 3px; overflow: hidden; }
.blk--on { box-shadow: 0 0 0 2px #93c5fd; }
.blk--focus { box-shadow: 0 0 0 3px #f59e0b; }
.blk__head { display: flex; align-items: baseline; gap: 8px; background: #2aacbe; color: #fff; padding: 2px 6px; font-size: 10px; font-weight: 700; letter-spacing: 1px; }
.blk__head--fu { background: #0e7490; }
.blk__sub { margin-left: auto; font-size: 9px; font-weight: 400; letter-spacing: 0; }
.dim { color: #6b7280; }
.bad { color: #dc2626; font-weight: 700; }
.fwd { color: #b45309; font-weight: 700; }
.kv { padding: 2px 6px; font-size: 11px; font-family: 'JetBrains Mono', monospace; }
.kv b { color: #0e7490; font-family: Inter, sans-serif; }
.kv--sm { font-size: 9px; padding: 1px 6px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tag { margin-left: 4px; padding: 0 4px; border-radius: 2px; font-size: 8.5px; font-weight: 700; }
.tag--pred { background: #dbeafe; color: #1d4ed8; }
.tag--cdb { background: #ffedd5; color: #c2410c; }

.strip { position: absolute; z-index: 3; background: #1e293b; border-radius: 4px; padding: 4px 8px; box-sizing: border-box; color: #e2e8f0; }
.strip__row { display: flex; align-items: center; gap: 4px; }
.strip__cyc { font-size: 11px; font-weight: 700; color: #fbbf24; margin-right: 6px; }
.strip__state { margin-left: 10px; font-size: 10px; color: #fca5a5; }
.chip { font-size: 9px; font-weight: 700; padding: 1px 6px; border-radius: 2px; background: #334155; color: #cbd5e1; }
.chip--done { background: #0f766e; color: #fff; }
.chip--cur { background: #f59e0b; color: #1f2937; }
.strip__text { font-size: 10.5px; color: #fde68a; margin-top: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

.row { display: flex; align-items: center; gap: 6px; padding: 0 6px; height: 17px; font-size: 9.5px; font-family: 'JetBrains Mono', monospace; border-top: 1px solid #f1f5f9; }
.row--free { background: #fafafa; }
.c-pc { color: #4b5563; }
.c-asm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.btb { display: grid; grid-template-columns: 1fr 1fr; gap: 0 8px; padding: 2px 6px; font-family: 'JetBrains Mono', monospace; font-size: 9px; }
.btb__e { height: 23px; display: flex; align-items: center; gap: 4px; color: #6b7280; border-top: 1px solid #f1f5f9; }
.btb__e--on { color: #1f2937; }
.btb__e b { color: #1d4ed8; }
.bht { display: grid; grid-template-columns: repeat(16, 1fr); gap: 2px; padding: 4px 6px; }
.bht__c { text-align: center; font-size: 9px; font-family: 'JetBrains Mono', monospace; border-radius: 2px; }
.bht__c--0 { background: #f1f5f9; color: #6b7280; }
.bht__c--1 { background: #e2e8f0; color: #374151; }
.bht__c--2 { background: #bbf7d0; color: #166534; }
.bht__c--3 { background: #16a34a; color: #fff; }
.grid { display: grid; grid-template-columns: repeat(4, 1fr); padding: 2px 4px; font-family: 'JetBrains Mono', monospace; font-size: 8.5px; }
.g { height: 18px; display: flex; align-items: center; gap: 3px; white-space: nowrap; border-top: 1px solid #f1f5f9; }
.g b { width: 22px; color: #0e7490; font-family: Inter, sans-serif; }
.g--tag { color: #92400e; font-weight: 700; }
.g--hl { background: #fef3c7; outline: 1px solid #f59e0b; }
.evs { padding: 2px 6px; overflow-y: auto; height: 108px; }
.ev { font-size: 9px; font-family: 'JetBrains Mono', monospace; color: #374151; white-space: nowrap; }
.ev--cur { color: #92400e; font-weight: 700; background: #fffbeb; }
.ev__st { display: inline-block; width: 34px; color: #6b7280; }

.t { width: 100%; border-collapse: collapse; font-size: 9px; font-family: 'JetBrains Mono', monospace; }
.t th { font-family: Inter, sans-serif; font-size: 8.5px; color: #374151; text-align: left; padding: 1px 4px; border-bottom: 1px solid #e5e7eb; }
.t td { padding: 0 4px; height: 15px; border-top: 1px solid #f1f5f9; white-space: nowrap; }
.t--rob td { height: 14.6px; }
.c-tag { color: #4b5563; font-weight: 700; width: 20px; }
.c-ptr { width: 18px; color: #15803d; font-weight: 700; }
.opq { background: #fef3c7; color: #92400e; font-weight: 700; padding: 0 3px; border-radius: 2px; }
.r-free td { background: #fafafa; }
.r-head td { background: #dcfce7; }
.r-done td:last-child { color: #15803d; font-weight: 700; }
.r-new td { background: #ecfeff; }
.r-wb td { background: #ffedd5; }
.r-exc td { color: #dc2626; }
.r-fwd td { background: #fef3c7; }
.r-mem td { background: #e0f2fe; }

.fu { border-color: #0e7490; }
.fu--busy { box-shadow: 0 0 0 2px #93c5fd; }
.fu--bad { box-shadow: 0 0 0 3px #fca5a5; border-color: #dc2626; }
.mul { display: flex; gap: 6px; padding: 1px 6px; font-size: 9px; font-family: 'JetBrains Mono', monospace; height: 17px; align-items: center; border-top: 1px solid #f1f5f9; }
.mul__st { width: 18px; color: #0e7490; font-weight: 700; }
.cdb { font-size: 10px; font-family: 'JetBrains Mono', monospace; color: #92400e; white-space: nowrap; }
.cdb b { margin-right: 6px; }

.tl { padding: 2px 6px; font-family: 'JetBrains Mono', monospace; font-size: 9px; }
.tl__row { display: flex; align-items: center; height: 16px; }
.tl__row--sq { opacity: 0.45; text-decoration: line-through; }
.tl__ins { width: 260px; flex: none; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tl__c { width: 56px; flex: none; text-align: center; margin-right: 2px; border-radius: 2px; font-weight: 700; font-size: 8.5px; }
.tl__c--h { color: #374151; font-weight: 700; }
.tl__c--now { background: #fef3c7; }
.k-IF { background: #94a3b8; color: #fff; }
.k-DI { background: #0f766e; color: #fff; }
.k-RS { background: #fde68a; color: #374151; }
.k-EX { background: #0891b2; color: #fff; }
.k-WB { background: #d97706; color: #fff; }
.k-HD { background: #e2e8f0; color: #374151; }
.k-CM { background: #15803d; color: #fff; }

.bar { position: fixed; bottom: 0; left: 0; right: 0; z-index: 10; display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; padding: 6px 12px; background: #1a1a2e; border-top: 1px solid #333; }
.btn { font: inherit; font-size: 11px; font-weight: 700; letter-spacing: 1px; text-transform: uppercase; padding: 5px 10px; border: 1px solid #555; border-radius: 3px; background: #2a2a3e; color: #ddd; cursor: pointer; }
.btn:disabled { opacity: 0.4; cursor: default; }
.btn--nav { border-color: #7c7cff; color: #b8b8ff; }
.btn--stage { border-color: #f59e0b; color: #fde68a; }
.btn--reset { border-color: #c44; color: #f88; }
.bar__div { color: #555; }
.bar__sel { font: inherit; font-size: 11px; padding: 4px; background: #2a2a3e; color: #ddd; border: 1px solid #555; max-width: 260px; }
.bar__sel--sm { max-width: 200px; }
.bar__speed { width: 80px; }
.bar__lbl { font-size: 10px; color: #d1d5db; }
.bar__about { flex-basis: 100%; font-size: 10.5px; color: #cbd5e1; }
</style>
