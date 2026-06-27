<script setup lang="ts">
// 控制状态总览 + 远控 toggle + 释放控制权 按钮。
// 从 ControlStatus.vue 和 LocalControlPanel.vue(远控 toggle 部分)
// 合并而来。
//
// mode prop 控制远控 toggle 的视觉权重:
//   setup — toggle 是 Setup 阶段主操作,放大
//   live  — toggle 在 Live 阶段不应该动,缩成小行
import { computed, ref } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";
import { useNetworkState } from "../stores/networkState";

defineProps<{ mode: "setup" | "live" }>();

const app = useAppState();
const net = useNetworkState();
const {
  remoteEnabled,
  controlLocked,
  presenterUnlocked,
  selectedWindow,
  activeClientId,
  presenterOnline,
} = storeToRefs(app);
const { online } = storeToRefs(net);

const canControl = computed(
  () => remoteEnabled.value && !controlLocked.value && presenterUnlocked.value
);

const lastError = ref<string | null>(null);

async function toggleRemote() {
  lastError.value = null;
  try {
    await app.setRemoteEnabled(!remoteEnabled.value);
  } catch (e) {
    lastError.value = String(e);
  }
}
</script>

<template>
  <section class="card">
    <h2>控制状态</h2>
    <ul class="status">
      <li>
        <span class="k">服务</span>
        <span class="v" :class="online ? 'ok' : 'bad'">{{ online ? "运行中" : "未连接" }}</span>
      </li>
      <li>
        <span class="k">PPT 窗口</span>
        <span class="v" :class="selectedWindow ? 'ok' : 'bad'">
          {{ selectedWindow ? "已选择" : "未选择" }}
        </span>
      </li>
      <li>
        <span class="k">远程控制</span>
        <span class="v" :class="remoteEnabled ? 'ok' : 'bad'">
          {{ remoteEnabled ? "允许" : "禁止" }}
        </span>
      </li>
      <li>
        <span class="k">强制锁</span>
        <span class="v" :class="controlLocked ? 'bad' : 'ok'">
          {{ controlLocked ? "锁定" : "释放" }}
        </span>
      </li>
      <li>
        <span class="k">主持人</span>
        <span class="v" :class="canControl ? 'ok' : 'warn'">
          {{ canControl ? "可控制" : "已锁定" }}
        </span>
      </li>
      <li>
        <span class="k">主持人</span>
        <span class="v" :class="presenterOnline ? 'ok' : 'bad'">
          {{ presenterOnline ? "在线" : "离线" }}
        </span>
      </li>
      <li>
        <span class="k">当前控制端</span>
        <span class="v">{{ activeClientId ? activeClientId.slice(0, 6) : "—" }}</span>
      </li>
    </ul>

    <button
      data-test="remote-toggle"
      :data-mode="$props.mode"
      :class="{ on: remoteEnabled }"
      class="remote"
      @click="toggleRemote"
    >
      远程控制:{{ remoteEnabled ? "已开启" : "已关闭" }}
    </button>

    <button
      data-test="release"
      class="release"
      :disabled="!activeClientId"
      @click="app.releaseController()"
    >
      释放控制权
    </button>

    <p v-if="lastError" class="error">{{ lastError }}</p>
  </section>
</template>

<style scoped>
.status {
  list-style: none;
  margin: 0 0 10px;
  padding: 0;
  display: grid;
  gap: 0;
}
.status li {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 7px 0;
  border-bottom: 1px solid var(--line);
  font-size: 13px;
}
.status li:last-child {
  border-bottom: none;
}
.k {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--ink-faint);
}
.v {
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.08em;
  color: var(--ink-dim);
}
.v.ok {
  color: var(--ink);
}
.v.bad {
  color: var(--danger);
}
.v.warn {
  color: var(--ink-dim);
}

.remote {
  width: 100%;
  font-family: var(--mono);
  letter-spacing: 0.16em;
  text-transform: uppercase;
  font-weight: 600;
  margin-bottom: 8px;
}
.remote[data-mode="setup"] {
  padding: 11px 12px;
  font-size: 11px;
}
.remote[data-mode="live"] {
  padding: 7px 10px;
  font-size: 10px;
  opacity: 0.85;
}
.remote.on {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

.release {
  width: 100%;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px;
}
.release:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.error {
  margin: 10px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
