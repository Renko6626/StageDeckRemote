<script setup lang="ts">
// 演出计时器(导播):快捷/自定义时长 + 启动/暂停/重置 + 本地平滑走秒。
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";

const app = useAppState();
const { timer } = storeToRefs(app);

const QUICK = [5, 10, 15, 20];
const customMin = ref(10);

// 本地走秒:用收到的 remaining_ms + 本地时间差插值,每次 status 更新重对齐。
const base = ref<{ remaining: number; running: boolean; at: number } | null>(null);
const liveRemaining = ref(0);
let tick: number | undefined;

watch(
  timer,
  (t) => {
    base.value = t ? { remaining: t.remaining_ms, running: t.running, at: Date.now() } : null;
    recompute();
  },
  { immediate: true }
);

function recompute() {
  if (!base.value) {
    liveRemaining.value = 0;
    return;
  }
  const b = base.value;
  liveRemaining.value = b.running ? b.remaining - (Date.now() - b.at) : b.remaining;
}

function fmt(ms: number): string {
  const neg = ms < 0;
  let s = Math.floor(Math.abs(ms) / 1000);
  const m = Math.floor(s / 60);
  s = s % 60;
  return (neg ? "+" : "") + m + ":" + (s < 10 ? "0" : "") + s;
}

const active = computed(() => timer.value != null);
const running = computed(() => timer.value?.running ?? false);
const over = computed(() => active.value && liveRemaining.value < 0);
const display = computed(() => (active.value ? fmt(liveRemaining.value) : "—:—"));

function start(min: number) {
  const m = Number.isFinite(min) ? min : 1;
  void app.timerStart(Math.min(180, Math.max(1, m)) * 60000);
}
function toggle() {
  void app.timerToggle();
}
function reset() {
  void app.timerReset();
}

onMounted(() => {
  tick = window.setInterval(recompute, 250);
  recompute();
});
onUnmounted(() => {
  if (tick) window.clearInterval(tick);
});
</script>

<template>
  <section class="card">
    <h2>演出计时器</h2>
    <div class="readout" :class="{ over, idle: !active }">{{ display }}</div>
    <div class="quick">
      <button v-for="q in QUICK" :key="q" @click="start(q)">{{ q }}m</button>
      <span class="custom">
        <input type="number" v-model.number="customMin" min="1" max="180" />
        <button @click="start(customMin)">设定</button>
      </span>
    </div>
    <div class="ctrl">
      <button :disabled="!active" @click="toggle">{{ running ? "暂停" : "继续" }}</button>
      <button :disabled="!active" @click="reset">重置</button>
    </div>
  </section>
</template>

<style scoped>
.readout {
  font-family: var(--mono);
  font-size: 38px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--ink);
  text-align: center;
  padding: 4px 0 10px;
}
.readout.over {
  color: var(--danger);
}
.readout.idle {
  color: var(--ink-faint);
}
.quick {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 8px;
  align-items: center;
}
.quick > button {
  font-family: var(--mono);
  font-size: 11px;
  padding: 7px 9px;
}
.custom {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  margin-left: auto;
}
.custom input {
  width: 48px;
  background: var(--panel-2);
  border: 1px solid var(--line-hi);
  color: var(--ink);
  font-family: var(--mono);
  font-size: 12px;
  padding: 6px;
}
.ctrl {
  display: flex;
  gap: 8px;
}
.ctrl button {
  flex: 1;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 9px;
}
.ctrl button:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
</style>
