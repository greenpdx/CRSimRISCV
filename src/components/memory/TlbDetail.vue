<template>
  <div class="tlb" :class="{ 'tlb--focus': focus }">
    <div class="tlb__head">
      <span>{{ title }}</span>
      <span class="tlb__geo">fully associative · 4 entries · ASID + G</span>
      <span v-if="status" class="tlb__status" :class="'tlb__status--' + status">{{ status }}</span>
    </div>
    <div class="tlb__body">
      <div class="tlb__left">
        <!-- what is being compared -->
        <div class="tlb__key" :class="{ 'tlb__key--on': !!lookup || !!flush }">
          <template v-if="flush">
            sfence.vma
            <span class="k k--vpn">{{ flush.va === null ? 'x0 (all VAs)' : 'VPN ' + fh(flush.va >>> 12, 5) }}</span>
            <span class="k k--asid">{{ flush.asid === null ? 'x0 (all ASIDs)' : 'ASID ' + flush.asid }}</span>
          </template>
          <template v-else-if="lookup">
            lookup <span class="k k--vpn">VPN {{ fh(lookup.va >>> 12, 5) }}</span><span class="k k--asid">ASID {{ lookup.asid }}</span>
          </template>
          <span v-else class="tlb__dim">—</span>
        </div>
        <table class="tlb__t">
          <thead>
            <tr class="tlb__grp">
              <th></th><th colspan="5">tag</th><th colspan="2">content (PTE)</th>
              <th v-if="flush" colspan="4" class="tlb__gh tlb__gh--flush">invalidate this entry? (Fig 7)</th>
              <th v-else colspan="4" class="tlb__gh">compare — all entries at once (Fig 6)</th>
            </tr>
            <tr>
              <th>#</th><th>V</th><th>ASID</th><th>VPN</th><th title="4 MiB superpage (level-1 leaf)">S</th><th>G</th>
              <th>PPN</th><th>D A G U X W R V</th>
              <template v-if="flush"><th>VA=</th><th>ASID=</th><th>¬G</th><th>INV</th></template>
              <template v-else><th>VPN=</th><th>ASID=</th><th>∨G</th><th>HIT</th></template>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(e, i) in entries" :key="i" :class="rowClass(i)">
              <td class="c-idx">{{ i }}</td>
              <td :class="e.valid ? 'b1' : 'b0'">{{ e.valid ? 1 : 0 }}</td>
              <template v-if="e.valid">
                <td>{{ e.asid }}</td>
                <td class="c-vpn">{{ e.is_super ? fh(e.vpn >>> 10, 3) + '·***' : fh(e.vpn, 5) }}</td>
                <td>{{ e.is_super ? 1 : 0 }}</td>
                <td :class="e.global ? 'b1' : 'b0'">{{ e.global ? 1 : 0 }}</td>
                <td class="c-ppn">{{ fh(e.ppn, 5) }}</td>
                <td class="c-flags"><span v-for="b in [7, 6, 5, 4, 3, 2, 1, 0]" :key="b" :class="(e.pte >> b) & 1 ? 'b1' : 'b0'">{{ (e.pte >> b) & 1 }}</span></td>
              </template>
              <template v-else><td colspan="6" class="tlb__dim">invalid</td></template>
              <!-- gates -->
              <template v-if="flush">
                <td :class="gate(flush.f[i]?.va_match, flush.f[i]?.valid)">{{ g(flush.f[i]?.va_match) }}</td>
                <td :class="gate(flush.f[i]?.asid_match, flush.f[i]?.valid)">{{ g(flush.f[i]?.asid_match) }}</td>
                <td :class="gate(!(flush.asid !== null && flush.f[i]?.global), flush.f[i]?.valid)">{{ flush.asid !== null && flush.f[i]?.global ? '0' : '1' }}</td>
                <td :class="flushDone && flush.f[i]?.inval ? 'g-inv' : flush.f[i]?.inval ? 'g-will' : 'g-off'">{{ flush.f[i]?.inval ? 1 : 0 }}</td>
              </template>
              <template v-else-if="cmp">
                <td :class="gate(cmp[i]?.vpn_match, cmp[i]?.valid)">{{ g(cmp[i]?.vpn_match) }}</td>
                <td :class="gate(cmp[i]?.asid_match, cmp[i]?.valid)">{{ g(cmp[i]?.asid_match) }}</td>
                <td :class="gate(cmp[i]?.global, cmp[i]?.valid)">{{ g(cmp[i]?.global) }}</td>
                <td :class="cmp[i]?.hit ? 'g-hit' : 'g-off'">{{ cmp[i]?.hit ? 1 : 0 }}</td>
              </template>
              <template v-else><td class="g-idle" colspan="4"></td></template>
            </tr>
          </tbody>
        </table>
        <div class="tlb__foot">
          <template v-if="flush">valid ∧ VA= ∧ ASID= ∧ ¬G → <b>INV</b>{{ flushDone ? ' — entries cleared' : '' }}</template>
          <template v-else-if="cmp">hit array <b>{{ cmp.map(c => c.hit ? 1 : 0).join('') }}</b> → OR → <b :class="hitIdx !== null ? 'f-hit' : 'f-miss'">{{ hitIdx !== null ? 'TLB hit, entry ' + hitIdx : 'TLB miss → PTW' }}</b></template>
          <template v-else-if="victim">victim → entry <b>{{ victim.idx }}</b> ({{ victim.free ? 'first invalid' : 'PLRU' }})</template>
          <span v-else class="tlb__dim">valid ∧ VPN= ∧ (ASID= ∨ G) → HIT</span>
        </div>
      </div>
      <PlruTree :bits="plru" :next="plruNext" :victim="victim && !victim.free ? victim.idx : null" :touched="touched ?? null" />
    </div>
  </div>
