<template>
  <div class="ct" :class="{ 'ct--active': active }">
    <div class="ct__head">
      <span class="ct__title">{{ title }}</span>
      <span class="ct__geo">{{ cache.sets }} sets × {{ cache.ways }} ways · 16 B lines</span>
    </div>
    <table class="ct__table">
      <tbody>
        <tr v-for="s in cache.sets" :key="s" :class="{ 'ct__set--miss': missSet(s - 1) }">
          <td class="ct__set">{{ s - 1 }}</td>
          <td
            v-for="w in cache.ways"
            :key="w"
            class="ct__line"
            :style="{ width: (cellWidth ?? 165) + 'px', maxWidth: (cellWidth ?? 165) + 'px' }"
            :class="[markClass(s - 1, w - 1), { 'ct__line--empty': line(s - 1, w - 1).state === 'I' }]"
            :title="lineTitle(s - 1, w - 1)"
          >
            <template v-if="line(s - 1, w - 1).state !== 'I'">
              <div class="ct__top">
                <span class="ct__state" :class="'ct__state--' + line(s - 1, w - 1).state">{{ line(s - 1, w - 1).state }}</span>
                <span class="ct__addr">{{ hex(line(s - 1, w - 1).addr, 4) }}</span>
                <span class="ct__lru">lru {{ line(s - 1, w - 1).lru }}</span>
                <span v-if="markText(s - 1, w - 1)" class="ct__mark">{{ markText(s - 1, w - 1) }}</span>
              </div>
              <div class="ct__words">
                <span v-for="(d, i) in line(s - 1, w - 1).data" :key="i">{{ d.toString(16) }}</span>
              </div>
            </template>
            <span v-else class="ct__inv">—</span>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
import type { MpCache, MpLine, LineMark } from '../../stores/memPanel'

/**
 * One cache as a set × way table. `marks` / `missSets` come from the
 * owning core's last access: keys are built with `keyOf(set, way)` and
 * `setKey(set)` so the same table works for L1I / L1D (per core) and L2.
 */
const props = defineProps<{
  title: string
  cache: MpCache
  marks: Map<string, LineMark>
  missSets: Set<string>
  keyOf: (set: number, way: number) => string
  setKey: (set: number) => string
  active?: boolean
  /** Line cell width in px (default 165) */
  cellWidth?: number
}>()

const hex = (v: number, w = 8) => (v >>> 0).toString(16).padStart(w, '0')
const line = (s: number, w: number): MpLine => props.cache.lines[s * props.cache.ways + w]!
const mark = (s: number, w: number) => props.marks.get(props.keyOf(s, w))
const missSet = (s: number) => props.missSets.has(props.setKey(s))
const markClass = (s: number, w: number) => { const m = mark(s, w); return m ? 'ct__line--' + m : '' }
const TEXT: Record<LineMark, string> = {
  hit: 'hit', 'fill-l2': 'fill', 'fill-mem': 'fill', evict: 'evicted', snoop: 'snooped', state: 'state', writeback: 'wb',
}
const markText = (s: number, w: number) => { const m = mark(s, w); return m ? TEXT[m] : '' }
const lineTitle = (s: number, w: number) => {
  const l = line(s, w)
  return l.state === 'I' ? `set ${s} way ${w}: invalid`
    : `set ${s} way ${w}: line 0x${hex(l.addr)} (${l.state}), LRU age ${l.lru}\n${l.data.map(d => '0x' + hex(d)).join(' ')}`
}
</script>

<style scoped>
.ct {
  background: #fff;
  border: 1.5px solid #2aacbe;
  border-radius: 3px;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  overflow: hidden;
}
.ct--active { box-shadow: 0 0 0 2px #f59e0b; }
.ct__head { display: flex; align-items: baseline; gap: 8px; background: #2aacbe; color: #fff; padding: 2px 6px; }
.ct__title { font-size: 10px; font-weight: 700; letter-spacing: 1px; }
.ct__geo { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; margin-left: auto; font-size: 8px; opacity: 0.85; }
.ct__table { border-collapse: collapse; table-layout: fixed; }
.ct__set { width: 14px; font-size: 8px; color: #4b5563; text-align: center; border-right: 1px solid #e5e7eb; }
.ct__set--miss .ct__set { background: #fee2e2; color: #dc2626; font-weight: 700; }
.ct__line { border: 1px solid #e5e7eb; padding: 1px 4px; height: 28px; vertical-align: top; transition: background 0.15s; overflow: hidden; }
.ct__line--empty { background: #fafafa; }
.ct__inv { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; font-size: 8px; color: #6b7280; }
.ct__top { display: flex; align-items: center; gap: 4px; font-size: 8px; }
.ct__state { font-weight: 700; padding: 0 3px; border-radius: 2px; color: #fff; background: #9ca3af; }
.ct__state--M { background: #dc2626; }
.ct__state--E { background: #2563eb; }
.ct__state--S { background: #16a34a; }
.ct__addr { color: #333; font-weight: 600; }
.ct__lru { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; color: #4b5563; font-size: 8px; }
.ct__mark { font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; margin-left: auto; font-size: 8px; font-weight: 700; text-transform: uppercase; }
.ct__words { display: flex; gap: 8px; font-size: 8px; color: #374151; }
/* marks from the last access */
.ct__line--hit { background: #dcfce7; outline: 2px solid #16a34a; outline-offset: -2px; }
.ct__line--hit .ct__mark { color: #16a34a; }
.ct__line--fill-l2 { background: #fef3c7; outline: 2px solid #d97706; outline-offset: -2px; }
.ct__line--fill-l2 .ct__mark { color: #d97706; }
.ct__line--fill-mem { background: #fee2e2; outline: 2px solid #dc2626; outline-offset: -2px; }
.ct__line--fill-mem .ct__mark { color: #dc2626; }
.ct__line--evict { background: #f3f4f6; outline: 2px dashed #6b7280; outline-offset: -2px; }
.ct__line--evict .ct__mark { color: #374151; }
.ct__line--snoop { background: #cffafe; outline: 2px solid #0891b2; outline-offset: -2px; }
.ct__line--snoop .ct__mark { color: #0891b2; }
.ct__line--state { outline: 2px solid #7c3aed; outline-offset: -2px; }
.ct__line--state .ct__mark { color: #7c3aed; }
.ct__line--writeback { background: #ffedd5; outline: 2px solid #ea580c; outline-offset: -2px; }
.ct__line--writeback .ct__mark { color: #ea580c; }
</style>
