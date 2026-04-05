<template>
  <tr
    class="reg-row"
    :class="{
      'reg-row--write':  reg.active,
      'reg-row--readA':  reg.readA,
      'reg-row--readB':  reg.readB,
      'reg-row--stripe': stripe,
    }"
    :style="rowStyle"
    :title="def?.description"
  >
    <!-- ABI name + optional altName -->
    <td class="reg-row__name">
      <span class="reg-row__abi">{{ def?.name ?? reg.name }}</span>
      <span v-if="def?.altName" class="reg-row__alt">/{{ def.altName }}</span>
    </td>

    <!-- xN number -->
    <td class="reg-row__xnum">x{{ reg.xNum }}</td>

    <!-- Hex value (double-click to edit) -->
    <td class="reg-row__hex" @dblclick="startEdit" v-if="!editing">{{ hexValue }}</td>
    <td class="reg-row__hex" v-else>
      <input
        ref="editInput"
        class="reg-row__edit"
        :value="hexValue"
        @keydown.enter="commitEdit($event)"
        @keydown.escape="editing = false"
        @blur="commitEdit($event)"
        spellcheck="false"
      />
    </td>

    <!-- Decimal value -->
    <td class="reg-row__dec">{{ decValue }}</td>

    <!-- Activity dot -->
    <td class="reg-row__dot">
      <span
        v-if="reg.active || reg.readA || reg.readB"
        class="reg-row__dot-inner"
        :class="{
          'reg-row__dot-inner--write': reg.active,
          'reg-row__dot-inner--readA': reg.readA && !reg.active,
          'reg-row__dot-inner--readB': reg.readB && !reg.active,
        }"
      />
    </td>
  </tr>
</template>

<script setup lang="ts">
import { computed, ref, nextTick } from 'vue'
import type { Register } from '../types/cpu'
import type { RegisterDef } from '../data/registerData'
import { GROUP_COLORS } from '../data/registerData'

const props = defineProps<{
  reg:     Register
  def?:    RegisterDef
  stripe?: boolean
}>()

const editing = ref(false)
const editInput = ref<HTMLInputElement | null>(null)

function startEdit() {
  if (props.reg.xNum === 0) return // x0 is hardwired zero
  editing.value = true
  nextTick(() => {
    editInput.value?.select()
  })
}

function commitEdit(event: Event) {
  const val = (event.target as HTMLInputElement).value.trim()
  editing.value = false
  let num: number
  if (val.startsWith('0x') || val.startsWith('0X')) {
    num = parseInt(val, 16)
  } else {
    num = parseInt(val, 10)
  }
  if (!isNaN(num)) {
    props.reg.value = num >>> 0
  }
}

/** Group tint from registerData — only when not in an active state */
const rowStyle = computed(() => {
  if (props.reg.active || props.reg.readA || props.reg.readB) return {}
  const base = props.def ? GROUP_COLORS[props.def.group] ?? '#ffffff' : '#ffffff'
  return props.stripe
    ? { backgroundColor: base }          // stripe is already a light tint in CSS
    : { backgroundColor: base }
})

const decValue = computed(() => (props.reg.value | 0).toString(10))

const hexValue = computed(() =>
  '0x' + (props.reg.value >>> 0).toString(16).padStart(8, '0')
)
</script>

<style scoped>
.reg-row { transition: background-color 0.15s ease; }
.reg-row--stripe  { filter: brightness(0.96); }
.reg-row--write   { background-color: #fff3c4 !important; font-weight: 600; }
.reg-row--readA   { background-color: #c8eeff !important; }
.reg-row--readB   { background-color: #d4f0d4 !important; }

.reg-row__name  { padding: 2px 6px; white-space: nowrap; }
.reg-row__abi   { font-size: 11px; font-weight: 600; color: #0a2030;
                  font-family: 'JetBrains Mono','Courier New',monospace; }
.reg-row__alt   { font-size: 8px; color: #6a9ab0; margin-left: 2px;
                  font-family: 'JetBrains Mono','Courier New',monospace; }

.reg-row__xnum  { padding: 2px 4px; font-size: 9px; color: #6a9ab0;
                  font-family: 'JetBrains Mono','Courier New',monospace; }

.reg-row__hex   { padding: 2px 6px; font-size: 10px; color: #0a2030;
                  font-family: 'JetBrains Mono','Courier New',monospace;
                  text-align: left; }

.reg-row__dec   { padding: 2px 6px; font-size: 10px; color: #2a4a60;
                  font-family: 'JetBrains Mono','Courier New',monospace;
                  text-align: right; min-width: 80px; }

.reg-row__edit {
  font-family: 'JetBrains Mono','Courier New',monospace;
  font-size: 10px;
  width: 80px;
  padding: 1px 3px;
  border: 1px solid #38bdf8;
  border-radius: 2px;
  background: #fff;
  color: #0a2030;
  outline: none;
}

.reg-row__dot        { padding: 0 4px; width: 12px; }
.reg-row__dot-inner  { display: inline-block; width: 7px; height: 7px; border-radius: 50%; }
.reg-row__dot-inner--write { background: #f59e0b; }
.reg-row__dot-inner--readA { background: #38bdf8; }
.reg-row__dot-inner--readB { background: #22c55e; }
</style>
