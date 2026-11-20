<script setup lang="ts">
import { ref } from 'vue'
import CpuLayout from './components/CpuLayout.vue'
import MulticycleLayout from './components/MulticycleLayout.vue'
import { useCpuConfig } from './stores/cpuConfig'
import EditorView from './views/EditorView.vue'
import PipelineView from './views/PipelineView.vue'
import MicroStepView from './views/MicroStepView.vue'
import MicroStepPipelineView from './views/MicroStepPipelineView.vue'
import MicroStepBarrelView from './views/MicroStepBarrelView.vue'

const config = useCpuConfig()

const activeTab = ref<'cpu' | 'editor' | 'pipeline' | 'microstep' | 'microstep-pipeline' | 'microstep-barrel'>('cpu')
</script>

<template>
  <div class="app">
    <div v-show="activeTab === 'cpu'">
      <!-- Config → Multicycle picks which datapath panel is shown -->
      <MulticycleLayout
        v-if="config.multicycle"
        @switch-tab="activeTab = 'editor'"
        @switch-pipeline="activeTab = 'pipeline'"
        @switch-microstep="activeTab = 'microstep'"
        @switch-microstep-pipeline="activeTab = 'microstep-pipeline'"
        @switch-microstep-barrel="activeTab = 'microstep-barrel'"
      />
      <CpuLayout
        v-else
        @switch-tab="activeTab = 'editor'"
        @switch-pipeline="activeTab = 'pipeline'"
        @switch-microstep="activeTab = 'microstep'"
        @switch-microstep-pipeline="activeTab = 'microstep-pipeline'"
        @switch-microstep-barrel="activeTab = 'microstep-barrel'"
      />
    </div>
    <div v-show="activeTab === 'editor'">
      <EditorView :visible="activeTab === 'editor'" @switch-tab="activeTab = 'cpu'" />
    </div>
    <div v-show="activeTab === 'pipeline'">
      <PipelineView @switch-tab="activeTab = 'cpu'" />
    </div>
    <div v-if="activeTab === 'microstep'">
      <MicroStepView @switch-cpu="activeTab = 'cpu'" />
    </div>
    <div v-if="activeTab === 'microstep-pipeline'">
      <MicroStepPipelineView @switch-cpu="activeTab = 'cpu'" />
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
