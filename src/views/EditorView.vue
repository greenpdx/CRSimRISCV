<template>
  <div class="editor">
    <div class="editor__header">
      <h1 class="editor__title">RISC-V Assembly Editor</h1>
      <span class="editor__subtitle">Coming soon — write ASM or Rust, see hex, load into CPU</span>
      <button class="editor__back" @click="emit('switch-tab')">Back to CPU</button>
    </div>
    <div class="editor__body">
      <textarea
        class="editor__textarea"
        v-model="source"
        placeholder="# Write RISC-V assembly here
addi x3, x0, 7
addi x4, x3, 5
lui  x5, 0x10
sw   x4, 4(x5)"
        spellcheck="false"
      />
      <div class="editor__output">
        <div class="editor__output-header">Output (placeholder)</div>
        <pre class="editor__output-text">{{ source || '# no source' }}</pre>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const emit = defineEmits<{
  'switch-tab': []
}>()

const source = ref('')
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

.editor__header {
  display: flex;
  align-items: baseline;
  gap: 16px;
  padding: 12px 20px;
  border-bottom: 1px solid #333;
}
.editor__title {
  font-size: 16px;
  font-weight: 700;
  letter-spacing: 1px;
  margin: 0;
}
.editor__subtitle {
  font-size: 11px;
  color: #888;
}
.editor__back {
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
.editor__back:hover { background: #3a3a6a; }

.editor__body {
  flex: 1;
  display: flex;
  gap: 1px;
  background: #333;
  overflow: hidden;
}
.editor__textarea {
  flex: 1;
  padding: 16px;
  background: #0a0a1a;
  color: #eee;
  border: none;
  font-family: 'JetBrains Mono', monospace;
  font-size: 13px;
  line-height: 1.5;
  resize: none;
  outline: none;
}
.editor__output {
  flex: 1;
  background: #0a0a1a;
  overflow: auto;
}
.editor__output-header {
  padding: 8px 16px;
  font-size: 11px;
  color: #888;
  border-bottom: 1px solid #222;
}
.editor__output-text {
  padding: 16px;
  font-size: 12px;
  color: #8fc;
  margin: 0;
}
</style>
