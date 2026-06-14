<script setup lang="ts">
// 控制状态总览（spec §6.1.3 / §12.1）。
import { computed } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";
import { useNetworkState } from "../stores/networkState";

const app = useAppState();
const net = useNetworkState();
const { remoteEnabled, controlLocked, presenterUnlocked, selectedWindow, activeClientId } =
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
        <span class="k">当前控制端</span>
        <span class="v">{{ activeClientId ?? "—" }}</span>
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
  gap: 8px;
}
.status li {
  display: flex;
  justify-content: space-between;
  font-size: 14px;
}
.k {
  opacity: 0.7;
}
.v.ok {
  color: #34c759;
}
.v.bad {
  color: #ff453a;
}
.v.warn {
  color: #ffb020;
}
</style>
