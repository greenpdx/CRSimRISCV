<template>
  <DraggableLayout
    :init-components="initComps"
    :init-buses="initBuses"
    :init-frame-w="550"
    :init-frame-h="600"
    @back="emit('back')"
  >
    <!-- Slot for each component — matched by comp.id -->
    <template #rom>
      <RomBlock :words="romWords" :pc="0" />
    </template>

    <template #pc_reg>
      <PcRegister :pc="0" :next-pc="4" />
    </template>

    <template #adder>
      <PcAdder :pc="0" :offset="4" />
    </template>

    <template #pc_mux>
      <MuxTrapezoid
        label="PC Sel"
        output-label="next PC"
        :inputs="pcMuxInputs"
        :select="0"
      />
    </template>

    <template #if_id_reg>
      <div class="pipe-reg">
        <div class="pipe-reg__label">IF / ID</div>
        <div class="pipe-reg__signals">
          <div class="pipe-reg__sig">pc [31:0]</div>
          <div class="pipe-reg__sig">inst [31:0]</div>
        </div>
      </div>
    </template>
  </DraggableLayout>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import DraggableLayout from './DraggableLayout.vue'

const emit = defineEmits<{
  back: []
}>()
import RomBlock from '../RomBlock.vue'
import PcRegister from '../PcRegister.vue'
import PcAdder from '../PcAdder.vue'
import MuxTrapezoid from '../MuxTrapezoid.vue'

const romWords = computed(() =>
  Array.from({ length: 8 }, (_, i) => ({
    address: i * 4,
    value: 0x00000013,
    asm: 'nop',
    active: false,
  }))
)

const pcMuxInputs = computed(() => [
  { name: 'PC+4',   value: 4,  bits: 32 },
  { name: 'branch', value: 0,  bits: 32 },
  { name: 'JALR',   value: 0,  bits: 32 },
  { name: 'trap',   value: 0,  bits: 32 },
])

const initComps = [
  { id: 'rom',       label: 'ROM',         x: 150, y: -120 },
  { id: 'pc_reg',    label: 'PC Register', x: 250, y: 150 },
  { id: 'adder',     label: '+4 Adder',    x: 100, y: 250 },
  { id: 'pc_mux',    label: 'PC MUX',      x: 150, y: 380 },
  { id: 'if_id_reg', label: 'IF/ID',       x: 470, y: 30 },
]

const initBuses = [
  { id: 'mux→pc', color: '#009977',
    points: [{ x: 210, y: 380 }, { x: 210, y: 200 }] },
  { id: 'pc→rom', color: '#0099cc',
    points: [{ x: 280, y: 150 }, { x: 280, y: 10 }] },
  { id: 'pc→adder', color: '#009977',
    points: [{ x: 250, y: 180 }, { x: 140, y: 180 }, { x: 140, y: 250 }] },
  { id: 'adder→mux', color: '#009977',
    points: [{ x: 120, y: 300 }, { x: 120, y: 340 }, { x: 170, y: 340 }, { x: 170, y: 450 }] },
  { id: 'rom→ifid', color: '#0099cc',
    points: [{ x: 350, y: 0 }, { x: 440, y: 0 }, { x: 440, y: 100 }, { x: 480, y: 100 }] },
  { id: 'pc→ifid', color: '#009977',
    points: [{ x: 350, y: 170 }, { x: 480, y: 170 }] },
  { id: 'branch←ID', color: '#cc8800',
    points: [{ x: 190, y: 560 }, { x: 190, y: 450 }] },
  { id: 'jalr←EX', color: '#cc8800',
    points: [{ x: 230, y: 560 }, { x: 230, y: 450 }] },
  { id: 'trap←WB', color: '#cc2222',
    points: [{ x: 270, y: 560 }, { x: 270, y: 450 }] },
]
</script>

<style scoped>
.pipe-reg {
  background: #e8e8e8;
  border: 1.5px solid #666;
  border-radius: 3px;
  padding: 6px 8px;
  width: 50px;
  min-height: 200px;
  display: flex;
  flex-direction: column;
  align-items: center;
}
.pipe-reg__label {
  font-size: 9px;
  font-weight: 700;
  color: #666;
  letter-spacing: 1px;
  writing-mode: vertical-rl;
  transform: rotate(180deg);
  margin-bottom: 8px;
}
.pipe-reg__signals {
  display: flex;
  flex-direction: column;
  gap: 12px;
  writing-mode: vertical-rl;
  transform: rotate(180deg);
}
.pipe-reg__sig {
  font-size: 7px;
  color: #888;
  white-space: nowrap;
}
</style>
