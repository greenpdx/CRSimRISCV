<template>
  <div class="cpu-layout">

    <!-- ── SVG wiring layer ── -->
    <svg class="cpu-wires" :width="1600" :height="850">
      <!-- ── Control lines: decoder bottom → components ── -->

      <!-- regWrite → Register File -->
      <ControlLine
        net-id="ctrl_reg_write"
        :points="[{ x: 334, y: 105 }, { x: 334, y: 466 }]"
        :asserted="cpu.controlStates['ctrl_reg_write']?.asserted ?? false"
        :show-label="true"
      />

      <!-- immSel → Imm Generator -->
      <ControlLine
        net-id="ctrl_imm_sel"
        :points="[{ x: 402, y: 105 }, { x: 402, y: 255 }]"
        :asserted="busVal('ctrl_imm_sel') !== 5"
        :show-label="true"
      />

      <!-- ctrl_is_jalr → PC Sel MUX -->
      <ControlLine
        net-id="ctrl_is_jalr"
        :points="[{ x: 473, y: 105 }, { x: 473, y: 161 }]"
        :asserted="cpu.controlStates['ctrl_is_jalr']?.asserted ?? false"
        :show-label="true"
      />

      <!-- aluSrc → ALU Src MUX -->
      <ControlLine
        net-id="ctrl_alu_src"
        :points="[{ x: 634, y: 105 }, { x: 634, y: 528 }]"
        :asserted="cpu.controlStates['ctrl_alu_src']?.asserted ?? false"
        :show-label="true"
      />

      <!-- ctrl_offset_sel → +4/Imm MUX -->
      <ControlLine
        net-id="ctrl_offset_sel"
        :points="[{ x: 720, y: 105 }, { x: 720, y: 141 }]"
        :asserted="cpu.controlStates['ctrl_offset_sel']?.asserted ?? false"
        :show-label="true"
      />

      <!-- aluOp → ALU Control -->
      <ControlLine
        net-id="ctrl_alu_op"
        :points="[{ x: 853, y: 105 }, { x: 853, y: 327 }]"
        :asserted="busVal('ctrl_alu_op') !== 0"
        :show-label="true"
      />

      <!-- wbSel → WB Sel MUX -->
      <ControlLine
        net-id="ctrl_wb_sel"
        :points="[{ x: 993, y: 105 }, { x: 993, y: 390 }]"
        :asserted="busVal('ctrl_wb_sel') !== 0"
        :show-label="true"
      />

      <!-- ── Data buses ── -->

      <!-- PC addr → ROM: from PC left edge up-left to ROM addr port -->
      <BusLine
        net-id="pc_out"
        :points="[
          { x: 300, y: 198 },
          { x: 260, y: 198 },
          { x: 260, y: 55 },
          { x: 240, y: 55 },
        ]"
        :bits="32"
        :value="cpu.pc"
        :state="busActive('pc_out') ? 'active' : 'idle'"
        :show-label="true"
      />

      <!-- PC Sel (next_pc) → PC: straight from mux output to PC right side -->
      <BusLine
        net-id="next_pc"
        :points="[
          { x: 417, y: 198 },
          { x: 390, y: 198 },
        ]"
        :bits="32"
        :value="cpu.nextPc"
        :state="busActive('next_pc') ? 'active' : 'idle'"
        :show-label="true"
      />

      <!-- PC → Adder: down from PC bottom, right along bus, up into adder bottom -->
      <BusLine
        net-id="pc_out_adder"
        :points="[
          { x: 260, y: 198 },
          { x: 260, y: 244 },
          { x: 587, y: 244 },
          { x: 587, y: 199 },
        ]"
        :bits="32"
        :value="cpu.pc"
        :state="busActive('pc_out') ? 'active' : 'idle'"
        :show-label="false"
      />

       <!-- Adder output → MUX PC Sel input -->
      <BusLine
        net-id="pc_adder_result"
        :points="[
          { x: 568, y: 180 },
          { x: 530, y: 180 },
        ]"
        :bits="32"
        :value="busVal('pc_adder_result')"
        :state="busActive('pc_adder_result') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- Instruction bus branch: down to ImmGen -->
      <BusLine
        net-id="instr_immgen"
        :points="[
          { x: 280, y: 40 },
          { x: 280, y: 287 },
          { x: 355, y: 287 },
        ]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- ImmGen output trunk -->
      <BusLine
        net-id="immediate"
        :points="[
          { x: 465, y: 287 },
          { x: 565, y: 287 },
        ]"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="true"
      />

      <!-- ImmGen → ALU Src mux (imm input): down from trunk to mux -->
      <BusLine
        net-id="immediate_alusrc"
        :points="[
          { x: 565, y: 287 },
          { x: 565, y: 574 },
          { x: 578, y: 574 },
        ]"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- ImmGen → +4/Imm mux (imm input): right, up, short left into mux -->
      <BusLine
        net-id="immediate_pcoffset"
        :points="[
          { x: 565, y: 287 },
          { x: 786, y: 287 },
          { x: 786, y: 190 },
          { x: 776, y: 190 },
        ]"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- ALU Control → ALU -->
      <BusLine
        net-id="alu_control"
        :points="[
          { x: 850, y: 397 },
          { x: 850, y: 495 },
        ]"
        :bits="4"
        :value="busVal('alu_control')"
        :state="busActive('alu_control') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- ALU Src output → ALU B input -->
      <BusLine
        net-id="alu_b"
        :points="[
          { x: 690, y: 565 },
          { x: 776, y: 565 },
        ]"
        :bits="32"
        :value="busVal('alu_b')"
        :state="busActive('alu_b') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- WB Sel → RegFile write data -->
      <BusLine
        net-id="write_data"
        :points="[
          { x: 937, y: 435 },
          { x: 410, y: 435 },
          { x: 410, y: 466 },
        ]"
        :bits="32"
        :value="busVal('write_data')"
        :state="busActive('write_data') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- ALU result → WB Sel input 0 -->
      <BusLine
        net-id="alu_result_wb"
        :points="[
          { x: 936, y: 545 },
          { x: 1075, y: 545 },
          { x: 1075, y: 415 },
          { x: 1049, y: 415 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- LoadStore load data → WB Sel input 1 -->
      <BusLine
        net-id="mem_read_data"
        :points="[
          { x: 1130, y: 100 },
          { x: 1130, y: 435 },
          { x: 1049, y: 435 },
        ]"
        :bits="32"
        :value="busVal('mem_read_data')"
        :state="busActive('mem_read_data') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- PC Adder result → WB Sel input 2 (PC+4 for JAL/JALR) -->
      <BusLine
        net-id="pc_adder_result_wb"
        :points="[
          { x: 567, y: 180 },
          { x: 567, y: 260 },
          { x: 1060, y: 260 },
          { x: 1060, y: 455 },
          { x: 1049, y: 455 },
        ]"
        :bits="32"
        :value="busVal('pc_adder_result')"
        :state="busActive('pc_adder_result') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- LoadStore addr → RAM -->
      <BusLine
        net-id="ls_addr_ram"
        :points="[
          { x: 1200, y: 35 },
          { x: 1280, y: 35 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- LoadStore data → RAM -->
      <BusLine
        net-id="ls_data_ram"
        :points="[
          { x: 1200, y: 55 },
          { x: 1280, y: 55 },
        ]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- LoadStore addr → I/O -->
      <BusLine
        net-id="ls_addr_io"
        :points="[
          { x: 1200, y: 70 },
          { x: 1240, y: 70 },
          { x: 1240, y: 470 },
          { x: 1280, y: 470 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- LoadStore data → I/O -->
      <BusLine
        net-id="ls_data_io"
        :points="[
          { x: 1200, y: 85 },
          { x: 1250, y: 85 },
          { x: 1250, y: 485 },
          { x: 1280, y: 485 },
        ]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- RegFile rs1 → ALU A input -->
      <BusLine
        net-id="rs1_data"
        :points="[
          { x: 520, y: 515 },
          { x: 776, y: 515 },
        ]"
        :bits="32"
        :value="busVal('rs1_data')"
        :state="busActive('rs1_data') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- RegFile rs2 → ALU Src mux -->
      <BusLine
        net-id="rs2_data"
        :points="[
          { x: 520, y: 548 },
          { x: 578, y: 548 },
        ]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- Instruction bus branch: down to ALU Control -->
      <BusLine
        net-id="instr_aluctrl"
        :points="[
          { x: 280, y: 287 },
          { x: 280, y: 313 },
          { x: 820, y: 313 },
          { x: 820, y: 327 },
        ]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="false"
      />

      <!-- Instruction bus: ROM → Decoder -->
      <BusLine
        net-id="instr"
        :points="[
          { x: 250, y: 40 },
          { x: 300, y: 40 },
        ]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="true"
      />

       <!-- +4/Imm MUX output → Adder right input -->
      <BusLine
        net-id="pc_offset"
        :points="[
          { x: 665, y: 180 },
          { x: 608, y: 180 },
        ]"
        :bits="32"
        :value="busVal('pc_offset')"
        :state="busActive('pc_offset') ? 'active' : 'idle'"
        :show-label="false"
      />

    </svg>

    <!-- ── ROM (far left, tall) ── -->
    <div class="cpu-block" :style="px(20, 20)">
      <RomBlock
        :words="cpu.romWords"
        :pc="cpu.pc"
      />
    </div>

    <!-- ── PC chain (horizontal row: PC → MUX PC Sel → Adder → MUX +4/Imm) ── -->

    <div class="cpu-block" :style="px(300, 178)">
      <PcRegister
        :pc="cpu.pc"
        :next-pc="cpu.nextPc"
        :active="busActive('pc_out')"
      />
    </div>

    <div class="cpu-block" :style="px(417, 161)">
      <MuxTrapezoid
        label="PC Sel"
        output-label="New PC"
        :inputs="muxPcInputs"
        :select="isJalrSel"
        :active="busActive('next_pc')"
      />
    </div>

    <div class="cpu-block" :style="px(567, 159)">
      <PcAdder
        :pc="cpu.pc"
        :offset="busVal('pc_offset')"
        :active="busActive('pc_adder_result')"
      />
    </div>

    <div class="cpu-block" :style="px(664, 141)">
      <MuxTrapezoid
        label="+4/Imm"
        output-label="offset"
        :inputs="muxPcOffsetInputs"
        :select="offsetSel"
        :active="busActive('pc_offset')"
      />
    </div>

    <!-- ── Decoder / Control (top) ── -->
    <div class="cpu-block" :style="px(300, 20)">
      <DecoderControl
        :opcode="opcode"
        :active="busActive('instr')"
      />
    </div>

    <!-- ── Immediate Generator ── -->
    <div class="cpu-block" :style="px(357, 255)">
      <ImmGenerator
        :instruction="cpu.lastInstr"
        :imm-sel="immSel"
        :active="busActive('immediate')"
      />
    </div>

    <!-- ── ALU Control ── -->
    <div class="cpu-block" :style="px(803, 327)">
      <AluControlUnit
        :alu-op="aluOp"
        :funct3="funct3"
        :funct7b30="funct7b30"
        :is-lui="isLui"
        :active="busActive('alu_control')"
      />
    </div>

    <!-- ── Register File ── -->
    <div class="cpu-block" :style="px(300, 466)">
      <RegisterFile
        :registers="cpu.registers"
      />
    </div>

    <!-- ── MUX ALU Src B ── -->
    <div class="cpu-block" :style="px(578, 528)">
      <MuxTrapezoid
        label="ALU Src"
        output-label="ALU B"
        direction="right"
        :inputs="muxAluSrcInputs"
        :select="aluSrcSel"
        :active="busActive('alu_b')"
      />
    </div>

    <!-- ── ALU ── -->
    <div class="cpu-block" :style="px(776, 485)">
      <AluUnit
        :alu-control="busVal('alu_control')"
        :operand-a="busVal('rs1_data')"
        :operand-b="busVal('alu_b')"
        :active="busActive('alu_result')"
      />
    </div>

    <!-- ── MUX Write-Back ── -->
    <div class="cpu-block" :style="px(937, 390)">
      <MuxTrapezoid
        label="WB Sel"
        output-label="write data"
        :inputs="muxWbInputs"
        :select="wbSel"
        :active="busActive('write_data')"
      />
    </div>

    <!-- ── Load / Store ── -->
    <div class="cpu-block" :style="px(1060, 20)">
      <LoadStore
        :op="loadStoreOp"
        :address="busVal('alu_result')"
        :width="'w'"
        :load-sign="'signed'"
        :write-data="busVal('rs2_data')"
        :read-data="busVal('mem_read_data')"
      />
    </div>

    <!-- ── RAM (far right) ── -->
    <div class="cpu-block" :style="px(1280, 20)">
      <RamBlock
        :words="cpu.ramWords"
        :last-op="ramOp"
        :last-address="busVal('alu_result')"
        :last-value="ramLastValue"
      />
    </div>

    <!-- ── I/O Block (below RAM) ── -->
    <div class="cpu-block" :style="px(1280, 460)">
      <IoBlock
        :base-address="cpu.ioBaseAddress"
        :address="busVal('alu_result')"
        :data="cpu.ioData"
        :op="cpu.ioOp"
      />
    </div>

  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useCpuState } from '../stores/cpuState'
