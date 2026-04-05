<template>
  <div class="editor">

    <!-- Header -->
    <div class="editor__header">
      <h1 class="editor__title">RISC-V Editor</h1>
      <select class="editor__select" v-model="language">
        <option value="asm">ASM</option>
        <option value="c">C</option>
        <option value="rust">Rust</option>
      </select>
      <button class="editor__btn" @click="loadAsmFile">Load ASM</button>
      <button class="editor__btn" @click="saveAsmFile">Save ASM</button>
      <button class="editor__btn" @click="saveHexFile" :disabled="!hexOutput.length">Save Hex</button>
      <button class="editor__btn editor__btn--back" @click="emit('switch-tab')">Back to CPU</button>

      <input ref="asmFileInput" type="file" accept=".asm,.s,.txt" style="display:none" @change="onAsmFileLoad" />
    </div>

    <!-- Three panes -->
    <div class="editor__panes">

      <!-- Left pane: C/Rust source OR Instruction Reference -->
      <div class="editor__pane" :class="language === 'asm' ? 'editor__pane--narrow' : 'editor__pane--wide'">

        <!-- C/Rust mode: source editor -->
        <template v-if="language !== 'asm'">
          <div class="editor__pane-header">
            <span class="editor__pane-label">{{ language === 'c' ? 'C' : 'Rust' }} Source</span>
            <button class="editor__btn" @click="compileSource" :disabled="compiling">
              {{ compiling ? 'Compiling...' : 'Compile → ASM' }}
            </button>
          </div>
          <textarea
            class="editor__textarea"
            v-model="sourceCode"
            :placeholder="language === 'c' ? cPlaceholder : rustPlaceholder"
            spellcheck="false"
          />
          <div v-if="compileErrors.length" class="editor__errors">
            <div v-for="(err, i) in compileErrors" :key="i" class="editor__error">{{ err }}</div>
          </div>
        </template>

        <!-- ASM mode: Grouped Instruction Reference -->
        <template v-else>
          <div class="editor__pane-header">
            <span class="editor__pane-label">Instructions</span>
          </div>
          <div class="editor__ref">
            <div
              v-for="group in instrGroups"
              :key="group.label"
              class="editor__ref-item"
              :class="{ 'editor__ref-item--active': selectedGroup === group.label }"
              draggable="true"
              @click="selectGroup(group.label)"
              @dragstart="onDragStart($event, group)"
            >
              <span class="editor__ref-label">{{ group.label }}</span>
              <span class="editor__ref-ops">{{ group.operands }}</span>
              <span class="editor__ref-fmt">{{ group.format }}</span>
            </div>
          </div>
        </template>

        <!-- Popup dialog for instruction detail -->
        <div v-if="popupGroup" class="editor__popup-overlay" @click.self="popupGroup = null">
          <div class="editor__popup">
            <div class="editor__popup-header">
              <span class="editor__popup-title">{{ popupGroup.label }}</span>
              <span class="editor__popup-format">{{ popupGroup.format }}</span>
              <button class="editor__popup-close" @click="popupGroup = null">X</button>
            </div>
            <div class="editor__popup-operands">
              {{ popupGroup.operands }}
            </div>
            <div class="editor__popup-variants">
              <div
                v-for="v in popupGroup.variants"
                :key="v.mnemonic"
                class="editor__popup-variant"
                @click="insertVariant(v.mnemonic, popupGroup)"
              >
                <span class="editor__popup-mnemonic">{{ v.mnemonic }}</span>
                <span class="editor__popup-desc">{{ v.description }}</span>
              </div>
            </div>
            <div class="editor__popup-example">
              <span class="editor__popup-example-label">Example:</span>
              <code>{{ popupGroup.example }}</code>
            </div>
          </div>
        </div>
      </div>

        <!-- Instruction wizard popup (on drop or variant click) -->
        <div v-if="wizardGroup" class="editor__wiz-overlay" @click.self="wizardCancel">
          <div class="editor__wiz">
            <div class="editor__wiz-header">
              <span class="editor__wiz-title">{{ wizardGroup.label }}</span>
              <span class="editor__wiz-fmt">{{ wizardGroup.format }}</span>
              <button class="editor__wiz-close" @click="wizardCancel">X</button>
            </div>

            <!-- Variant selector -->
            <div class="editor__wiz-section">
              <span class="editor__wiz-label">Instruction</span>
              <div class="editor__wiz-variants">
                <button
                  v-for="v in wizardGroup.variants"
                  :key="v.mnemonic"
                  class="editor__wiz-var"
                  :class="{ 'editor__wiz-var--active': wizardMnemonic === v.mnemonic }"
                  @click="wizardMnemonic = v.mnemonic"
                >{{ v.mnemonic }}</button>
              </div>
            </div>

            <!-- Arguments -->
            <div v-if="'RD' in wizardArgs" class="editor__wiz-section">
              <span class="editor__wiz-label">RD (destination)</span>
              <select class="editor__wiz-select" v-model="wizardArgs.RD">
                <option v-for="r in REGISTERS_8" :key="r" :value="r">{{ r }}</option>
              </select>
            </div>

            <div v-if="'RS1' in wizardArgs" class="editor__wiz-section">
              <span class="editor__wiz-label">RS1 (source 1)</span>
              <select class="editor__wiz-select" v-model="wizardArgs.RS1">
                <option v-for="r in REGISTERS_8" :key="r" :value="r">{{ r }}</option>
              </select>
            </div>

            <div v-if="'RS2' in wizardArgs" class="editor__wiz-section">
              <span class="editor__wiz-label">RS2 (source 2)</span>
              <select class="editor__wiz-select" v-model="wizardArgs.RS2">
                <option v-for="r in REGISTERS_8" :key="r" :value="r">{{ r }}</option>
              </select>
            </div>

            <div v-if="'IMM' in wizardArgs" class="editor__wiz-section">
              <span class="editor__wiz-label">{{ wizardGroup.operands.includes('SHAMT') ? 'SHAMT (0-31)' : 'IMM (immediate)' }}</span>
              <input class="editor__wiz-input" v-model="wizardArgs.IMM" placeholder="0" />
            </div>

            <div v-if="'OFFSET' in wizardArgs" class="editor__wiz-section">
              <span class="editor__wiz-label">OFFSET</span>
              <input class="editor__wiz-input" v-model="wizardArgs.OFFSET" placeholder="0" />
            </div>

            <div v-if="'LABEL' in wizardArgs" class="editor__wiz-section">
              <span class="editor__wiz-label">LABEL</span>
              <input class="editor__wiz-input" v-model="wizardArgs.LABEL" placeholder="LOOP" />
            </div>

            <!-- Preview -->
            <div class="editor__wiz-preview">
              <span class="editor__wiz-label">Preview</span>
              <code class="editor__wiz-code">{{ wizardPreview }}</code>
            </div>

            <div class="editor__wiz-footer">
              <button class="editor__btn editor__btn--load" @click="wizardInsert">Insert</button>
              <button class="editor__btn" @click="wizardCancel">Cancel</button>
            </div>
          </div>
        </div>

      <!-- Middle: Assembly editor (two-column per line) -->
      <div class="editor__pane editor__pane--asm" @drop.prevent="onDrop" @dragover.prevent>
        <div class="editor__pane-header">
          <span class="editor__pane-label">RV32I Assembly</span>
          <button class="editor__btn" @click="addAsmLine">+ Line</button>
          <button class="editor__btn" @click="assembleCode">Assemble → Hex</button>
        </div>
        <div class="editor__asm-scroll">
          <div class="editor__asm-header">
            <span class="editor__asm-hdr-label">Label</span>
            <span class="editor__asm-hdr-instr">Instruction</span>
          </div>
          <div
            v-for="(line, i) in asmLines"
            :key="i"
            class="editor__asm-row"
          >
            <span class="editor__asm-linenum">{{ i + 1 }}</span>
            <input
              class="editor__asm-label"
              v-model="line.label"
              placeholder=""
              spellcheck="false"
              @keydown.tab.prevent="focusInstr(i)"
              @keydown.enter.prevent="addAsmLineAfter(i)"
              @keydown.backspace="onLabelBackspace($event, i)"
            />
            <input
              :ref="el => instrRefs[i] = el as HTMLInputElement"
              class="editor__asm-instr"
              v-model="line.instr"
              placeholder="NOP"
              spellcheck="false"
              @keydown.enter.prevent="addAsmLineAfter(i)"
              @keydown.backspace="onInstrBackspace($event, i)"
            />
            <button class="editor__asm-del" @click="removeAsmLine(i)" title="Delete line">×</button>
          </div>
        </div>
        <div v-if="asmErrors.length" class="editor__errors">
          <div v-for="(err, i) in asmErrors" :key="i" class="editor__error">{{ err }}</div>
        </div>
      </div>

      <!-- Right: Hex output -->
      <div class="editor__pane">
        <div class="editor__pane-header">
          <span class="editor__pane-label">Machine Code</span>
          <button class="editor__btn editor__btn--load" @click="loadIntoCpu" :disabled="!hexOutput.length">
            Load into CPU
          </button>
        </div>
        <div class="editor__hex-output">
          <table class="editor__hex-table" v-if="hexOutput.length">
            <thead>
              <tr>
                <th>Addr</th>
                <th>Hex</th>
                <th>Assembly</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="instr in hexOutput" :key="instr.address">
                <td class="editor__hex-addr">{{ fmtAddr(instr.address) }}</td>
                <td class="editor__hex-val">{{ instr.hex }}</td>
                <td class="editor__hex-asm">{{ instr.asm.toUpperCase() }}</td>
              </tr>
            </tbody>
          </table>
          <div v-else class="editor__hex-empty">No output yet</div>
        </div>
        <div class="editor__hex-footer" v-if="hexOutput.length">
          {{ hexOutput.length }} instruction{{ hexOutput.length > 1 ? 's' : '' }}
        </div>
      </div>

    </div>

  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, watch, nextTick } from 'vue'
