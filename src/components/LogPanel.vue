<script setup lang="ts">
// 操作日志（spec §6.1 / §13.5）。
import { computed, ref, watch } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";
import { api, type DiagnosticsStatus } from "../utils/api";

const app = useAppState();
const { logs } = storeToRefs(app);

// 最新的在上面。
const ordered = computed(() => [...logs.value].reverse());
const diagnostics = ref<DiagnosticsStatus | null>(null);
const statusError = ref<string | null>(null);
const operationError = ref<string | null>(null);
const changing = ref(false);
const opening = ref(false);
let refreshing = false;

async function refreshDiagnostics() {
  if (refreshing || changing.value) return;
  refreshing = true;
  try {
    diagnostics.value = await api.getDiagnosticsStatus();
    statusError.value = null;
  } catch (e) {
    statusError.value = `读取诊断状态失败：${String(e)}`;
  } finally {
    refreshing = false;
  }
}

// 复用导播日志刷新，及时显示文件写入错误。
watch(logs, refreshDiagnostics, { immediate: true });

async function changeDetailed(event: Event) {
  const input = event.target as HTMLInputElement;
  const enabled = input.checked;
  input.checked = diagnostics.value?.enabled ?? false;
  changing.value = true;
  operationError.value = null;
  try {
    diagnostics.value = await api.setDetailedDiagnostics(enabled);
    statusError.value = null;
  } catch (e) {
    operationError.value = `切换详细诊断失败：${String(e)}`;
  } finally {
    changing.value = false;
  }
}

async function openDirectory() {
  opening.value = true;
  operationError.value = null;
  try {
    await api.openLogsDirectory();
  } catch (e) {
    operationError.value = `打开日志目录失败：${String(e)}`;
  } finally {
    opening.value = false;
  }
}

function fmtTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => n.toString().padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
</script>

<template>
  <section class="card log">
    <h2>日志</h2>
    <div class="diagnostics">
      <div class="diagnostic-controls">
        <label>
          <input
            type="checkbox"
            :checked="diagnostics?.enabled ?? false"
            :disabled="!diagnostics || changing"
            @change="changeDetailed"
          />
          详细诊断
        </label>
        <button :disabled="!diagnostics?.directory || opening" @click="openDirectory">打开日志目录</button>
      </div>
      <p class="hint">常规日志自动保存。详细诊断仅本次运行有效。</p>
      <p v-if="diagnostics?.directory" class="directory">{{ diagnostics.directory }}</p>
      <p v-if="diagnostics?.error" class="error" role="alert">日志文件不可用：{{ diagnostics.error }}</p>
      <p v-if="statusError" class="error" role="alert">{{ statusError }}</p>
      <p v-if="operationError" class="error" role="alert">{{ operationError }}</p>
    </div>
    <ul>
      <li v-for="(entry, i) in ordered" :key="`${entry.time_ms}-${i}`">
        <span class="t">{{ fmtTime(entry.time_ms) }}</span>
        <span class="m">{{ entry.message }}</span>
      </li>
      <li v-if="!ordered.length" class="empty">暂无日志</li>
    </ul>
  </section>
</template>

<style scoped>
.log {
  display: flex;
  flex-direction: column;
  min-height: 0;
  box-sizing: border-box;
}

.log h2 {
  flex: 0 0 auto;
}

.diagnostics {
  flex: 0 0 auto;
  margin-bottom: 10px;
}

.diagnostic-controls {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 12px;
}

.diagnostic-controls label {
  display: flex;
  align-items: center;
  gap: 6px;
}

.diagnostic-controls button {
  padding: 5px 8px;
  font-size: 11px;
}

.hint, .directory, .error {
  margin: 6px 0 0;
  font-size: 11px;
  overflow-wrap: anywhere;
}

.hint, .directory {
  color: var(--ink-faint);
}

.directory {
  font-family: var(--mono);
}

.error {
  color: var(--danger);
}

.log ul {
  list-style: none;
  margin: 0;
  padding: 0;
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
}

.log li {
  display: flex;
  gap: 10px;
  padding: 5px 0;
  border-bottom: 1px solid var(--line);
  font-size: 12px;
  font-family: var(--mono);
  letter-spacing: 0.02em;
}

.log li:last-child {
  border-bottom: none;
}

.t {
  flex: 0 0 auto;
  color: var(--ink-faint);
  font-size: 11px;
  letter-spacing: 0.06em;
}

.m {
  color: var(--ink-dim);
  word-break: break-all;
}

.empty {
  color: var(--ink-faint);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}
</style>
