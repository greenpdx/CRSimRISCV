<template>
  <div class="pipeline-view">
    <div class="pipeline-view__header">
      <h1 class="pipeline-view__title">5-Stage Pipeline — IF Stage</h1>
      <button class="pipeline-view__btn" @click="emit('switch-tab')">Back to CPU</button>
    </div>
    <div class="pipeline-view__body">
      <PipelineIfStage />
    </div>
  </div>
</template>

<script setup lang="ts">
import PipelineIfStage from '../components/pipeline/PipelineIfStage.vue'

const emit = defineEmits<{
  'switch-tab': []
}>()
</script>

<style scoped>
.pipeline-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: #1a1a2e;
  color: #eee;
  font-family: 'JetBrains Mono', monospace;
}
.pipeline-view__header {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 8px 16px;
  border-bottom: 1px solid #333;
}
.pipeline-view__title {
  font-size: 14px;
  font-weight: 700;
  letter-spacing: 1px;
  margin: 0;
}
.pipeline-view__btn {
  margin-left: auto;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 5px 14px;
  border: 1px solid #555;
  border-radius: 3px;
  background: #2a2a4a;
  color: #eee;
  cursor: pointer;
}
.pipeline-view__btn:hover { background: #3a3a6a; }
.pipeline-view__body {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}
</style>
