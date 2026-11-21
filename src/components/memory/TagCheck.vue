<template>
  <div class="tc" :class="{ 'tc--focus': focus, 'tc--l2': l2 }">
    <div class="tc__head"><span>TAG CHECK</span><span class="tc__sub">{{ title }}</span></div>
    <!-- where the address comes from -->
    <div class="tc__in">
      <template v-if="l2">PA ← bus (the L1 miss)</template>
      <template v-else><b class="c-tag">tag</b> ← MMU PA · <b class="c-idx">index</b>, <b class="c-off">offset</b> ← VA</template>
    </div>

    <!-- 1. split -->
    <div class="tc__row" :class="rowCls(1)">
      <span class="n">1</span>
      <template v-if="on(1)">
        <span class="pa">{{ hex(lookup!.pa) }}</span>→
        <span class="f f--tag">{{ hex(lookup!.tag, tagDigits) }}</span>
        <span class="f f--idx">{{ lookup!.set }}</span>
        <span class="f f--off">{{ lookup!.pa & 0xf }}</span>
      </template>
      <span v-else class="dim">PA → tag | index | offset</span>
    </div>
    <!-- 2. set select -->
    <div class="tc__row" :class="rowCls(2)">
      <span class="n">2</span>
      <template v-if="on(2)">index <b class="c-idx">{{ lookup!.set }}</b> → read set {{ lookup!.set }} ▶ ({{ lookup!.ways.length }} ways)</template>
      <span v-else class="dim">index selects the set</span>
    </div>
    <!-- 3. comparators, one per way -->
    <div class="tc__cmp" :class="rowCls(3)">
      <span class="n">3</span>
      <div class="cmps">
        <div v-for="(w, i) in (lookup?.ways ?? blank)" :key="i" class="cmp" :class="on(3) ? (w.hit ? 'cmp--hit' : w.valid ? 'cmp--ne' : 'cmp--inv') : ''">
          <span class="cmp__w">w{{ i }}</span>
          <span class="cmp__eq">=</span>
          <template v-if="on(3)">
            <span class="cmp__t">{{ w.valid ? hex(w.tag, tagDigits) : '———————' }}</span>
            <span class="cmp__v">V{{ w.valid ? 1 : 0 }}</span>
            <span class="cmp__r">{{ w.hit ? '✓' : '✗' }}</span>
          </template>
          <span v-else class="dim">way tag ◀ set</span>
        </div>
      </div>
    </div>
    <!-- 4. result -->
    <div class="tc__row" :class="rowCls(4)">
      <span class="n">4</span>
      <template v-if="on(4)">
        <span v-if="hitWay !== null" class="res res--hit">OR → HIT way {{ hitWay }}</span>
        <span v-else class="res res--miss">OR → MISS → {{ missTo }}</span>
      </template>
      <span v-else class="dim">any way valid ∧ equal → hit</span>
    </div>
    <!-- 5. word select -->
    <div class="tc__row" :class="rowCls(4)">
      <span class="n">5</span>
      <template v-if="on(4) && hitWay !== null">offset {{ lookup!.pa & 0xf }} → word {{ (lookup!.pa >> 2) & 3 }} → <b class="word">{{ word !== null ? hex(word) : '' }}</b></template>
      <template v-else-if="on(4)"><span class="dim">data comes back with the fill</span></template>
      <span v-else class="dim">offset picks the word</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { LookupEv } from '../../stores/memPanel'

/**
 * Where a cache's tags come from: the address split into tag / index /
 * offset (1), the index reading out one set (2), one comparator per way
 * (3), OR → hit / miss (4), and the offset picking the word (5).
 * `stage` = how many of these steps are revealed (0–4); `cur` = the one
 * being shown now.
 */
const props = defineProps<{
  title: string
  lookup: LookupEv | null
  stage: number
  cur: number | null
  ways: number
  sets: number
  word: number | null
  missTo: string
  l2?: boolean
  focus?: boolean
}>()

const hex = (v: number, w = 8) => (v >>> 0).toString(16).padStart(w, '0')
const tagDigits = computed(() => Math.ceil((32 - 4 - Math.log2(props.sets)) / 4))
const blank = computed(() => Array.from({ length: props.ways }, () => ({ valid: false, tag: 0, hit: false })))
const on = (n: number) => !!props.lookup && props.stage >= n
const rowCls = (n: number) => ({ 'tc--cur': props.cur === n, 'tc--on': on(n) })
const hitWay = computed(() => { const i = props.lookup?.ways.findIndex(w => w.hit) ?? -1; return i >= 0 ? i : null })
</script>

<style scoped>
.tc { width: 230px; background: #fff; border: 1.5px solid #64748b; border-radius: 3px; overflow: hidden; font-family: 'JetBrains Mono', monospace; font-size: 8px; color: #1f2937; }
.tc--focus { box-shadow: 0 0 0 3px #f59e0b; }
.tc__head { display: flex; align-items: baseline; gap: 6px; background: #475569; color: #fff; padding: 2px 6px; font-size: 10px; font-weight: 700; letter-spacing: 1px; }
.tc__sub { margin-left: auto; font-size: 8px; font-weight: 400; letter-spacing: 0; font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; }
.tc__in { padding: 2px 6px; font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; color: #374151; background: #f8fafc; border-bottom: 1px solid #e5e7eb; }
.tc__row, .tc__cmp { display: flex; align-items: center; gap: 4px; padding: 1px 5px; min-height: 15px; border-left: 3px solid transparent; }
.tc__cmp { align-items: flex-start; }
.tc--cur { border-left-color: #f59e0b; background: #fffbeb; }
.n { width: 9px; color: #6b7280; font-weight: 700; }
.tc--on .n { color: #1f2937; }
.dim { color: #6b7280; font-family: Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; }
.pa { font-weight: 700; }
.f { padding: 0 3px; border-radius: 2px; font-weight: 700; }
.f--tag, .c-tag { background: #e0e7ff; color: #3730a3; }
.f--idx, .c-idx { background: #fef3c7; color: #92400e; }
.f--off, .c-off { background: #f1f5f9; color: #334155; }
.c-tag, .c-idx, .c-off { padding: 0 2px; border-radius: 2px; }
.cmps { display: flex; flex-direction: column; gap: 1px; flex: 1; }
.cmp { display: flex; align-items: center; gap: 4px; padding: 0 3px; border-radius: 2px; }
.cmp__w { width: 14px; font-weight: 700; }
.cmp__eq { display: inline-flex; align-items: center; justify-content: center; width: 12px; height: 12px; border: 1.2px solid #64748b; border-radius: 50%; font-weight: 700; font-size: 8px; }
.cmp__t { font-weight: 700; }
.cmp__v { color: #374151; }
.cmp__r { margin-left: auto; font-weight: 700; }
.cmp--hit { background: #dcfce7; color: #166534; }
.cmp--hit .cmp__eq { border-color: #16a34a; background: #16a34a; color: #fff; }
.cmp--ne { background: #fee2e2; color: #991b1b; }
.cmp--ne .cmp__eq { border-color: #dc2626; }
.cmp--inv { background: #f3f4f6; color: #4b5563; }
.res { font-weight: 700; }
.res--hit { color: #15803d; }
.res--miss { color: #b91c1c; }
.word { background: #1e3a8a; color: #fff; padding: 0 3px; border-radius: 2px; }
</style>
