<script setup lang="ts">
// Standalone teaching widget for IRQ delivery + MMU page-walk.
// Not wired to the wasm simulator — runs scripted "microsteps" so
// students can press a button and step through what happens.
//
// Toy MMU dimensions (small enough to display):
//   - 12-bit virtual address  = vpn1(4) | vpn0(4) | offset(4)
//   - 16-byte pages (offset 4 bits)
//   - 16 entries per page table level
//   - 12-bit physical address
//   - 16-bit PTE = ppn(8) | reserved(2) | flags(6: V R W X A D)

import { ref, computed } from 'vue'

// ── Toy MMU storage ──────────────────────────────────────────────────
const PTE_V = 1 << 0
const PTE_R = 1 << 1
const PTE_W = 1 << 2
const PTE_X = 1 << 3
const PTE_A = 1 << 4
const PTE_D = 1 << 5

function pte(ppn: number, flags: number): number {
  return ((ppn & 0xff) << 8) | (flags & 0x3f)
}

// Hand-built page tables. Layout:
//   satp.PPN = 0x10 → root table at phys 0x100
//   L1[0] → pointer to L0 table at PPN 0x20 (phys 0x200)
//   L1[1] → leaf "superpage" at PPN 0x30, V R W X
//   L1[2..15] → 0 (invalid)
//   L0[0] → leaf at PPN 0x40, V R W X (initial — A bit not yet set)
//   L0[5] → 0 (invalid: for fault demo)
//   L0[1..4,6..15] → leaf at PPN 0x50, V R W

interface ToyMmu {
  satp_ppn: number  // 8 bits
  ram: Map<number, number>  // phys-byte-addr → 16-bit PTE
  tlb: { valid: boolean; vpn: number; ppn: number; pte: number }[]
}

function makeMmu(): ToyMmu {
  const ram = new Map<number, number>()
  // Root at 0x100 (PPN 0x10, page size 16 → addr = ppn*16)
  ram.set(0x100 + 0*2, pte(0x20, PTE_V))  // L1[0] → pointer
  ram.set(0x100 + 1*2, pte(0x30, PTE_V|PTE_R|PTE_W|PTE_X))  // L1[1] = leaf
  // L0 table at 0x200
  ram.set(0x200 + 0*2, pte(0x40, PTE_V|PTE_R|PTE_W|PTE_X))  // L0[0] leaf, no A
  ram.set(0x200 + 5*2, 0)  // L0[5] invalid (fault demo)
  for (let i = 1; i < 16; i++) {
    if (i === 5) continue
    ram.set(0x200 + i*2, pte(0x50, PTE_V|PTE_R|PTE_W))
  }
  return {
    satp_ppn: 0x10,
    ram,
    tlb: Array.from({ length: 4 }, () => ({ valid: false, vpn: 0, ppn: 0, pte: 0 })),
  }
}

const mmu = ref<ToyMmu>(makeMmu())

// ── Walk demo state machine ──────────────────────────────────────────
type WalkStep =
  | 'idle' | 'decompose' | 'read_root' | 'read_l1'
  | 'leaf_or_pointer' | 'read_l0' | 'check_perm'
  | 'compose_phys' | 'install_tlb' | 'done' | 'fault'

interface WalkState {
  vaddr: number
  vpn1: number
  vpn0: number
  off: number
  pte1_addr: number
  pte1: number
  pte0_addr: number
  pte0: number
  leaf_level: 1 | 0 | null
  phys: number | null
  fault_reason: string | null
  step: WalkStep
  step_index: number
}

const walk = ref<WalkState>({
  vaddr: 0x015, vpn1: 0, vpn0: 0, off: 0,
  pte1_addr: 0, pte1: 0, pte0_addr: 0, pte0: 0,
  leaf_level: null, phys: null, fault_reason: null,
  step: 'idle', step_index: 0,
})

const vaddrInput = ref('0x015')
const vaddrPresets = [
  { label: '0x015 (L0 walk)', v: 0x015 },
  { label: '0x123 (superpage)', v: 0x123 },
  { label: '0x055 (page fault)', v: 0x055 },
]