import { useCpuState } from '../stores/cpuState'
import { useCpuPhases } from '../stores/cpuPhases'
import { compileWithGodbolt } from '../data/godbolt'
import { INSTR_GROUPS } from '../data/instrReference'
import type { InstrGroup } from '../data/instrReference'

const emit = defineEmits<{
  'switch-tab': []
}>()

const cpu = useCpuState()
const phases = useCpuPhases()

// ── State ────────────────────────────────────────────────────────────────────

const language = ref<'asm' | 'c' | 'rust'>('asm')
const sourceCode = ref('')

interface AsmLine { label: string; instr: string }
const asmLines = ref<AsmLine[]>([
  { label: '', instr: '' },
])
const instrRefs: (HTMLInputElement | null)[] = []

/** Convert asmLines to a single string for the assembler */
const asmCode = computed(() =>
  asmLines.value.map(l => {
    const label = l.label.trim()
    const instr = l.instr.trim()
    if (label && instr) return `${label}: ${instr}`
    if (label) return `${label}:`
    return instr
  }).filter(l => l).join('\n')
)
const compiling = ref(false)
const compileErrors = ref<string[]>([])
const asmErrors = ref<string[]>([])
const selectedGroup = ref<string | null>(null)
const popupGroup = ref<InstrGroup | null>(null)
const instrGroups = INSTR_GROUPS