import type { ImmSel } from '../data/immData'

import RomBlock from './RomBlock.vue'
import RamBlock from './RamBlock.vue'
import RegisterFile from './RegisterFile.vue'
import PcRegister from './PcRegister.vue'
import PcAdder from './PcAdder.vue'
import AluUnit from './AluUnit.vue'
import AluControlUnit from './AluControlUnit.vue'
import DecoderControl from './DecoderControl.vue'
import ImmGenerator from './ImmGenerator.vue'
import LoadStore from './LoadStore.vue'
import IoBlock from './IoBlock.vue'
import MuxTrapezoid from './MuxTrapezoid.vue'
import BusLine from './BusLine.vue'
import ControlLine from './ControlLine.vue'

// ── Store ────────────────────────────────────────────────────────────────────

const cpu = useCpuState()

// ── Position helper (direct pixel coordinates) ──────────────────────────────

function px(x: number, y: number) {
  return { left: `${x}px`, top: `${y}px` }
}

// ── Bus / control helpers ────────────────────────────────────────────────────

function busVal(id: string): number {
  return cpu.busStates[id]?.value ?? 0
}

function busActive(id: string): boolean {
  return cpu.busStates[id]?.state === 'active'
}

// ── Decoded instruction fields ───────────────────────────────────────────────

