<script setup lang="ts">
// 操作日志（spec §6.1 / §13.5）。
import { computed } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";

const app = useAppState();
const { logs } = storeToRefs(app);

// 最新的在上面。
const ordered = computed(() => [...logs.value].reverse());

function fmtTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => n.toString().padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
</script>

<template>
  <section class="card log">
    <h2>日志</h2>
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
.log ul {
  list-style: none;
  margin: 0;
  padding: 0;
  max-height: 220px;
  overflow: auto;
  font-size: 13px;
}
.log li {
  display: flex;
  gap: 8px;
  padding: 3px 0;
  border-bottom: 1px solid #23232b;
}
.t {
  opacity: 0.5;
  font-family: ui-monospace, monospace;
  flex: 0 0 auto;
}
.m {
  word-break: break-all;
}
.empty {
  opacity: 0.4;
}
</style>
