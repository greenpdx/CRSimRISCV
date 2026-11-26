<script setup lang="ts">
import { ref, watchEffect } from 'vue'
import CpuLayout from './components/CpuLayout.vue'
import MulticycleLayout from './components/MulticycleLayout.vue'
import { useCpuConfig } from './stores/cpuConfig'
import EditorView from './views/EditorView.vue'
import MicroStepView from './views/MicroStepView.vue'
import MicroStepPipelineView from './views/MicroStepPipelineView.vue'
import MicroStepBarrelView from './views/MicroStepBarrelView.vue'
import MemoryView from './views/MemoryView.vue'
import OooView from './views/OooView.vue'

const config = useCpuConfig()

const activeTab = ref<'cpu' | 'editor' | 'microstep' | 'microstep-pipeline' | 'microstep-barrel' | 'memory' | 'ooo'>('cpu')

/** Page background = the active view's own background, so nothing shows round it
    (base.css would otherwise make it black in dark mode, white in light mode) */
const PAGE_BG: Record<typeof activeTab.value, string> = {
  cpu: '#fafafa', memory: '#fafafa', ooo: '#fafafa',
  editor: '#1a1a2e',
  microstep: '#1e1e1e', 'microstep-pipeline': '#1e1e1e', 'microstep-barrel': '#1e1e1e',
}
watchEffect(() => { document.body.style.background = PAGE_BG[activeTab.value] })
</script>

<template>
  <div class="app">
    <div v-show="activeTab === 'cpu'">
      <!-- Config → Multicycle picks which datapath panel is shown -->
      <MulticycleLayout
        v-if="config.multicycle"
        @switch-tab="activeTab = 'editor'"
        @switch-pipeline="activeTab = 'microstep-pipeline'"
        @switch-memory="activeTab = 'memory'"
        @switch-ooo="activeTab = 'ooo'"
        @switch-microstep="activeTab = 'microstep'"
        @switch-microstep-pipeline="activeTab = 'microstep-pipeline'"
        @switch-microstep-barrel="activeTab = 'microstep-barrel'"
      />
      <CpuLayout
        v-else
        @switch-tab="activeTab = 'editor'"
        @switch-pipeline="activeTab = 'microstep-pipeline'"
        @switch-memory="activeTab = 'memory'"
        @switch-ooo="activeTab = 'ooo'"
        @switch-microstep="activeTab = 'microstep'"
        @switch-microstep-pipeline="activeTab = 'microstep-pipeline'"
        @switch-microstep-barrel="activeTab = 'microstep-barrel'"
      />
    </div>
    <div v-show="activeTab === 'editor'">
      <EditorView :visible="activeTab === 'editor'" @switch-tab="activeTab = 'cpu'" />
    </div>
    <div v-if="activeTab === 'microstep'">
      <MicroStepView @switch-cpu="activeTab = 'cpu'" />
    </div>
    <div v-if="activeTab === 'microstep-pipeline'">
      <MicroStepPipelineView @switch-cpu="activeTab = 'cpu'" @switch-memory="activeTab = 'memory'" @switch-ooo="activeTab = 'ooo'" />
    </div>
    <div v-if="activeTab === 'memory'">
      <MemoryView @switch-pipeline="activeTab = 'microstep-pipeline'" />
    </div>
    <div v-if="activeTab === 'ooo'">
      <OooView @switch-pipeline="activeTab = 'microstep-pipeline'" />
    </div>
    <div v-if="activeTab === 'microstep-barrel'">
      <MicroStepBarrelView @switch-cpu="activeTab = 'cpu'" />
    </div>
  </div>
</template>

<style scoped>
.app {
  margin: 0;
  padding: 0;
}
</style>
