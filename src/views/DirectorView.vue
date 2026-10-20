<script setup lang="ts">
// 导播端主面板:Setup / Live 双模式 + 可拖拽分栏(左栏宽 / 预览·日志高,
// 夹紧并记忆到 localStorage)。
//
// 设计来源:docs/superpowers/specs/2026-06-27-director-gui-setup-live-split-design.md
import { onMounted, onUnmounted, ref } from "vue";

import { useNetworkState } from "../stores/networkState";
import { useAppState } from "../stores/appState";
import StatusBar from "../components/StatusBar.vue";
import EmergencyLock from "../components/EmergencyLock.vue";
import WindowPicker from "../components/WindowPicker.vue";
import QrCodePanel from "../components/QrCodePanel.vue";
import LocalNav from "../components/LocalNav.vue";
import DirectorTimer from "../components/DirectorTimer.vue";
import LogPanel from "../components/LogPanel.vue";
import DirectorPreview from "../components/DirectorPreview.vue";
import DirectorMessages from "../components/DirectorMessages.vue";

type DirectorMode = "setup" | "live";

const app = useAppState();
const net = useNetworkState();
// 注意: Vue 3 <script setup> 下这些是 per-instance 变量(每个组件实例独立),
// 不要随便挪进 composable —— 共享 state 会让两个 DirectorView 实例相互覆盖。
let timer: number | undefined;
let inFlight = false;

// ── 模式(冷启动总在 Setup,不持久化到 localStorage)──
const mode = ref<DirectorMode>("setup");

// ── 可拖拽分栏 ──
const RAIL_MIN = 300;
const RAIL_MAX = 600;
const RAIL_DEFAULT = 330;
const LOG_MIN = 160;
const LOG_MAX = 800;
const LOG_DEFAULT = 180;

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

function readStored(key: string, def: number, lo: number, hi: number): number {
  const raw = localStorage.getItem(key);
  const n = raw == null ? NaN : Number(raw);
  return Number.isFinite(n) ? clamp(n, lo, hi) : def;
}

const railWidth = ref(readStored("sd.dir.railW", RAIL_DEFAULT, RAIL_MIN, RAIL_MAX));
const logHeight = ref(readStored("sd.dir.logH", LOG_DEFAULT, LOG_MIN, LOG_MAX));

function startRail(e: PointerEvent) {
  e.preventDefault();
  const h = e.currentTarget as HTMLElement;
  h.setPointerCapture(e.pointerId);
  const startX = e.clientX;
  const startW = railWidth.value;
  const move = (ev: PointerEvent) => {
    railWidth.value = clamp(startW + (ev.clientX - startX), RAIL_MIN, RAIL_MAX);
  };
  const up = () => {
    h.removeEventListener("pointermove", move);
    h.removeEventListener("pointerup", up);
    h.removeEventListener("pointercancel", up);
    localStorage.setItem("sd.dir.railW", String(railWidth.value));
  };
  h.addEventListener("pointermove", move);
  h.addEventListener("pointerup", up);
  h.addEventListener("pointercancel", up);
}

function startLog(e: PointerEvent) {
  e.preventDefault();
  const h = e.currentTarget as HTMLElement;
  h.setPointerCapture(e.pointerId);
  const startY = e.clientY;
  const startH = logHeight.value;
  const mainH = h.parentElement ? h.parentElement.clientHeight : 0;
  const toolbarH = h.parentElement?.querySelector(".local-controls")?.clientHeight ?? 0;
  const upper = mainH > 0 ? Math.max(LOG_MIN, Math.min(LOG_MAX, mainH - toolbarH - 212)) : LOG_MAX;
  const move = (ev: PointerEvent) => {
    logHeight.value = clamp(startH + (startY - ev.clientY), LOG_MIN, upper);
  };
  const up = () => {
    h.removeEventListener("pointermove", move);
    h.removeEventListener("pointerup", up);
    h.removeEventListener("pointercancel", up);
    localStorage.setItem("sd.dir.logH", String(logHeight.value));
  };
  h.addEventListener("pointermove", move);
  h.addEventListener("pointerup", up);
  h.addEventListener("pointercancel", up);
}

// ── 轮询状态(原逻辑保留,仅迁移过来)──
async function pull() {
  if (inFlight) return;
  inFlight = true;
  try {
    await app.refresh();
  } finally {
    inFlight = false;
  }
}

onMounted(async () => {
  await pull();
  timer = window.setInterval(pull, 1500);
});

onUnmounted(() => {
  if (timer !== undefined) window.clearInterval(timer);
});
</script>

