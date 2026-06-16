<script setup lang="ts">
// 实时捕获预览(只读监看)+ COM 状态头条带。
// 帧走 Tauri IPC(get_snapshot,进程内 → 避免 webview 跨源拉环回 HTTP);头条带轮询 get_slide_state。
import { ref, onMounted, onUnmounted } from "vue";

import { api, type SlideState } from "../utils/api";

const slide = ref<SlideState | null>(null);
const frameSrc = ref("");
const frameOk = ref(false);
let stateTimer: number | undefined;
let frameTimer: number | undefined;
let alive = true;

async function pollState() {
  try {
    slide.value = await api.getSlideState();
  } catch {
    slide.value = null;
  }
}

// 帧链:取一帧 → 显示 → 排下一帧(自调度,慢捕获不堆积)。
async function loadFrame() {
  if (!alive) return;
  try {
    const b64 = await api.getSnapshot();
    frameSrc.value = "data:image/jpeg;base64," + b64;
    frameOk.value = true;
  } catch {
    frameOk.value = false;
  }
  if (alive) frameTimer = window.setTimeout(loadFrame, 400);
}

onMounted(() => {
  void pollState();
  stateTimer = window.setInterval(pollState, 1500);
  void loadFrame();
});
onUnmounted(() => {
  alive = false;
  if (stateTimer) window.clearInterval(stateTimer);
  if (frameTimer) window.clearTimeout(frameTimer);
});
</script>

<template>
  <section class="preview">
    <header class="pv-head">
      <span class="stat">
        <span class="dot" :class="{ on: slide?.available }"></span>
        POWERPOINT <b>{{ slide?.available ? "已连接" : "未连接" }}</b>
      </span>
      <span class="stat">
        当前页 <b>{{ slide?.available ? slide.current + "/" + slide.total : "—" }}</b>
      </span>
      <span class="show-flag" :class="{ live: slide?.in_show }">
        {{ slide?.in_show ? "LIVE" : "IDLE" }}
      </span>
    </header>
    <div class="stage">
      <img v-show="frameOk" :src="frameSrc" alt="PPT 捕获预览" draggable="false" />
      <div class="placeholder" v-if="!frameOk">等待捕获 · 未选择 PPT 窗口</div>
    </div>
  </section>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.pv-head {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 9px 14px;
  border: 1px solid var(--line);
  border-bottom: none;
  background: var(--panel);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--ink-dim);
}
.pv-head .stat {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}
.pv-head b {
  color: var(--ink);
  font-weight: 600;
}
.pv-head .dot {
  width: 8px;
  height: 8px;
  background: var(--danger);
  box-shadow: 0 0 0 1px #000;
}
.pv-head .dot.on {
  background: var(--ink);
}
.pv-head .show-flag {
  margin-left: auto;
  font-size: 10px;
  letter-spacing: 0.18em;
  border: 1px solid var(--line-hi);
  padding: 2px 8px;
  color: var(--ink-faint);
}
.pv-head .show-flag.live {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}
.stage {
  flex: 1 1 auto;
  min-height: 0;
  position: relative;
  background: #000;
  border: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.stage img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  display: block;
}
.placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #000;
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.1em;
  color: var(--ink-faint);
  pointer-events: none;
}
</style>
