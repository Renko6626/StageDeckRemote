<script setup lang="ts">
// 真实访问地址与二维码；显示模式只选择二维码，不改变后台连接。
import { ref, computed, onMounted, watch } from "vue";
import { storeToRefs } from "pinia";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useNetworkState } from "../stores/networkState";
import { useAppState } from "../stores/appState";
import { renderQrCode } from "../utils/qrcode";
import { relayStatusText } from "../utils/relay";
import ConnectionPanel from "./ConnectionPanel.vue";

const net = useNetworkState();
const app = useAppState();
const { accessUrl, localIp, serverPort } = storeToRefs(net);
const { relayAccessUrl, relayStatus, presenterOnline, presenterSeenMsAgo } = storeToRefs(app);
const canvas = ref<HTMLCanvasElement | null>(null);
const mode = ref<"lan" | "relay">("lan");
const configuring = ref(false);
const error = ref("");
const activeUrl = computed(() => mode.value === "relay" ? relayAccessUrl.value : accessUrl.value);
const summary = computed(() => {
  if (!activeUrl.value) return null;
  try {
    const url = new URL(activeUrl.value);
    return { host: url.host, session: mode.value === "relay" ? url.pathname.split("/")[2] : "局域网直连" };
  } catch { return null; }
});
const presence = computed(() => {
  if (!presenterOnline.value) return "离线";
  const age = presenterSeenMsAgo.value;
  return age != null && age > 3000 ? `弱连接，${Math.floor(age / 1000)}s 前` : "在线";
});

async function draw() {
  const c = canvas.value;
  if (!c) return;
  if (!activeUrl.value) { c.getContext("2d")?.clearRect(0, 0, c.width, c.height); return; }
  try {
    await renderQrCode(activeUrl.value, c);
    error.value = "";
  } catch (e) {
    c.getContext("2d")?.clearRect(0, 0, c.width, c.height);
    error.value = `二维码生成失败：${String(e)}`;
  }
}
onMounted(draw);
watch(activeUrl, draw);
async function openInBrowser() {
  if (!activeUrl.value) return;
  try { await openUrl(activeUrl.value); error.value = ""; }
  catch (e) { error.value = `无法打开浏览器：${String(e)}`; }
}
</script>

<template>
  <section class="card connection-card">
    <header class="connection-head"><h2>主持人连接</h2><button class="configure" @click="configuring = true">配置连接</button></header>
    <div class="mode-toggle" aria-label="主持人二维码类型">
      <button :class="{ active: mode === 'lan' }" :aria-pressed="mode === 'lan'" @click="mode = 'lan'">局域网</button>
      <button :class="{ active: mode === 'relay' }" :aria-pressed="mode === 'relay'" @click="mode = 'relay'">Relay 中继</button>
    </div>
    <div class="qr-row">
      <div class="qr-box">
        <canvas v-show="activeUrl" ref="canvas" width="112" height="112" class="qr" aria-label="主持人访问二维码"></canvas>
        <div v-if="!activeUrl" class="qr-placeholder">等待连接</div>
      </div>
      <div class="connection-info">
        <strong :class="{ failed: mode === 'relay' && relayStatus.startsWith('error:') }">{{ mode === 'relay' ? relayStatusText(relayStatus) : '局域网直连' }}</strong>
        <template v-if="summary"><span>{{ summary.session }}</span><span class="host">{{ summary.host }}</span></template>
        <span v-else>{{ mode === 'relay' ? '连接成功后显示二维码' : '等待本机服务地址' }}</span>
        <button class="open-browser" :disabled="!activeUrl" @click="openInBrowser">在浏览器打开</button>
      </div>
    </div>
    <p class="hint">{{ mode === 'lan' ? '手机与导播连接同一局域网后扫码。' : '手机与导播均需能访问 Relay 服务器。' }}</p>
    <div class="presence"><span>主持人</span><span :class="{ weak: presenterOnline && presenterSeenMsAgo != null && presenterSeenMsAgo > 3000 }">{{ presence }}</span></div>
    <details class="connection-details"><summary>访问地址与本机信息</summary><p>本机 IP：{{ localIp ?? '—' }}<br />服务端口：{{ serverPort }}</p><p v-if="activeUrl" class="url">{{ activeUrl }}</p><p v-else>当前没有可用访问地址。</p></details>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <ConnectionPanel v-if="configuring" :mode="mode" @close="configuring = false" @select-mode="mode = $event" />
  </section>
</template>

<style scoped>
.connection-card { border-color: #62626c; }
.connection-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-bottom: 11px; }
.connection-head h2 { margin: 0; font-family: var(--sans); font-size: 13px; font-weight: 600; letter-spacing: normal; }
.configure { padding: 6px 10px; font-size: 11px; background: #24242b; border-color: #73737e; }
.mode-toggle { display: flex; gap: 0; margin-bottom: 12px; }
.mode-toggle button { font-size: 11px; padding: 5px 10px; color: var(--ink-dim); }
.mode-toggle .active { background: var(--ink); border-color: var(--ink); color: #111; }
.qr-row { display: grid; grid-template-columns: 112px minmax(0, 1fr); gap: 13px; align-items: center; }
.qr-box { width: 112px; height: 112px; }
.qr { display: block; width: 112px; height: 112px; background: white; }
.qr-placeholder { display: grid; place-items: center; width: 100%; height: 100%; background: var(--panel-2); border: 1px solid var(--line-hi); color: var(--ink-faint); font-size: 11px; }
.connection-info { display: flex; flex-direction: column; align-items: flex-start; min-width: 0; gap: 7px; }
.connection-info strong { font-size: 12px; font-weight: 600; }
.connection-info span { color: var(--ink-dim); font-size: 11px; overflow-wrap: anywhere; }
.host { font-family: var(--mono); }
.open-browser { padding: 3px 0; border: 0; text-decoration: underline; text-underline-offset: 4px; font-size: 11px; }
.hint { margin: 10px 0 0; font-size: 11px; color: var(--ink-dim); line-height: 1.6; }
.presence { display: flex; justify-content: space-between; padding-top: 10px; margin-top: 10px; border-top: 1px solid var(--line); font-size: 11px; color: var(--ink-dim); }
.weak { color: #e6c083; }
.connection-details { margin-top: 10px; font-size: 11px; color: var(--ink-dim); }
summary { cursor: pointer; }
.connection-details p { line-height: 1.7; margin: 9px 0 0; }
.url { font-family: var(--mono); overflow-wrap: anywhere; }
.error, .failed { color: #f19a9a; }
.error { font-size: 11px; overflow-wrap: anywhere; }
</style>
