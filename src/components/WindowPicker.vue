<script setup lang="ts">
// 选择 / 自动识别 PPT 放映窗口（spec §6.1.1）。
import { ref } from "vue";
import { storeToRefs } from "pinia";

import { api, type TargetWindow } from "../utils/api";
import { useAppState } from "../stores/appState";

const app = useAppState();
const { selectedWindow } = storeToRefs(app);

const windows = ref<TargetWindow[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);

async function refreshList() {
  loading.value = true;
  error.value = null;
  try {
    windows.value = await api.listWindows();
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function autoDetect() {
  try {
    await app.autoDetectPpt();
  } catch (e) {
    error.value = String(e);
  }
}

async function pick(w: TargetWindow) {
  try {
    await app.selectWindow(w);
  } catch (e) {
    error.value = String(e);
  }
}
</script>

<template>
  <section class="card">
    <h2>PPT 窗口</h2>

    <div class="current">
      <template v-if="selectedWindow">
        <div class="title">{{ selectedWindow.title }}</div>
        <div class="meta">
          {{ selectedWindow.width }}×{{ selectedWindow.height }} @ ({{ selectedWindow.left }},
          {{ selectedWindow.top }})
        </div>
      </template>
      <div v-else class="meta">未选择窗口</div>
    </div>

    <div class="actions">
      <button @click="autoDetect">自动识别 PowerPoint</button>
      <button @click="refreshList" :disabled="loading">
        {{ loading ? "刷新中…" : "枚举窗口" }}
      </button>
    </div>

    <p v-if="error" class="error">{{ error }}</p>

    <ul v-if="windows.length" class="win-list">
      <li v-for="w in windows" :key="w.handle">
        <button class="win" @click="pick(w)">{{ w.title }}</button>
      </li>
    </ul>
    <p v-else-if="!loading" class="hint">
      （Windows 下点击"枚举窗口"会列出可选窗口；当前平台为占位空列表）
    </p>
  </section>
</template>

<style scoped>
.current {
  margin-bottom: 12px;
  padding: 8px 0;
  border-bottom: 1px solid var(--line);
}

.title {
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.06em;
  color: var(--ink);
  margin-bottom: 4px;
}

.meta {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-faint);
  letter-spacing: 0.06em;
}

.actions {
  display: flex;
  gap: 8px;
  margin-bottom: 2px;
}

.actions button {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px 10px;
}

.error {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  margin: 8px 0 0;
}

.win-list {
  list-style: none;
  padding: 0;
  margin: 10px 0 0;
  display: grid;
  gap: 4px;
  max-height: 160px;
  overflow: auto;
}

.win {
  width: 100%;
  text-align: left;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.04em;
  padding: 8px 10px;
}

.hint {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--ink-faint);
  letter-spacing: 0.06em;
  margin: 8px 0 0;
}
</style>
