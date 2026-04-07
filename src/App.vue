<script setup lang="ts">
import { ref } from 'vue'
import CpuLayout from './components/CpuLayout.vue'
import EditorView from './views/EditorView.vue'
import PipelineView from './views/PipelineView.vue'
import MicroStepView from './views/MicroStepView.vue'

const activeTab = ref<'cpu' | 'editor' | 'pipeline' | 'microstep'>('cpu')
</script>

<template>
  <div class="app">
    <div v-show="activeTab === 'cpu'">
      <CpuLayout
        @switch-tab="activeTab = 'editor'"
        @switch-pipeline="activeTab = 'pipeline'"
        @switch-microstep="activeTab = 'microstep'"
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
  </div>
</template>

<style scoped>
.app {
  margin: 0;
  padding: 0;
}
</style>