const opcode   = computed(() => cpu.lastInstr & 0x7f)
const funct3   = computed(() => (cpu.lastInstr >> 12) & 0x07)
const funct7b30 = computed(() => ((cpu.lastInstr >> 30) & 0x1) as 0 | 1)
const isLui    = computed(() => opcode.value === 0b0110111)

// ── Control signal values ────────────────────────────────────────────────────

const aluOp    = computed(() => (busVal('ctrl_alu_op') as 0 | 1 | 2))
const immSel   = computed(() => busVal('ctrl_imm_sel') as ImmSel)
const aluSrcSel = computed(() => cpu.controlStates['ctrl_alu_src']?.asserted ? 1 : 0)
const wbSel    = computed(() => busVal('ctrl_wb_sel'))
const offsetSel = computed(() => cpu.controlStates['ctrl_offset_sel']?.asserted ? 1 : 0)
const isJalrSel = computed(() => cpu.controlStates['ctrl_is_jalr']?.asserted ? 1 : 0)

// ── MUX input arrays ─────────────────────────────────────────────────────────

const muxPcOffsetInputs = computed(() => [
  { name: '+4',  value: 4,                    bits: 32 },
  { name: 'imm', value: busVal('immediate'),  bits: 32 },
])

const muxPcInputs = computed(() => [
  { name: '4BR',  value: busVal('pc_adder_result'), bits: 32 },
  { name: 'JALR', value: busVal('jalr_target'),     bits: 32 },
])