<template>
  <div class="dashboard" :style="{ '--rail-w': railWidth + 'px', '--log-h': logHeight + 'px' }">
    <!-- 顶栏 Setup / Live 切换 -->
    <header class="mode-bar">
      <button
        data-test="mode-setup"
        :data-active="mode === 'setup'"
        class="mode-btn"
        @click="mode = 'setup'"
      >
        Setup
      </button>
      <button
        data-test="mode-live"
        :data-active="mode === 'live'"
        class="mode-btn"
        @click="mode = 'live'"
      >
        Live
      </button>
      <span class="service-status" :class="{ offline: !net.online }">{{ net.online ? "服务运行中" : "服务未连接" }}</span>
    </header>

    <aside class="rail" :class="{ 'setup-rail': mode === 'setup' }">
      <!-- Setup 模式 rail -->
      <template v-if="mode === 'setup'">
        <WindowPicker />
        <QrCodePanel />
        <StatusBar mode="setup" />
      </template>

      <!-- Live 模式 rail -->
      <template v-else>
        <StatusBar mode="live" />
        <EmergencyLock size="large" />
        <DirectorTimer />
        <DirectorMessages />
        <LocalNav />
      </template>
    </aside>

    <div class="vsplit" @pointerdown="startRail" title="拖动调整栏宽"></div>

    <main class="main">
      <DirectorPreview class="pane-preview" />
      <div v-if="mode === 'setup'" class="local-controls">
        <LocalNav compact />
        <EmergencyLock size="small" compact />
      </div>
      <div class="hsplit" @pointerdown="startLog" title="拖动调整预览/日志比例"></div>
      <LogPanel class="pane-log" />
    </main>
  </div>
</template>

<style scoped>
.setup-rail > :deep(.card) { padding-top: 12px; padding-bottom: 12px; }
.service-status { margin-left: auto; font-size: 11px; color: var(--ink-dim); }
.service-status.offline { color: #f19a9a; }
.local-controls { flex-shrink: 0; display: flex; align-items: flex-start; gap: 12px; padding: 10px 12px; border: 1px solid var(--line); background: var(--panel); margin-top: 10px; }
.local-controls > :first-child { flex: 1; }

.dashboard {
  display: grid;
  grid-template-areas:
    "mode mode mode"
    "rail vsplit main";
  grid-template-columns: var(--rail-w) 12px minmax(0, 1fr);
  grid-template-rows: 36px 1fr;
  gap: 0;
  padding: 16px;
  height: calc(100dvh - 60px);
  overflow: hidden;
}

.mode-bar {
  grid-area: mode;
  display: flex;
  gap: 4px;
  justify-content: flex-start;
  align-items: center;
  border-bottom: 1px solid var(--line);
  padding-bottom: 8px;
  margin-bottom: 8px;
}

.mode-btn {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  font-weight: 600;
  padding: 6px 18px;
  border: 1px solid var(--line);
  background: transparent;
  color: var(--ink-dim);
  cursor: pointer;
}

.mode-btn[data-active="true"] {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

.rail {
  grid-area: rail;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
  overflow-y: auto;
}

.vsplit {
  grid-area: vsplit;
}

.main {
  grid-area: main;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.pane-preview {
  flex: 1 1 0;
  min-height: 0;
}
.pane-log {
  flex: 0 0 auto;
  height: var(--log-h);
  min-height: 0;
}

.vsplit,
.hsplit {
  position: relative;
  background: var(--line);
  transition: background 0.1s linear;
  touch-action: none;
  -webkit-user-select: none;
  user-select: none;
}
.vsplit {
  cursor: col-resize;
}
.hsplit {
  flex: 0 0 12px;
  cursor: row-resize;
}
.vsplit:hover,
.hsplit:hover {
  background: var(--ink-dim);
}
.vsplit::before,
.hsplit::before {
  content: "";
  position: absolute;
  inset: 0;
  margin: auto;
  background: var(--line-hi);
}
.vsplit::before {
  width: 2px;
  height: 28px;
}
.hsplit::before {
  width: 28px;
  height: 2px;
}

@media (max-width: 980px) {
  .local-controls { flex-wrap: wrap; }

  .dashboard {
    grid-template-areas:
      "mode"
      "rail"
      "main";
    grid-template-columns: 1fr;
    grid-template-rows: 36px auto auto;
    height: auto;
    overflow: visible;
  }
  .vsplit,
  .hsplit {
    display: none;
  }
  .pane-preview {
    flex: none;
    height: 56.25vw;
  }
  .pane-log {
    height: 280px;
  }
}
</style>