function startWalk() {
  const v = parseInt(vaddrInput.value.replace(/^0x/, ''), 16) & 0xfff
  walk.value = {
    vaddr: v,
    vpn1: (v >> 8) & 0xf,
    vpn0: (v >> 4) & 0xf,
    off:  v & 0xf,
    pte1_addr: 0, pte1: 0, pte0_addr: 0, pte0: 0,
    leaf_level: null, phys: null, fault_reason: null,
    step: 'decompose', step_index: 0,
  }
}

function walkStep() {
  const w = walk.value
  switch (w.step) {
    case 'idle':
      return
    case 'decompose':
      w.pte1_addr = mmu.value.satp_ppn * 16 + w.vpn1 * 2
      w.step = 'read_root'
      break
    case 'read_root':
      w.step = 'read_l1'
      break
    case 'read_l1':
      w.pte1 = mmu.value.ram.get(w.pte1_addr) ?? 0
      w.step = 'leaf_or_pointer'
      break
    case 'leaf_or_pointer': {
      if (!(w.pte1 & PTE_V)) {
        w.fault_reason = 'L1 PTE invalid (V=0)'
        w.step = 'fault'
        break
      }
      const isLeaf = (w.pte1 & (PTE_R|PTE_W|PTE_X)) !== 0
      if (isLeaf) {
        w.leaf_level = 1
        w.step = 'check_perm'
      } else {
        const next_ppn = (w.pte1 >> 8) & 0xff
        w.pte0_addr = next_ppn * 16 + w.vpn0 * 2
        w.step = 'read_l0'
      }
      break
    }
    case 'read_l0':
      w.pte0 = mmu.value.ram.get(w.pte0_addr) ?? 0
      if (!(w.pte0 & PTE_V)) {
        w.fault_reason = 'L0 PTE invalid (V=0)'
        w.step = 'fault'
        break
      }
      if ((w.pte0 & (PTE_R|PTE_W|PTE_X)) === 0) {
        w.fault_reason = 'L0 PTE is a pointer (no leaf)'
        w.step = 'fault'
        break
      }
      w.leaf_level = 0
      w.step = 'check_perm'
      break
    case 'check_perm':
      // Toy demo: just check R is set (we model loads only).
      const leafPte = w.leaf_level === 1 ? w.pte1 : w.pte0
      if (!(leafPte & PTE_R)) {
        w.fault_reason = 'leaf PTE missing R bit'
        w.step = 'fault'
        break
      }
      w.step = 'compose_phys'
      break
    case 'compose_phys': {
      if (w.leaf_level === 1) {
        const ppn1 = (w.pte1 >> 12) & 0xf  // top 4 bits of PPN8
        // For toy: superpage covers 8-bit physical aperture; combine
        // ppn[high 4] with vaddr[low 8]
        w.phys = (ppn1 << 8) | (w.vaddr & 0xff)
      } else {
        const ppn = (w.pte0 >> 8) & 0xff
        w.phys = (ppn << 4) | w.off
      }
      // Set A bit on the leaf (write-back).
      if (w.leaf_level === 1) {
        const np = w.pte1 | PTE_A
        mmu.value.ram.set(w.pte1_addr, np)
        w.pte1 = np
      } else {
        const np = w.pte0 | PTE_A
        mmu.value.ram.set(w.pte0_addr, np)
        w.pte0 = np
      }
      w.step = 'install_tlb'
      break
    }
    case 'install_tlb': {
      const vpn = (w.vaddr >> 4) & 0xff
      const ppn = w.leaf_level === 1
        ? ((w.pte1 >> 8) & 0xff)
        : ((w.pte0 >> 8) & 0xff)
      const pteVal = w.leaf_level === 1 ? w.pte1 : w.pte0
      const slot = vpn % 4
      mmu.value.tlb[slot] = { valid: true, vpn, ppn, pte: pteVal }
      w.step = 'done'
      break
    }
    case 'done':
    case 'fault':
      return
  }
  w.step_index++
}

function walkReset() {
  mmu.value = makeMmu()
  walk.value.step = 'idle'
  walk.value.step_index = 0
  walk.value.phys = null
  walk.value.fault_reason = null
  walk.value.leaf_level = null
}