</template>

<script setup lang="ts">
import PlruTree from './PlruTree.vue'
import type { MpTlb, EntryCmp, EntryFlush } from '../../stores/memPanel'

/**
 * One ITLB / DTLB drawn as CVA6 Fig 6 (lookup: every entry's comparators
 * → hit array) or Fig 7 (SFENCE.VMA: every entry's invalidate gate).
 * The parent decides what the current phase reveals.
 */
const props = defineProps<{
  title: string
  entries: MpTlb[]
  plru: number
  plruNext: number | null
  lookup?: { va: number; asid: number } | null
  cmp?: EntryCmp[] | null
  hitIdx?: number | null
  flush?: { va: number | null; asid: number | null; f: EntryFlush[] } | null
  flushDone?: boolean
  victim?: { idx: number; free: boolean } | null
  fillIdx?: number | null
  touched?: number | null
  status?: string
  focus?: boolean
}>()

const fh = (n: number, w: number) => (n >>> 0).toString(16).padStart(w, '0')
const g = (b: boolean | undefined) => b ? '1' : '0'
/** Gate colour; `valid` = the entry's V bit when the compare ran (not after a fill) */
const gate = (b: boolean | undefined, valid: boolean | undefined) => !valid ? 'g-off' : b ? 'g-1' : 'g-0'
const rowClass = (i: number) => ({
  'r-hit': props.hitIdx === i,
  'r-fill': props.fillIdx === i,
  'r-victim': props.victim?.idx === i && props.fillIdx !== i,
  'r-inv': !!props.flush && !!props.flush.f[i]?.inval,
})
</script>

<style scoped>
.tlb { width: 560px; height: 160px; background: #fff; border: 1.5px solid #2aacbe; border-radius: 3px; overflow: hidden; font-family: 'JetBrains Mono', monospace; }
.tlb--focus { box-shadow: 0 0 0 3px #f59e0b; }
.tlb__head { display: flex; align-items: baseline; gap: 8px; background: #2aacbe; color: #fff; padding: 2px 6px; font-size: 10px; font-weight: 700; letter-spacing: 1px; }
.tlb__geo { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 8px; font-weight: 400; letter-spacing: 0; opacity: 0.9; }
.tlb__status { margin-left: auto; font-size: 8px; padding: 0 5px; border-radius: 2px; color: #fff; }
.tlb__status--HIT { background: #2563eb; }
.tlb__status--MISS { background: #7c3aed; }
.tlb__status--UPDATE { background: #7c3aed; }
.tlb__status--FLUSH { background: #0f766e; }
.tlb__status--FAULT { background: #dc2626; }
.tlb__body { display: flex; gap: 6px; padding: 2px 4px; }
.tlb__left { flex: 1; }
.tlb__key { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 8px; color: #4b5563; height: 13px; }
.tlb__key--on { color: #334155; }
.k { margin-left: 5px; padding: 0 4px; border-radius: 2px; font-weight: 700; }
.k--vpn { background: #e0e7ff; color: #3730a3; }
.k--asid { background: #fce7f3; color: #9d174d; }
.tlb__dim { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; color: #6b7280; }
.tlb__t { border-collapse: collapse; font-size: 8px; }
.tlb__t th { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-weight: 700; color: #475569; padding: 0 3px; text-align: center; white-space: nowrap; }
.tlb__grp th { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 8px; color: #4b5563; border-bottom: 1px solid #e2e8f0; }
.tlb__gh { color: #2563eb !important; }
.tlb__gh--flush { color: #0f766e !important; }
.tlb__t td { padding: 1px 3px; text-align: center; border-top: 1px solid #f1f5f9; white-space: nowrap; height: 14px; }
.c-idx { color: #4b5563; }
.c-vpn { color: #3730a3; font-weight: 700; }
.c-ppn { font-weight: 700; }
.c-flags span { display: inline-block; width: 8px; }
.b1 { color: #0f172a; font-weight: 700; }
.b0 { color: #6b7280; }
.g-idle { background: #f8fafc; }
.g-off { color: #6b7280; }
.g-1 { background: #dcfce7; color: #15803d; font-weight: 700; }
.g-0 { background: #fee2e2; color: #b91c1c; }
.g-hit { background: #2563eb; color: #fff; font-weight: 700; }
.g-will { background: #ccfbf1; color: #0f766e; font-weight: 700; }
.g-inv { background: #0f766e; color: #fff; font-weight: 700; }
.r-hit td { background-color: #dbeafe; }
.r-fill td { background-color: #ede9fe; }
.r-victim td { background-color: #f5f3ff; }
.r-inv td:not([class^='g-']) { text-decoration: line-through; color: #475569; }
.tlb__foot { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 8px; color: #475569; margin-top: 2px; }
.f-hit { color: #2563eb; }
.f-miss { color: #7c3aed; }
</style>