const muxAluSrcInputs = computed(() => [
  { name: 'rs2',  value: busVal('rs2_data'),  bits: 32 },
  { name: 'imm',  value: busVal('immediate'), bits: 32 },
])

const muxWbInputs = computed(() => [
  { name: 'ALU',    value: busVal('alu_result'),       bits: 32 },
  { name: 'Mem',    value: busVal('mem_read_data'),    bits: 32 },
  { name: 'PC+4',   value: busVal('pc_adder_result'),  bits: 32 },
])

// ── Load / Store derived state ───────────────────────────────────────────────

const loadStoreOp = computed(() => {
  if (cpu.controlStates['ctrl_mem_read']?.asserted)  return 'load' as const
  if (cpu.controlStates['ctrl_mem_write']?.asserted) return 'store' as const
  return 'idle' as const
})

const ramOp = computed(() => {
  if (cpu.controlStates['ctrl_mem_read']?.asserted)  return 'read' as const
  if (cpu.controlStates['ctrl_mem_write']?.asserted) return 'write' as const
  return 'idle' as const
})

const ramLastValue = computed(() => {
  if (ramOp.value === 'read')  return busVal('mem_read_data')
  if (ramOp.value === 'write') return busVal('rs2_data')
  return 0
})
</script>

<style scoped>
.cpu-layout {
  position: relative;
  width: 1600px;
  height: 850px;
  font-family: 'JetBrains Mono', monospace;
  background: #fafafa;
  overflow: auto;
}

.cpu-wires {
  position: absolute;
  top: 0;
  left: 0;
  pointer-events: none;
  z-index: 2;
}

.cpu-block {
  position: absolute;
  z-index: 1;
}
</style>
