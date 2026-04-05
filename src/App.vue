<script setup lang="ts">
import { ref } from 'vue'
import CpuLayout from './components/CpuLayout.vue'
import EditorView from './views/EditorView.vue'

const activeTab = ref<'cpu' | 'editor'>('cpu')
</script>

<template>
  <div class="app">
    <div v-show="activeTab === 'cpu'">
      <CpuLayout @switch-tab="activeTab = 'editor'" />
    </div>
    <div v-show="activeTab === 'editor'">
      <EditorView :visible="activeTab === 'editor'" @switch-tab="activeTab = 'cpu'" />
    </div>
  </div>
</template>

<style scoped>
.app {
  margin: 0;
  padding: 0;
}
</style>