// Drop wizard state
const wizardGroup = ref<InstrGroup | null>(null)
const wizardMnemonic = ref('')
const wizardArgs = ref<Record<string, string>>({})

const REGISTERS_8 = ['X0', 'X1', 'X2', 'X3', 'X4', 'X5', 'X6', 'X7']

interface HexInstr {
  address: number
  value: number
  hex: string
  asm: string
}
const hexOutput = ref<HexInstr[]>([])

// ── WASM assembler ───────────────────────────────────────────────────────────

let wasmAssemble: ((source: string) => any) | null = null

const asmFileInput = ref<HTMLInputElement | null>(null)

function loadAsmFile() {
  asmFileInput.value?.click()
}

function onAsmFileLoad(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return
  const reader = new FileReader()
  reader.onload = () => {
    const text = reader.result as string
    parseAsmText(text)
    language.value = 'asm'
  }
  reader.readAsText(file)
  ;(event.target as HTMLInputElement).value = ''
}

function saveAsmFile() {
  const text = asmLines.value.map(l => {
    const label = l.label.trim()
    const instr = l.instr.trim()
    if (label && instr) return `${label.padEnd(10)}${instr}`
    if (label) return `${label}:`
    if (instr) return `          ${instr}`
    return ''
  }).filter(l => l).join('\n')

  const blob = new Blob([text + '\n'], { type: 'text/plain' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'program.asm'
  a.click()
  URL.revokeObjectURL(url)
}

function saveHexFile() {
  if (!hexOutput.value.length) return
  // Save as one hex value per line
  const text = hexOutput.value.map(i => i.hex).join('\n')
  const blob = new Blob([text + '\n'], { type: 'text/plain' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'program.hex'
  a.click()
  URL.revokeObjectURL(url)
}

function loadAsmFromStorage() {
  const saved = localStorage.getItem('cpuvis_asm')
  if (saved) {
    parseAsmText(saved)
  }
}

onMounted(async () => {
  loadAsmFromStorage()
  try {
    const wasm = await import('../../assembler-wasm/pkg/rv32i_assembler.js')
    await wasm.default()
    wasmAssemble = wasm.assemble
  } catch (err) {
    console.warn('WASM assembler not available', err)
  }
})

// Also check for new asm when becoming visible (tab switch)
const props = defineProps<{
  visible?: boolean
}>()

watch(() => props.visible, (val) => {
  if (val) loadAsmFromStorage()
})

// ── Instruction Reference (grouped) ──────────────────────────────────────────

function selectGroup(label: string) {
  const group = instrGroups.find(g => g.label === label)
  if (group) {
    selectedGroup.value = label
    popupGroup.value = group
  }
}

function onDragStart(event: DragEvent, group: InstrGroup) {
  event.dataTransfer?.setData('text/plain', group.example)
  event.dataTransfer!.effectAllowed = 'copy'
}

async function insertVariant(mnemonic: string, group: InstrGroup) {
  // Open wizard instead of inserting template directly
  popupGroup.value = null
  openWizard(group, mnemonic)
}

function onDrop(event: DragEvent) {
  const text = event.dataTransfer?.getData('text/plain')
  if (!text) return
  const group = instrGroups.find(g => g.example === text)
  if (group) {
    openWizard(group, group.variants[0].mnemonic)
  } else {
    asmLines.value.push({ label: '', instr: text.toUpperCase() })
  }
}

function openWizard(group: InstrGroup, mnemonic: string) {
  wizardGroup.value = group
  wizardMnemonic.value = mnemonic
  // Parse operands to set default args
  const args: Record<string, string> = {}
  const ops = group.operands
  if (ops.includes('RD')) args.RD = 'X3'
  if (ops.includes('RS1') && !ops.includes('OFFSET')) args.RS1 = 'X0'
  if (ops.includes('RS2')) args.RS2 = 'X0'
  if (ops.includes('IMM') || ops.includes('SHAMT')) args.IMM = '0'
  if (ops.includes('OFFSET')) { args.OFFSET = '0'; args.RS1 = 'X5' }
  if (ops.includes('LABEL')) args.LABEL = 'LOOP'
  wizardArgs.value = args
}

async function wizardInsert() {
  if (!wizardGroup.value) return
  const instr = buildInstrString()
  asmLines.value.push({ label: '', instr: instr.toUpperCase() })
  wizardGroup.value = null
}

function buildInstrString(): string {
  if (!wizardGroup.value) return ''
  const ops = wizardGroup.value.operands
  let s = wizardMnemonic.value

  if (ops === '(none)') {
    // no operands
  } else if (ops.includes('OFFSET')) {
    const reg = ops.startsWith('RS2') ? wizardArgs.value.RS2 || 'X4' : wizardArgs.value.RD || 'X3'
    s += ` ${reg}, ${wizardArgs.value.OFFSET || '0'}(${wizardArgs.value.RS1 || 'X5'})`
  } else if (ops.includes('LABEL') && ops.includes('RS1')) {
    s += ` ${wizardArgs.value.RS1 || 'X3'}, ${wizardArgs.value.RS2 || 'X0'}, ${wizardArgs.value.LABEL || 'LOOP'}`
  } else if (ops === 'LABEL') {
    s += ` ${wizardArgs.value.LABEL || 'LOOP'}`
  } else if (ops === 'RD, LABEL') {
    s += ` ${wizardArgs.value.RD || 'X1'}, ${wizardArgs.value.LABEL || 'FUNC'}`
  } else if (ops.includes('RS2')) {
    s += ` ${wizardArgs.value.RD || 'X3'}, ${wizardArgs.value.RS1 || 'X1'}, ${wizardArgs.value.RS2 || 'X2'}`
  } else if (ops.includes('IMM') || ops.includes('SHAMT')) {
    if (ops.includes('RS1')) {
      s += ` ${wizardArgs.value.RD || 'X3'}, ${wizardArgs.value.RS1 || 'X0'}, ${wizardArgs.value.IMM || '0'}`
    } else {
      s += ` ${wizardArgs.value.RD || 'X5'}, ${wizardArgs.value.IMM || '0'}`
    }
  } else if (ops.includes('RS/IMM')) {
    if (wizardMnemonic.value === 'MV') {
      s += ` ${wizardArgs.value.RD || 'X3'}, ${wizardArgs.value.RS1 || 'X4'}`
    } else {
      s += ` ${wizardArgs.value.RD || 'X3'}, ${wizardArgs.value.IMM || '42'}`
    }
  }
  return s
}

const wizardPreview = computed(() => buildInstrString().toUpperCase())

function wizardCancel() {
  wizardGroup.value = null
}

// ── Uppercase ASM ────────────────────────────────────────────────────────────

function addAsmLine() {
  asmLines.value.push({ label: '', instr: '' })
}

function addAsmLineAfter(i: number) {
  asmLines.value.splice(i + 1, 0, { label: '', instr: '' })
  nextTick(() => instrRefs[i + 1]?.focus())
}

function removeAsmLine(i: number) {
  if (asmLines.value.length <= 1) {
    asmLines.value[0] = { label: '', instr: '' }
    return
  }
  asmLines.value.splice(i, 1)
}

function focusInstr(i: number) {
  nextTick(() => instrRefs[i]?.focus())
}

function onLabelBackspace(event: KeyboardEvent, i: number) {
  const line = asmLines.value[i]
  if (line.label === '' && line.instr === '' && asmLines.value.length > 1) {
    event.preventDefault()
    removeAsmLine(i)
  }
}

function onInstrBackspace(event: KeyboardEvent, i: number) {
  const line = asmLines.value[i]
  if (line.instr === '' && line.label === '' && asmLines.value.length > 1) {
    event.preventDefault()
    removeAsmLine(i)
  }
}

/** Parse a text string into asmLines */
function parseAsmText(text: string) {
  const lines = text.split('\n').filter(l => l.trim())
  if (lines.length === 0) {
    asmLines.value = [{ label: '', instr: '' }]
    return
  }
  asmLines.value = lines.map(line => {
    const trimmed = line.trim()
    const colonIdx = trimmed.indexOf(':')
    if (colonIdx > 0 && !trimmed.substring(0, colonIdx).includes(' ')) {
      return {
        label: trimmed.substring(0, colonIdx).toUpperCase(),
        instr: trimmed.substring(colonIdx + 1).trim().toUpperCase(),
      }
    }
    return { label: '', instr: trimmed.toUpperCase() }
  })
}

// ── Placeholders ─────────────────────────────────────────────────────────────

const cPlaceholder = `// RISC-V C program (freestanding)
#define LED_ADDR ((volatile int *)0x40020000)

void _start() {
    int x = 7;
    int y = x + 5;
    *LED_ADDR = y;
    while(1) {}
}`

const rustPlaceholder = `#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let x: i32 = 7;
    let y = x + 5;
    unsafe {
        core::ptr::write_volatile(0x40020000 as *mut i32, y);
    }
    loop {}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }`

// ── Compile Rust/C → ASM ────────────────────────────────────────────────────

async function compileSource() {
  if (language.value === 'asm') return
  compiling.value = true
  compileErrors.value = []

  const result = await compileWithGodbolt(sourceCode.value, language.value as 'c' | 'rust')

  if (result.success && result.asm) {
    parseAsmText(result.asm)
  }
  compileErrors.value = result.errors
  compiling.value = false
}

// ── Assemble ASM → Hex ──────────────────────────────────────────────────────

function assembleCode() {
  asmErrors.value = []
  hexOutput.value = []

  if (!wasmAssemble) {
    asmErrors.value = ['WASM assembler not loaded']
    return
  }

  // Assembler expects lowercase
  const result = wasmAssemble(asmCode.value.toLowerCase())

  if (result.success) {
    hexOutput.value = result.instructions
  }
  if (result.errors && result.errors.length) {
    asmErrors.value = result.errors.map(
      (e: { line: number; message: string }) => `Line ${e.line}: ${e.message}`
    )
  }
}

// ── Load into CPU ────────────────────────────────────────────────────────────

function loadIntoCpu() {
  if (!hexOutput.value.length) return
  cpu.loadProgramWithAsm(
    hexOutput.value.map(i => ({ value: i.value, asm: i.asm.toUpperCase() }))
  )
  phases.reset()
  emit('switch-tab')
}

function fmtAddr(addr: number): string {
  return '0x' + addr.toString(16).padStart(4, '0')
}
</script>

<style scoped>
.editor {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: #1a1a2e;
  color: #eee;
  font-family: 'JetBrains Mono', monospace;
}

/* Header */
.editor__header {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 8px 16px;
  border-bottom: 1px solid #333;
}
.editor__title {
  font-size: 14px;
  font-weight: 700;
  letter-spacing: 1px;
  margin: 0;
}

/* Buttons */
.editor__btn {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 4px 12px;
  border: 1px solid #555;
  border-radius: 3px;
  background: #2a2a4a;
  color: #ccc;
  cursor: pointer;
}
.editor__btn:hover { background: #3a3a6a; }
.editor__btn:disabled { opacity: 0.5; cursor: default; }
.editor__btn--back { margin-left: auto; }
.editor__btn--load { border-color: #4a8; color: #8fc; }
.editor__btn--load:hover { background: #1a3a2a; }
.editor__btn--insert { border-color: #88f; color: #aaf; margin-top: 6px; }
.editor__btn--insert:hover { background: #1a1a4a; }

.editor__select {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 3px 8px;
  background: #2a2a4a;
  border: 1px solid #555;
  border-radius: 3px;
  color: #eee;
}

/* Three panes */
.editor__panes {
  flex: 1;
  display: flex;
  gap: 1px;
  background: #333;
  overflow: hidden;
}

.editor__pane {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: #0a0a1a;
  overflow: hidden;
}
.editor__pane--wide { flex: 2; min-width: 580px; }
.editor__pane--narrow { flex: 0.8; min-width: 240px; max-width: 320px; }
.editor__pane--asm { flex: 2; min-width: 440px; }

.editor__pane-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-bottom: 1px solid #222;
  flex-shrink: 0;
}
.editor__pane-label {
  font-size: 11px;
  color: #888;
  font-weight: 600;
}

/* Textarea */
.editor__textarea {
  flex: 1;
  padding: 12px;
  background: transparent;
  color: #eee;
  border: none;
  font-family: 'JetBrains Mono', monospace;
  font-size: 12px;
  line-height: 1.5;
  resize: none;
  outline: none;
  overflow: auto;
}
.editor__textarea--asm {
  text-transform: uppercase;
}

/* Two-column ASM editor */
.editor__asm-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}
.editor__asm-header {
  display: flex;
  padding: 2px 8px 2px 28px;
  border-bottom: 1px solid #222;
}
.editor__asm-hdr-label {
  width: 80px;
  font-size: 8px;
  color: #555;
  text-transform: uppercase;
  letter-spacing: 1px;
  flex-shrink: 0;
}
.editor__asm-hdr-instr {
  font-size: 8px;
  color: #555;
  text-transform: uppercase;
  letter-spacing: 1px;
}

.editor__asm-row {
  display: flex;
  align-items: center;
  padding: 1px 4px;
  gap: 0;
}
.editor__asm-row:hover {
  background: #0f0f2a;
}
.editor__asm-linenum {
  width: 24px;
  font-size: 9px;
  color: #444;
  text-align: right;
  padding-right: 6px;
  flex-shrink: 0;
  user-select: none;
}
.editor__asm-label {
  width: 80px;
  flex-shrink: 0;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  color: #8fc;
  background: transparent;
  border: none;
  border-right: 1px solid #222;
  padding: 3px 4px;
  outline: none;
  text-transform: uppercase;
}
.editor__asm-label:focus {
  background: #1a2a1a;
}
.editor__asm-instr {
  flex: 1;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  color: #eee;
  background: transparent;
  border: none;
  padding: 3px 6px;
  outline: none;
  text-transform: uppercase;
}
.editor__asm-instr:focus {
  background: #0a0a2a;
}
.editor__asm-del {
  width: 20px;
  background: none;
  border: none;
  color: #444;
  font-size: 14px;
  cursor: pointer;
  padding: 0;
  line-height: 1;
  flex-shrink: 0;
}
.editor__asm-del:hover {
  color: #f66;
}

/* Errors */
.editor__errors {
  padding: 6px 12px;
  background: #1a0a0a;
  border-top: 1px solid #400;
  max-height: 80px;
  overflow-y: auto;
  flex-shrink: 0;
}
.editor__error {
  font-size: 10px;
  color: #f66;
  line-height: 1.4;
}

/* Instruction Reference (grouped) */
.editor__ref {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}
.editor__ref-item {
  display: flex;
  align-items: baseline;
  gap: 6px;
  padding: 4px 8px;
  cursor: grab;
  transition: background 0.1s;
  border-bottom: 1px solid #111;
}
.editor__ref-item:hover {
  background: #1a1a3a;
}
.editor__ref-item--active {
  background: #2a2a5a;
}
.editor__ref-label {
  font-size: 9px;
  font-weight: 700;
  color: #8fc;
  letter-spacing: 0.5px;
  flex-shrink: 0;
}
.editor__ref-ops {
  font-size: 8px;
  color: #888;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.editor__ref-fmt {
  font-size: 7px;
  color: #555;
  flex-shrink: 0;
}

/* Popup dialog */
.editor__popup-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
}
.editor__popup {
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 6px;
  min-width: 300px;
  max-width: 420px;
  font-family: 'JetBrains Mono', monospace;
  color: #eee;
}
.editor__popup-header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border-bottom: 1px solid #333;
}
.editor__popup-title {
  font-size: 14px;
  font-weight: 700;
  color: #8fc;
  letter-spacing: 1px;
}
.editor__popup-format {
  font-size: 10px;
  color: #888;
}
.editor__popup-close {
  margin-left: auto;
  background: none;
  border: none;
  color: #888;
  font-size: 14px;
  cursor: pointer;
  font-family: 'JetBrains Mono', monospace;
}
.editor__popup-close:hover { color: #fff; }
.editor__popup-operands {
  padding: 6px 14px;
  font-size: 11px;
  color: #aaa;
  border-bottom: 1px solid #222;
}
.editor__popup-variants {
  padding: 6px 0;
}
.editor__popup-variant {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 6px 14px;
  cursor: pointer;
  transition: background 0.1s;
}
.editor__popup-variant:hover {
  background: #2a2a5a;
}
.editor__popup-mnemonic {
  font-size: 12px;
  font-weight: 700;
  color: #f0c060;
  min-width: 50px;
}
.editor__popup-desc {
  font-size: 10px;
  color: #ccc;
}
.editor__popup-example {
  padding: 8px 14px;
  border-top: 1px solid #222;
  font-size: 10px;
  color: #888;
}
.editor__popup-example-label {
  color: #555;
  margin-right: 4px;
}
.editor__popup-example code {
  color: #f0c060;
}

/* Instruction wizard */
.editor__wiz-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  z-index: 300;
  display: flex;
  align-items: center;
  justify-content: center;
}
.editor__wiz {
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 6px;
  min-width: 320px;
  max-width: 440px;
  font-family: 'JetBrains Mono', monospace;
  color: #eee;
}
.editor__wiz-header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border-bottom: 1px solid #333;
}
.editor__wiz-title {
  font-size: 14px;
  font-weight: 700;
  color: #8fc;
  letter-spacing: 1px;
}
.editor__wiz-fmt {
  font-size: 10px;
  color: #888;
}
.editor__wiz-close {
  margin-left: auto;
  background: none;
  border: none;
  color: #888;
  font-size: 14px;
  cursor: pointer;
  font-family: 'JetBrains Mono', monospace;
}
.editor__wiz-close:hover { color: #fff; }

.editor__wiz-section {
  padding: 6px 14px;
  display: flex;
  align-items: center;
  gap: 10px;
}
.editor__wiz-label {
  font-size: 10px;
  color: #888;
  min-width: 100px;
  flex-shrink: 0;
}
.editor__wiz-variants {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.editor__wiz-var {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  font-weight: 700;
  padding: 4px 10px;
  border: 1px solid #444;
  border-radius: 3px;
  background: #2a2a4a;
  color: #ccc;
  cursor: pointer;
}
.editor__wiz-var:hover { background: #3a3a6a; }
.editor__wiz-var--active {
  background: #2a4a2a;
  border-color: #4a8;
  color: #8fc;
}

.editor__wiz-select {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 3px 8px;
  background: #2a2a4a;
  border: 1px solid #555;
  border-radius: 3px;
  color: #eee;
  min-width: 80px;
}
.editor__wiz-input {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 3px 8px;
  background: #2a2a4a;
  border: 1px solid #555;
  border-radius: 3px;
  color: #eee;
  width: 100px;
}
.editor__wiz-input:focus,
.editor__wiz-select:focus {
  outline: none;
  border-color: #0099cc;
}

.editor__wiz-preview {
  padding: 8px 14px;
  display: flex;
  align-items: center;
  gap: 10px;
  border-top: 1px solid #222;
  margin-top: 4px;
}
.editor__wiz-code {
  font-size: 12px;
  font-weight: 700;
  color: #f0c060;
  letter-spacing: 0.5px;
}

.editor__wiz-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 10px 14px;
  border-top: 1px solid #333;
}

/* Hex output */
.editor__hex-output {
  flex: 1;
  overflow: auto;
}
.editor__hex-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 11px;
}
.editor__hex-table th {
  padding: 4px 8px;
  text-align: left;
  color: #666;
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 1px;
  border-bottom: 1px solid #222;
  position: sticky;
  top: 0;
  background: #0a0a1a;
}
.editor__hex-table td { padding: 3px 8px; }
.editor__hex-addr { color: #666; }
.editor__hex-val { color: #8fc; font-weight: 600; }
.editor__hex-asm { color: #aaa; }
.editor__hex-empty {
  padding: 24px 12px;
  color: #555;
  font-size: 11px;
  text-align: center;
}
.editor__hex-footer {
  padding: 4px 12px;
  font-size: 10px;
  color: #666;
  border-top: 1px solid #222;
  flex-shrink: 0;
}
</style>
