<script setup lang="ts">
// 主持人访问地址 + 二维码（spec §6.1.2）。
import { ref, watch, onMounted } from "vue";
import { storeToRefs } from "pinia";
import { openUrl } from "@tauri-apps/plugin-opener";

import { useNetworkState } from "../stores/networkState";
import { renderQrCode } from "../utils/qrcode";

const net = useNetworkState();
const { accessUrl, localIp, serverPort } = storeToRefs(net);
const canvas = ref<HTMLCanvasElement | null>(null);

async function draw() {
  if (canvas.value && accessUrl.value) {
    await renderQrCode(accessUrl.value, canvas.value);
  }
}

onMounted(draw);
watch(accessUrl, draw);

async function openInBrowser() {
  if (accessUrl.value) await openUrl(accessUrl.value);
}
</script>

<template>
  <section class="card">
    <h2>主持人访问</h2>
    <div class="net">
      本机 IP：<b>{{ localIp ?? "—" }}</b> · 端口：<b>{{ serverPort }}</b>
    </div>

    <div class="qr-row">
      <canvas ref="canvas" width="160" height="160" class="qr"></canvas>
      <div class="url-box">
        <div class="url" v-if="accessUrl">{{ accessUrl }}</div>
        <div class="url muted" v-else>等待服务地址…</div>
        <button @click="openInBrowser" :disabled="!accessUrl">在浏览器打开</button>
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
  margin-bottom: 12px;
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
