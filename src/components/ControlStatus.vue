<script setup lang="ts">
// 控制状态总览（spec §6.1.3 / §12.1）。
import { computed } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";
import { useNetworkState } from "../stores/networkState";

const app = useAppState();
const net = useNetworkState();
const { remoteEnabled, controlLocked, presenterUnlocked, selectedWindow, activeClientId, presenterOnline } =
  storeToRefs(app);
const { online } = storeToRefs(net);

const canControl = computed(
  () => remoteEnabled.value && !controlLocked.value && presenterUnlocked.value
);
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
      <li class="release-row">
        <button class="release" :disabled="!activeClientId" @click="app.releaseController()">
          释放控制权
        </button>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.status {
  list-style: none;
  margin: 0;
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

/* ok = white (active/connected/enabled) */
.v.ok {
  color: var(--ink);
}

/* bad = danger red (locked/disconnected) — only for hard-danger states */
.v.bad {
  color: var(--danger);
}

/* warn = dim (not yet active but not dangerous) */
.v.warn {
  color: var(--ink-dim);
}
.release-row {
  border-bottom: none;
  padding-top: 8px;
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
</style>