// ── IRQ demo state machine ───────────────────────────────────────────
type IrqStep =
  | 'idle' | 'assert_mip' | 'and_gate' | 'master_gate'
  | 'capture_epc_cause' | 'push_status' | 'set_priv'
  | 'redirect_pc' | 'done'

interface IrqState {
  mip: number      // 3-bit (M E I)
  mie: number      // 3-bit
  mstatus_mie: boolean
  mstatus_mpie: boolean
  mstatus_mpp: number
  mepc: number
  mcause: number
  mtvec: number
  pc: number
  priv: number     // 0/1/3
  step: IrqStep
  step_index: number
}

const irq = ref<IrqState>({
  mip: 0, mie: 0b100, mstatus_mie: true, mstatus_mpie: false, mstatus_mpp: 0,
  mepc: 0, mcause: 0, mtvec: 0x80, pc: 0x40, priv: 1,
  step: 'idle', step_index: 0,
})

function startIrq() {
  irq.value = {
    ...irq.value,
    mip: 0, mcause: 0, mepc: 0, pc: 0x40, priv: 1,
    mstatus_mie: true, mstatus_mpie: false, mstatus_mpp: 0,
    step: 'assert_mip', step_index: 0,
  }
}

function irqStep() {
  const i = irq.value
  switch (i.step) {
    case 'idle': return
    case 'assert_mip':
      i.mip |= (1 << 2)  // MEI
      i.step = 'and_gate'
      break
    case 'and_gate':
      i.step = 'master_gate'
      break
    case 'master_gate':
      i.step = 'capture_epc_cause'
      break
    case 'capture_epc_cause':
      i.mepc = i.pc
      i.mcause = 0x8000_000b | 0  // MEI
      i.step = 'push_status'
      break
    case 'push_status':
      i.mstatus_mpie = i.mstatus_mie
      i.mstatus_mpp = i.priv
      i.mstatus_mie = false
      i.step = 'set_priv'
      break
    case 'set_priv':
      i.priv = 3
      i.step = 'redirect_pc'
      break
    case 'redirect_pc':
      i.pc = i.mtvec
      i.step = 'done'
      break
    case 'done': return
  }
  i.step_index++
}

function irqReset() {
  irq.value.step = 'idle'
  irq.value.step_index = 0
  irq.value.mip = 0
  irq.value.mcause = 0
  irq.value.mepc = 0
  irq.value.pc = 0x40
  irq.value.priv = 1
  irq.value.mstatus_mie = true
  irq.value.mstatus_mpie = false
  irq.value.mstatus_mpp = 0
}

// ── Helpers ──────────────────────────────────────────────────────────
const hex = (n: number, w = 3) => '0x' + n.toString(16).padStart(w, '0')
const bit = (v: number, b: number) => ((v >> b) & 1) === 1

const walkActive = computed(() => walk.value.step !== 'idle')
const irqActive = computed(() => irq.value.step !== 'idle')

function pickPreset(v: number) {
  vaddrInput.value = hex(v)
}

function flagBox(pte: number, mask: number, label: string) {
  return { label, set: (pte & mask) !== 0 }
}
function flagsList(pte: number) {
  return [
    flagBox(pte, PTE_V, 'V'),
    flagBox(pte, PTE_R, 'R'),
    flagBox(pte, PTE_W, 'W'),
    flagBox(pte, PTE_X, 'X'),
    flagBox(pte, PTE_A, 'A'),
    flagBox(pte, PTE_D, 'D'),
  ]
}

const stepHints: Record<WalkStep, string> = {
  idle:           'press Trigger Walk to start',
  decompose:      'split vaddr into vpn1 | vpn0 | offset',
  read_root:      'compute L1 PTE address from satp + vpn1',
  read_l1:        'fetch the L1 PTE from RAM',
  leaf_or_pointer:'check V; leaf if any of R/W/X set',
  read_l0:        'fetch the L0 PTE',
  check_perm:     'check permission bits against access',
  compose_phys:   'assemble physical = ppn | offset; set A bit',
  install_tlb:    'cache the translation in the TLB',
  done:           'translation complete',
  fault:          'page fault — see reason',
}

