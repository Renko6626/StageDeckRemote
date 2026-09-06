<script setup lang="ts">
// 实时捕获预览:帧走 Tauri Channel(broker 推送,变化才来)→ canvas 双缓冲;头条带轮询 slide_state。
import { ref, onMounted, onUnmounted } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";

import { api, type SlideState } from "../utils/api";

const slide = ref<SlideState | null>(null);
const frameOk = ref(false);
const canvasEl = ref<HTMLCanvasElement | null>(null);
let stateTimer: number | undefined;
let channel: Channel<ArrayBuffer> | null = null;

async function pollState() {
  try {
    slide.value = await api.getSlideState();
  } catch {
    slide.value = null;
  }
}

// 解码异步:理论上相邻帧可能乱序落地,但 broker 变化门控 + 低帧率下几乎不会并发在飞,
// 即便偶发陈旧一帧也会被下一帧覆盖,故不加序号守卫。
async function drawFrame(buf: ArrayBuffer) {
  const canvas = canvasEl.value;
  if (!canvas) return;
  try {
    const bmp = await createImageBitmap(new Blob([buf], { type: "image/jpeg" }));
    canvas.width = bmp.width;
    canvas.height = bmp.height;
    canvas.getContext("2d")?.drawImage(bmp, 0, 0);
    bmp.close();
    frameOk.value = true;
  } catch {
    /* 单帧解码失败忽略,保留上一帧 */
  }
}

onMounted(() => {
  void pollState();
  stateTimer = window.setInterval(pollState, 1500);
  channel = new Channel<ArrayBuffer>();
  channel.onmessage = (buf) => void drawFrame(buf);
  void invoke("subscribe_preview", { onFrame: channel });
});
onUnmounted(() => {
  if (stateTimer) window.clearInterval(stateTimer);
  // 释放本地引用。注意:置 null 不会通知 Rust(Channel 无 JS→Rust 关闭信号)——
  // 旧推送任务由后端在「下次 subscribe_preview 时 abort 上一个」收尾(见 commands.rs)。
  // 本组件在导播单窗口内只挂一次,残留任务至多一个、应用退出时回收。
  channel = null;
});
</script>

<template>
  <section class="preview">
    <header class="pv-head">
      <span class="stat">
        <span class="dot" :class="{ on: slide?.available }"></span>
        POWERPOINT <b>{{ slide?.busy ? "忙碌 · 请检查 PPT 弹窗" : slide?.available ? "已连接" : "未连接" }}</b>
      </span>
      <span class="stat">
        当前页 <b>{{ slide?.available ? slide.current + "/" + slide.total : "—" }}</b>
      </span>
      <span class="show-flag" :class="{ live: slide?.in_show }">
        {{ slide?.in_show ? "LIVE" : "IDLE" }}
      </span>
    </header>
    <div class="stage">
      <canvas v-show="frameOk" ref="canvasEl" class="frame"></canvas>
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
.stage .frame {
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
