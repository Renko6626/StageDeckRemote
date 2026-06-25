<script setup lang="ts">
// 主持人访问地址 + 二维码（spec §6.1.2）。LAN / Relay 两路切换。
import { ref, computed, watch, onMounted } from "vue";
import { storeToRefs } from "pinia";
import { openUrl } from "@tauri-apps/plugin-opener";

import { useNetworkState } from "../stores/networkState";
import { useAppState } from "../stores/appState";
import { renderQrCode } from "../utils/qrcode";

const net = useNetworkState();
const app = useAppState();
const { accessUrl, localIp, serverPort } = storeToRefs(net);
const { relayAccessUrl } = storeToRefs(app);
const canvas = ref<HTMLCanvasElement | null>(null);

// LAN / Relay 显示模式
const mode = ref<"lan" | "relay">("lan");

const activeUrl = computed(() =>
  mode.value === "relay" ? relayAccessUrl.value : accessUrl.value
);

function clearCanvas() {
  const c = canvas.value;
  if (!c) return;
  const ctx = c.getContext("2d");
  if (ctx) ctx.clearRect(0, 0, c.width, c.height);
}

async function draw() {
  if (!canvas.value) return;
  if (!activeUrl.value) {
    clearCanvas();
    return;
  }
  try {
    await renderQrCode(activeUrl.value, canvas.value);
  } catch (e) {
    console.error("QR render failed:", e);
    clearCanvas();
  }
}

onMounted(draw);
watch(activeUrl, draw);

async function openInBrowser() {
  if (activeUrl.value) await openUrl(activeUrl.value);
}
</script>

<template>
  <section class="card">
    <h2>主持人访问</h2>
    <div class="net">
      本机 IP：<b>{{ localIp ?? "—" }}</b> · 端口：<b>{{ serverPort }}</b>
    </div>

    <div class="mode-toggle">
      <button
        :class="{ active: mode === 'lan' }"
        @click="mode = 'lan'"
      >LAN</button>
      <button
        :class="{ active: mode === 'relay' }"
        :disabled="!relayAccessUrl"
        @click="mode = 'relay'"
      >Relay</button>
    </div>

    <div class="qr-row">
      <canvas ref="canvas" width="160" height="160" class="qr"></canvas>
      <div class="url-box">
        <div class="url" v-if="activeUrl">{{ activeUrl }}</div>
        <div class="url muted" v-else-if="mode === 'relay'">Relay 未连接</div>
        <div class="url muted" v-else>等待服务地址…</div>
        <button @click="openInBrowser" :disabled="!activeUrl">在浏览器打开</button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.net {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.1em;
  color: var(--ink-faint);
  margin-bottom: 10px;
}

.mode-toggle {
  display: flex;
  gap: 6px;
  margin-bottom: 12px;
}

.mode-toggle button {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 5px 12px;
  opacity: 0.5;
}

.mode-toggle button.active {
  opacity: 1;
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

.mode-toggle button:disabled {
  opacity: 0.25;
  cursor: not-allowed;
}

.net b {
  color: var(--ink);
  font-weight: 600;
}

.qr-row {
  display: flex;
  gap: 14px;
  align-items: flex-start;
}

.qr {
  border: 1px solid var(--line-hi);
  border-radius: 0;
  flex: 0 0 auto;
  background: #fff;
}

.url-box {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.url {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.04em;
  color: var(--ink);
  word-break: break-all;
}

.muted {
  color: var(--ink-faint);
}

.url-box button {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px 10px;
  align-self: flex-start;
}
</style>