const irqHints: Record<IrqStep, string> = {
  idle:              'press Trigger IRQ to start',
  assert_mip:        'external device asserts MEI in mip',
  and_gate:          'mip & mie selects which sources are pending',
  master_gate:       'mstatus.MIE gates pending → fire',
  capture_epc_cause: 'save mepc ← pc, mcause ← interrupt code',
  push_status:       'push: MPIE ← MIE; MPP ← priv; MIE ← 0',
  set_priv:          'priv ← M',
  redirect_pc:       'pc ← mtvec',
  done:              'now executing the trap handler',
}
</script>

<template>
  <div class="demo-root">
    <!-- ─── IRQ panel ─── -->
    <section class="panel irq">
      <header>
        <h3>IRQ delivery</h3>
        <div class="controls">
          <button @click="startIrq">Trigger IRQ</button>
          <button @click="irqStep" :disabled="!irqActive || irq.step==='done'">Step</button>
          <button @click="irqReset">Reset</button>
          <span class="step-num" v-if="irqActive">{{ irq.step_index }}/7</span>
        </div>
      </header>
      <div class="hint">{{ irqHints[irq.step] }}</div>

      <div class="irq-body">
        <div class="reg-row">
          <span class="lbl">mip</span>
          <span class="led" :class="{ on: bit(irq.mip,0) }">S</span>
          <span class="led" :class="{ on: bit(irq.mip,1) }">T</span>
          <span class="led" :class="{ on: bit(irq.mip,2) }">E</span>
        </div>
        <div class="reg-row">
          <span class="lbl">mie</span>
          <span class="led" :class="{ on: bit(irq.mie,0) }">S</span>
          <span class="led" :class="{ on: bit(irq.mie,1) }">T</span>
          <span class="led" :class="{ on: bit(irq.mie,2) }">E</span>
          <span class="gate" :class="{ on: (irq.mip & irq.mie & 7) !== 0 && irq.step !== 'idle' && irq.step !== 'assert_mip' }">AND</span>
        </div>
        <div class="reg-row">
          <span class="lbl">MIE gate</span>
          <span class="gate" :class="{ on: irq.mstatus_mie }">{{ irq.mstatus_mie ? 'OPEN' : 'CLOSED' }}</span>
        </div>
        <div class="csr-grid">
          <div :class="{ active: irq.step==='capture_epc_cause' || irq.mepc }">
            <span>mepc</span><b>{{ hex(irq.mepc, 4) }}</b>
          </div>
          <div :class="{ active: irq.step==='capture_epc_cause' || irq.mcause }">
            <span>mcause</span><b>{{ hex(irq.mcause, 8) }}</b>
          </div>
          <div :class="{ active: irq.step==='push_status' }">
            <span>MPIE</span><b>{{ irq.mstatus_mpie ? '1' : '0' }}</b>
          </div>
          <div :class="{ active: irq.step==='push_status' }">
            <span>MPP</span><b>{{ irq.mstatus_mpp }}</b>
          </div>
          <div :class="{ active: irq.step==='set_priv' }">
            <span>priv</span><b>{{ irq.priv === 3 ? 'M' : irq.priv === 1 ? 'S' : 'U' }}</b>
          </div>
          <div :class="{ active: irq.step==='redirect_pc' }">
            <span>pc</span><b>{{ hex(irq.pc, 4) }}</b>
          </div>
          <div>
            <span>mtvec</span><b>{{ hex(irq.mtvec, 4) }}</b>
          </div>
        </div>
      </div>
    </section>

    <!-- ─── MMU panel ─── -->
    <section class="panel mmu">
      <header>
        <h3>MMU page walk (toy 12-bit / 16-byte pages)</h3>
        <div class="controls">
          <input v-model="vaddrInput" class="vaddr-in" />
          <button @click="startWalk">Trigger Walk</button>
          <button @click="walkStep" :disabled="!walkActive || walk.step==='done' || walk.step==='fault'">Step</button>
          <button @click="walkReset">Reset</button>
          <span class="step-num" v-if="walkActive">{{ walk.step_index }}</span>
        </div>
      </header>
      <div class="presets">
        <span>presets:</span>
        <button v-for="p in vaddrPresets" :key="p.v" @click="pickPreset(p.v)">{{ p.label }}</button>
      </div>
      <div class="hint">{{ stepHints[walk.step] }}</div>

      <div class="mmu-body">
        <div class="vaddr-row" :class="{ active: walk.step==='decompose' }">
          <span class="lbl">vaddr {{ hex(walk.vaddr, 3) }}</span>
          <span class="bits">
            <span class="bf vpn1">vpn1: {{ walk.vpn1.toString(16) }}</span>
            <span class="bf vpn0">vpn0: {{ walk.vpn0.toString(16) }}</span>
            <span class="bf off">off: {{ walk.off.toString(16) }}</span>
          </span>
        </div>

        <div class="reg-row">
          <span class="lbl">satp</span>
          <span class="csr-cell">PPN={{ hex(mmu.satp_ppn, 2) }}</span>
        </div>

        <div class="pte-row" :class="{ active: walk.step==='read_l1' || walk.step==='leaf_or_pointer' }">
          <span class="lbl">L1 PTE @{{ hex(walk.pte1_addr, 3) }}</span>
          <span class="csr-cell">{{ hex(walk.pte1, 4) }}</span>
          <span class="flags">
            <span v-for="f in flagsList(walk.pte1)" :key="f.label" class="flag" :class="{ on: f.set }">{{ f.label }}</span>
          </span>
          <span v-if="walk.leaf_level === 1" class="badge">leaf</span>
        </div>

        <div class="pte-row" v-if="walk.leaf_level === 0 || walk.pte0_addr"
             :class="{ active: walk.step==='read_l0' }">
          <span class="lbl">L0 PTE @{{ hex(walk.pte0_addr, 3) }}</span>
          <span class="csr-cell">{{ hex(walk.pte0, 4) }}</span>
          <span class="flags">
            <span v-for="f in flagsList(walk.pte0)" :key="f.label" class="flag" :class="{ on: f.set }">{{ f.label }}</span>
          </span>
          <span v-if="walk.leaf_level === 0" class="badge">leaf</span>
        </div>

        <div class="result-row" v-if="walk.phys !== null"
             :class="{ active: walk.step==='compose_phys' }">
          <span class="lbl">phys</span>
          <b>{{ hex(walk.phys, 3) }}</b>
        </div>

        <div class="fault-row" v-if="walk.step === 'fault'">
          <span class="fault-stamp">PAGE FAULT</span>
          <span>{{ walk.fault_reason }}</span>
        </div>

        <div class="tlb-row">
          <span class="lbl">TLB</span>
          <span v-for="(e, i) in mmu.tlb" :key="i" class="tlb-slot"
                :class="{ valid: e.valid, hot: walk.step==='install_tlb' && ((walk.vaddr >> 4) & 0xff) % 4 === i }">
            <template v-if="e.valid">
              vpn={{ hex(e.vpn, 2) }}<br>ppn={{ hex(e.ppn, 2) }}
            </template>
            <template v-else>—</template>
          </span>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.demo-root {
  display: flex;
  gap: .5rem;
  width: 100%;
  height: 100%;
  padding: .4rem;
  box-sizing: border-box;
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  font-size: 11px;
  color: #ddd;
}

.panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  border-radius: 4px;
  padding: .4rem .55rem;
  overflow: auto;
  min-width: 0;
}
.panel.irq { background: #1a0f1f; border: 1px solid #6a3a8a; }
.panel.mmu { background: #0a1c1c; border: 1px solid #2a8080; }

header { display: flex; align-items: center; gap: .5rem; margin-bottom: .25rem; }
h3 { margin: 0; font-size: .8rem; }
.irq h3 { color: #c080e0; }
.mmu h3 { color: #4dd0d0; }

.controls { display: flex; gap: .25rem; align-items: center; margin-left: auto; }
.controls button {
  background: #222; color: #ddd; border: 1px solid #555;
  padding: 1px 6px; font-size: 10px; border-radius: 3px;
  cursor: pointer; font-family: inherit;
}
.controls button:disabled { opacity: 0.4; cursor: default; }
.irq .controls button { border-color: #6a3a8a; }
.mmu .controls button { border-color: #2a8080; }
.step-num { font-size: 10px; color: #888; padding: 0 4px; }

.vaddr-in {
  width: 5em; background: #000; color: #fff;
  border: 1px solid #2a8080; font-family: inherit;
  font-size: 10px; padding: 1px 4px;
}

.presets { font-size: 10px; color: #888; margin-bottom: .2rem; }
.presets button {
  background: #112; color: #aaa; border: 1px solid #333;
  font-size: 10px; padding: 0 4px; margin-left: 3px; cursor: pointer;
  font-family: inherit;
}

.hint {
  font-size: 10px; color: #aaa; font-style: italic;
  margin-bottom: .35rem; min-height: 1em;
}

.reg-row, .vaddr-row, .pte-row, .result-row, .fault-row, .tlb-row {
  display: flex; align-items: center; gap: .35rem;
  padding: 2px 0;
}
.lbl { color: #888; min-width: 5.5em; font-size: 10px; }

.led {
  display: inline-block; width: 16px; height: 16px;
  border-radius: 50%; background: #1a1a1a; border: 1px solid #444;
  text-align: center; line-height: 14px; font-size: 9px; color: #555;
}
.led.on { background: #c080e0; color: #000; box-shadow: 0 0 6px #c080e0; }

.gate {
  background: #222; border: 1px solid #555; padding: 1px 5px;
  border-radius: 3px; font-size: 10px; color: #888;
}
.gate.on { background: #c080e0; color: #000; border-color: #e0a0ff; }

.csr-grid {
  display: grid; grid-template-columns: 1fr 1fr; gap: 3px;
  margin-top: .3rem;
}
.csr-grid > div {
  display: flex; justify-content: space-between;
  background: #1a1020; border: 1px solid #4a2a5a;
  padding: 2px 6px; font-size: 10px;
}
.csr-grid > div.active { background: #4a2068; border-color: #c080e0; color: #fff; }
.csr-grid b { color: #fff; }

.bits { display: inline-flex; gap: 2px; }
.bf {
  background: #112020; border: 1px solid #2a8080;
  padding: 1px 5px; font-size: 10px; color: #4dd0d0;
}
.vaddr-row.active .bf { background: #1a4040; color: #fff; }

.csr-cell {
  background: #112020; border: 1px solid #2a8080;
  padding: 1px 5px; font-size: 10px; color: #4dd0d0;
}
.pte-row.active .csr-cell { background: #1a4040; color: #fff; }

.flags { display: inline-flex; gap: 1px; }
.flag {
  display: inline-block; width: 14px; height: 14px;
  background: #0a1818; border: 1px solid #2a4a4a;
  text-align: center; line-height: 12px; font-size: 9px;
  color: #335555;
}
.flag.on { background: #2a8080; color: #fff; border-color: #4dd0d0; }

.result-row b { color: #4dd0d0; font-size: 12px; }
.result-row.active b { color: #fff; }

.fault-row { color: #ff6666; }
.fault-stamp {
  background: #4a0000; color: #ff8080;
  border: 1px solid #ff4444; padding: 1px 6px;
  font-weight: bold; font-size: 10px;
}

.badge {
  background: #2a8080; color: #000; padding: 0 4px;
  font-size: 9px; border-radius: 2px; font-weight: bold;
}

.tlb-row { margin-top: .25rem; align-items: stretch; }
.tlb-slot {
  flex: 1; min-width: 0;
  background: #0a1818; border: 1px solid #2a4a4a;
  padding: 2px 4px; font-size: 9px; color: #335555;
  text-align: center; line-height: 1.1;
}
.tlb-slot.valid { background: #112828; color: #4dd0d0; border-color: #2a8080; }
.tlb-slot.hot { background: #2a8080; color: #fff; box-shadow: 0 0 6px #4dd0d0; }
</style>
