<script setup lang="ts">
// 置顶软弹窗:监听 host-message 事件,显示约 6 秒后自关;不抢焦点。
import { ref, onMounted, onUnmounted } from "vue";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface HostMessage {
  from: string;
  text: string;
  at_ms: number;
}

const DURATION = 6000;
const text = ref("");
const time = ref("");
const extra = ref(0); // 连发叠加计数
const progress = ref(100);

let unlisten: UnlistenFn | null = null;
let hideTimer: number | undefined;
let tick: number | undefined;
let deadline = 0;

function fmtTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => n.toString().padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}`;
}

function chime() {
  if (localStorage.getItem("sd.dir.toastChime") === "off") return;
  try {
    const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    const ac = new Ctx();
    const o = ac.createOscillator();
    const g = ac.createGain();
    o.frequency.value = 880;
    o.connect(g);
    g.connect(ac.destination);
    g.gain.setValueAtTime(0.0001, ac.currentTime);
    g.gain.exponentialRampToValueAtTime(0.15, ac.currentTime + 0.02);
    g.gain.exponentialRampToValueAtTime(0.0001, ac.currentTime + 0.25);
    o.start();
    o.stop(ac.currentTime + 0.26);
  } catch {
    /* 音频不可用则静默 */
  }
}

function show(msg: HostMessage) {
  if (text.value) extra.value += 1; // 已有一条在显示 → 叠加计数
  text.value = msg.text;
  time.value = fmtTime(msg.at_ms);
  deadline = Date.now() + DURATION;
  progress.value = 100;
  if (hideTimer) window.clearTimeout(hideTimer);
  hideTimer = window.setTimeout(dismiss, DURATION);
  chime();
}

function dismiss() {
  if (hideTimer) window.clearTimeout(hideTimer);
  extra.value = 0;
  text.value = "";
  void getCurrentWindow().hide();
}

async function openMain() {
  try {
    const w = await Window.getByLabel("main");
    if (w) {
      await w.unminimize();
      await w.setFocus();
    }
  } catch {
    /* ignore */
  }
  dismiss();
}

onMounted(async () => {
  unlisten = await listen<HostMessage>("host-message", (e) => show(e.payload));
  tick = window.setInterval(() => {
    if (!text.value) return;
    progress.value = Math.max(0, ((deadline - Date.now()) / DURATION) * 100);
  }, 100);
});
onUnmounted(() => {
  if (unlisten) unlisten();
  if (hideTimer) window.clearTimeout(hideTimer);
  if (tick) window.clearInterval(tick);
});
</script>

<template>
  <div class="toast" @click="openMain">
    <div class="accent"></div>
    <div class="head">
      <span class="who">● 主持人 · {{ time }}<span v-if="extra"> · +{{ extra }}</span></span>
      <button class="x" @click.stop="dismiss">✕</button>
    </div>
    <div class="msg">{{ text }}</div>
    <div class="prog"><i :style="{ width: progress + '%' }"></i></div>
  </div>
</template>

<style scoped>
.toast {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  border: 1px solid var(--line-hi);
  cursor: pointer;
  overflow: hidden;
  font-family: var(--sans);
}
.accent {
  height: 3px;
  background: var(--danger);
  flex: 0 0 auto;
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 7px 12px 2px;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--ink-dim);
}
.x {
  appearance: none;
  background: transparent;
  border: none;
  color: var(--ink-faint);
  cursor: pointer;
  font-size: 12px;
  padding: 2px 4px;
}
.msg {
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  padding: 2px 14px 8px;
  font-size: 20px;
  font-weight: 700;
  line-height: 1.2;
  color: var(--ink);
}
.prog {
  flex: 0 0 auto;
  height: 2px;
  background: var(--line);
}
.prog i {
  display: block;
  height: 100%;
  background: var(--ink);
}
</style>
