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
  margin-bottom: 10px;
}
.title {
  font-weight: 600;
}
.meta {
  font-size: 13px;
  opacity: 0.6;
}
.actions {
  display: flex;
  gap: 8px;
}
.win-list {
  list-style: none;
  padding: 0;
  margin: 10px 0 0;
  display: grid;
  gap: 6px;
  max-height: 160px;
  overflow: auto;
}
.win {
  width: 100%;
  text-align: left;
}
.error {
  color: #ff453a;
  font-size: 13px;
}
.hint {
  font-size: 12px;
  opacity: 0.5;
}
</style>
