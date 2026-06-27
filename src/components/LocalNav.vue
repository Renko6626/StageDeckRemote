<script setup lang="ts">
// 导播本地翻页组件。
import { ref } from "vue";

import { api } from "../utils/api";

const lastError = ref<string | null>(null);

async function run(fn: () => Promise<unknown>) {
  lastError.value = null;
  try {
    await fn();
  } catch (e) {
    lastError.value = String(e);
  }
}

const next = () => run(() => api.localNext());
const prev = () => run(() => api.localPrev());
</script>

<template>
  <section class="card">
    <h2>本地翻页(导播)</h2>

    <div class="grid">
      <button data-test="prev" @click="prev">上一页</button>
      <button data-test="next" @click="next">下一页</button>
    </div>

    <p v-if="lastError" class="error">{{ lastError }}</p>
  </section>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 8px;
}

.grid button {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 10px 8px;
}

.error {
  margin: 10px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
