<template>
  <div class="rs" :class="{ 'rs--focus': focus }">
    <div class="rs__head"><span>{{ title }}</span><span class="rs__sub">{{ entries.length }} / {{ size }}</span></div>
    <table class="rs__t">
      <thead><tr><th>ROB</th><th>instruction</th><th>Vj / Qj</th><th>Vk / Qk</th><th>ready</th></tr></thead>
      <tbody>
        <tr v-for="n in size" :key="n" :class="rowClass(entries[n - 1])">
          <template v-if="entries[n - 1]">
            <td class="c-tag">{{ entries[n - 1]!.rob }}</td>
            <td class="c-ins">{{ instr(entries[n - 1]!.rob) }}</td>
            <td><span :class="opClass(entries[n - 1]!.j)">{{ opText(entries[n - 1]!.j) }}</span></td>
            <td><span :class="opClass(entries[n - 1]!.k)">{{ opText(entries[n - 1]!.k) }}</span></td>
            <td>{{ ready(entries[n - 1]!) ? '✓' : '' }}</td>
          </template>
          <td v-else colspan="5" class="c-free">free</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
import type { Operand, RsEntry } from '../../stores/ooo'

/** One reservation station: per entry its ROB tag, instruction and two operands (value Vj/Vk, or the ROB tag Qj/Qk it waits for). */
const props = defineProps<{
  title: string
  entries: RsEntry[]
  size: number
  instr: (rob: number) => string
  /** ROB tags dispatched / woken this cycle */
  dispatched: Set<number>
  woken: Set<number>
  focus?: boolean
}>()

const hex = (v: number) => (v >>> 0).toString(16).padStart(8, '0')
const waits = (o: Operand) => o.tag !== null && o.tag !== undefined
const opText = (o: Operand) => waits(o) ? `ROB ${o.tag}` : hex(o.value)
const opClass = (o: Operand) => waits(o) ? 'op op--q' : 'op op--v'
const ready = (r: RsEntry) => !waits(r.j) && !waits(r.k)
const rowClass = (r?: RsEntry) => !r ? 'r-free' : props.woken.has(r.rob) ? 'r-woken' : props.dispatched.has(r.rob) ? 'r-new' : ready(r) ? 'r-ready' : ''
</script>

<style scoped>
.rs { width: 430px; background: #fff; border: 1.5px solid #2aacbe; border-radius: 3px; overflow: hidden; font-family: 'JetBrains Mono', monospace; }
.rs--focus { box-shadow: 0 0 0 3px #f59e0b; }
.rs__head { display: flex; background: #2aacbe; color: #fff; padding: 2px 6px; font-size: 10px; font-weight: 700; letter-spacing: 1px; }
.rs__sub { margin-left: auto; font-size: 9px; font-weight: 400; font-family: Inter, 'Segoe UI', Roboto, Arial, sans-serif; }
.rs__t { width: 100%; border-collapse: collapse; font-size: 9px; }
.rs__t th { font-family: Inter, 'Segoe UI', Roboto, Arial, sans-serif; font-size: 8.5px; color: #374151; text-align: left; padding: 1px 4px; border-bottom: 1px solid #e5e7eb; }
.rs__t td { padding: 1px 4px; height: 15px; border-top: 1px solid #f1f5f9; white-space: nowrap; }
.c-tag { width: 24px; color: #4b5563; font-weight: 700; }
.c-ins { max-width: 130px; overflow: hidden; text-overflow: ellipsis; }
.c-free { color: #6b7280; font-family: Inter, 'Segoe UI', Roboto, Arial, sans-serif; }
.op { padding: 0 3px; border-radius: 2px; }
.op--v { color: #1f2937; }
.op--q { background: #fef3c7; color: #92400e; font-weight: 700; }
.r-new td { background: #ecfeff; }
.r-woken td { background: #ffedd5; }
.r-ready td:last-child { color: #15803d; font-weight: 700; }
.r-free td { background: #fafafa; }
</style>
