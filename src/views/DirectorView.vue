<script setup lang="ts">
// 导播端主面板:方案 B + 可拖拽分栏(左栏宽 / 预览·日志高,夹紧并记忆到 localStorage)。
import { onMounted, onUnmounted, ref } from "vue";

import { useAppState } from "../stores/appState";
import ControlStatus from "../components/ControlStatus.vue";
import WindowPicker from "../components/WindowPicker.vue";
import QrCodePanel from "../components/QrCodePanel.vue";
import LocalControlPanel from "../components/LocalControlPanel.vue";
import LogPanel from "../components/LogPanel.vue";
import DirectorPreview from "../components/DirectorPreview.vue";

const app = useAppState();
let timer: number | undefined;

// ── 可拖拽分栏 ──
const RAIL_MIN = 300;
const RAIL_MAX = 600;
const RAIL_DEFAULT = 380;
const LOG_MIN = 160;
const LOG_MAX = 800;
const LOG_DEFAULT = 300;

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

function readStored(key: string, def: number, lo: number, hi: number): number {
  const raw = localStorage.getItem(key);
  const n = raw == null ? NaN : Number(raw);
  return Number.isFinite(n) ? clamp(n, lo, hi) : def;
}

const railWidth = ref(readStored("sd.dir.railW", RAIL_DEFAULT, RAIL_MIN, RAIL_MAX));
const logHeight = ref(readStored("sd.dir.logH", LOG_DEFAULT, LOG_MIN, LOG_MAX));

// 竖向拖条:调左栏宽度。
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
  h.addEventListener("pointercancel", up); // 触控被系统手势打断时也要收尾
}

// 横向拖条:调预览/日志高度比(向上拖增大日志)。
function startLog(e: PointerEvent) {
  e.preventDefault();
  const h = e.currentTarget as HTMLElement;
  h.setPointerCapture(e.pointerId);
  const startY = e.clientY;
  const startH = logHeight.value;
  const mainH = h.parentElement ? h.parentElement.clientHeight : 0;
  const upper = mainH > 0 ? Math.min(LOG_MAX, mainH - 200) : LOG_MAX;
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
  h.addEventListener("pointercancel", up); // 触控被系统手势打断时也要收尾
}

onMounted(async () => {
  await app.refresh();
  // 启动时未选窗口才自动识别;手动选择为权威,绝不被自动悄悄覆盖。
  if (!app.selectedWindow) {
    try {
      await app.autoDetectPpt();
    } catch {
      /* 自动识别失败不阻塞启动 */
    }
  }
  timer = window.setInterval(() => void app.refresh(), 1500);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <div class="dashboard" :style="{ '--rail-w': railWidth + 'px', '--log-h': logHeight + 'px' }">
    <aside class="rail">
      <WindowPicker />
      <ControlStatus />
      <LocalControlPanel />
      <QrCodePanel />
    </aside>
    <div class="vsplit" @pointerdown="startRail" title="拖动调整栏宽"></div>
    <main class="main">
      <DirectorPreview class="pane-preview" />
      <div class="hsplit" @pointerdown="startLog" title="拖动调整预览/日志比例"></div>
      <LogPanel class="pane-log" />
    </main>
  </div>
</template>

<style scoped>
.dashboard {
  display: grid;
  grid-template-columns: var(--rail-w) 12px minmax(0, 1fr);
  gap: 0;
  padding: 16px;
  height: calc(100vh - 38px); /* 减去顶栏(App.vue .topbar,实测约 36px)高度 */
  overflow: hidden;
}
.rail {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  overflow-y: auto;
}
.main {
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

/* ── splitters ── */
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
/* 居中握把 */
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
  .dashboard {
    grid-template-columns: 1fr;
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
