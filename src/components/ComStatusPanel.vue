<script setup lang="ts">
// COM 状态面板：轮询 PowerPoint COM 连接状态。
import { ref, onMounted, onUnmounted } from "vue";
import { api, type SlideState } from "../utils/api";

const state = ref<SlideState | null>(null);
const error = ref<string | null>(null);
let timer: number | undefined;

async function poll() {
  try {
    state.value = await api.getSlideState();
    error.value = null;
  } catch (e) {
    error.value = String(e);
    state.value = null;
  }
}

onMounted(() => {
  void poll();
  timer = window.setInterval(poll, 1500);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <section class="card com-panel">
    <h2>PowerPoint · COM</h2>

    <div class="rows">
      <div class="row">
        <span class="k">POWERPOINT</span>
        <span class="v" :class="state?.available ? 'on' : 'off'">
          {{ state?.available ? "已连接" : "未连接" }}
        </span>
      </div>

      <div class="row">
        <span class="k">当前页</span>
        <span class="v readout">
          <template v-if="state?.available">
            {{ state.current }}<span class="sep">/</span>{{ state.total }}
          </template>
          <template v-else>—</template>
        </span>
      </div>

      <div class="row">
        <span class="k">放映</span>
        <span class="chip" :class="state?.in_show ? 'live' : 'idle'">
          {{ state?.in_show ? "LIVE" : "IDLE" }}
        </span>
      </div>
    </div>

    <p v-if="error" class="err">{{ error }}</p>
  </section>
</template>

<style scoped>
.com-panel {
  padding: 14px 16px;
}

.rows {
  display: grid;
  gap: 10px;
}

.row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.k {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  color: var(--ink-faint);
}

.v {
  font-family: var(--mono);
  font-size: 14px;
  letter-spacing: 0.06em;
  color: var(--ink-dim);
}

.v.on {
  color: var(--ink);
}

.readout {
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--ink);
}

.sep {
  font-size: 14px;
  color: var(--ink-faint);
  margin: 0 2px;
}

.chip {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  border: 1px solid var(--line-hi);
  padding: 2px 8px;
  color: var(--ink-faint);
}

.chip.live {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

.err {
  margin: 10px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
