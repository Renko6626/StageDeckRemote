<script setup lang="ts">
// 实时捕获预览(只读监看)+ COM 状态头条带。
// 预览图复用主持人端的 /api/snapshot 链式刷新;头条带轮询 get_slide_state(IPC)。
import { ref, onMounted, onUnmounted, watch } from "vue";
import { storeToRefs } from "pinia";

import { api, type SlideState } from "../utils/api";
import { useNetworkState } from "../stores/networkState";

const net = useNetworkState();
const { serverPort, token } = storeToRefs(net);

const slide = ref<SlideState | null>(null);
const img = ref<HTMLImageElement | null>(null);
const frameOk = ref(false);
let timer: number | undefined;
let started = false;

async function poll() {
  try {
    slide.value = await api.getSlideState();
  } catch {
    slide.value = null;
  }
}

// 链式刷新:上一帧 load/error 后再排下一帧,避免慢链路堆积。
function loadFrame() {
  if (!img.value || !token.value) return;
  img.value.src =
    "http://127.0.0.1:" +
    serverPort.value +
    "/api/snapshot.jpg?token=" +
    encodeURIComponent(token.value) +
    "&t=" +
    Date.now();
}
function onFrameLoad() {
  frameOk.value = true;
  window.setTimeout(loadFrame, 300);
}
function onFrameError() {
  frameOk.value = false;
  window.setTimeout(loadFrame, 1000);
}

// token 在首次状态轮询后才就位;就位即启动链式刷新(仅一次)。
function kick() {
  if (started || !token.value) return;
  started = true;
  loadFrame();
}

onMounted(() => {
  void poll();
  timer = window.setInterval(poll, 1500);
  kick();
});
watch(token, kick);
onUnmounted(() => {
  if (timer) window.clearInterval(timer);
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
      <img
        ref="img"
        v-show="frameOk"
        alt="PPT 捕获预览"
        draggable="false"
        @load="onFrameLoad"
        @error="onFrameError"
      />
      <div class="placeholder" v-if="!frameOk">
        {{ token ? "等待捕获 · 未选择 PPT 窗口" : "服务未就绪" }}
      </div>
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
