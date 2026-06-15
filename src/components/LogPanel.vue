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
.log {
  display: flex;
  flex-direction: column;
  min-height: 0;
  box-sizing: border-box;
}

.log h2 {
  flex: 0 0 auto;
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
