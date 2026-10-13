<script setup lang="ts">
// 导播设置模态:重置访问码 / 服务端口(重启生效)/ Relay 配置 / 提示音开关。
import { ref, onMounted } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";
import { useNetworkState } from "../stores/networkState";

const emit = defineEmits<{ close: [] }>();
const app = useAppState();
const net = useNetworkState();
const { serverPort } = storeToRefs(net);

const chimeOn = ref(localStorage.getItem("sd.dir.toastChime") !== "off");
const portInput = ref<number>(serverPort.value || 8787);
const confirmReset = ref(false);
const note = ref("");

// Relay 表单字段
const relayBase = ref("");
const relaySid = ref("");
const relaySecret = ref("");

function flash(t: string) {
  note.value = t;
  window.setTimeout(() => (note.value = ""), 2400);
}
function toggleChime() {
  chimeOn.value = !chimeOn.value;
  localStorage.setItem("sd.dir.toastChime", chimeOn.value ? "on" : "off");
}
async function resetToken() {
  if (!confirmReset.value) {
    confirmReset.value = true;
    return;
  }
  confirmReset.value = false;
  try {
    await app.regenerateToken();
    flash("访问码已重置 — 主持人需重新扫码");
  } catch {
    flash("重置失败 — 请重试");
  }
}
async function savePort() {
  // 空/非法输入(清空输入框会得到 NaN)直接拒绝,不静默写入错误端口。
  const raw = Math.floor(portInput.value);
  if (!Number.isFinite(raw) || raw < 1 || raw > 65535) {
    flash("请输入有效端口(1–65535)");
    return;
  }
  portInput.value = raw;
  try {
    await app.setServerPort(raw);
    flash("端口已保存 — 重启后生效");
  } catch {
    flash("端口保存失败 — 请重试");
  }
}

// 打开模态时加载 relay 配置
onMounted(async () => {
  try {
    await app.loadRelay();
    if (app.relayConfig) {
      relayBase.value = app.relayConfig.base_url;
      relaySid.value = app.relayConfig.session_id;
      relaySecret.value = app.relayConfig.secret;
    }
  } catch (error) {
    flash(`无法读取 Relay 配置：${String(error)}`);
  }
});

async function saveRelay() {
  relayBase.value = relayBase.value.trim();
  relaySid.value = relaySid.value.trim();
  if (!/^[A-Z0-9-]{4,16}$/.test(relaySid.value)) {
    flash("Session ID 必须匹配 [A-Z0-9-]{4,16}");
    return;
  }
  try {
    await app.saveRelay({
      base_url: relayBase.value,
      session_id: relaySid.value,
      secret: relaySecret.value,
    });
    flash("Relay 配置已保存 — 重启导播生效");
  } catch (error) {
    flash(String(error));
  }
}

async function clearRelay() {
  try {
    await app.saveRelay(null);
    relayBase.value = "";
    relaySid.value = "";
    relaySecret.value = "";
    flash("已保存关闭设置，重启导播后停止 Relay 连接");
  } catch {
    flash("操作失败 — 请重试");
  }
}
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <section class="modal">
      <header class="mhead">
        <span>设置 · SETTINGS</span>
        <button class="x" @click="emit('close')">✕</button>
      </header>

      <div class="grp">
        <div class="glabel">访问码</div>
        <p class="desc">重置后已连接的主持人立即断开,需重新扫码。</p>
        <button class="danger" @click="resetToken">
          {{ confirmReset ? "确认重置?(踢掉所有主持人)" : "重置访问码" }}
        </button>
      </div>

      <div class="grp">
        <div class="glabel">服务端口</div>
        <p class="desc">改端口需重启应用才生效;当前运行端口 {{ serverPort }}。</p>
        <div class="prow">
          <input type="number" v-model.number="portInput" min="1" max="65535" />
          <button @click="savePort">保存(重启生效)</button>
        </div>
      </div>

      <div class="grp">
        <div class="glabel">提示音</div>
        <button class="chime" :class="{ on: chimeOn }" @click="toggleChime">
          收到主持人消息提示音：{{ chimeOn ? "开" : "关" }}
        </button>
      </div>

      <div class="grp">
        <div class="glabel">Relay</div>
        <p class="desc">
          <span class="relay-status" :class="'rs-' + app.relayStatus.split(':')[0]">●</span>
          {{ app.relayStatus }}

        </p>
        <p class="desc">下面是已保存配置。修改后重启生效，当前连接和二维码继续使用启动时配置。</p>
        <div class="prow">
          <input v-model="relayBase" placeholder="ws://stagedeck.example.com:8080" />
        </div>
        <div class="prow">
          <input v-model="relaySid" placeholder="Session ID（如 PROM-2026）" />
        </div>
        <div class="prow">
          <input v-model="relaySecret" placeholder="预共享密钥" type="password" />
        </div>
        <div class="prow relay-btns">
          <button @click="saveRelay">保存</button>
          <button @click="clearRelay" class="danger relay-clear">关闭 Relay</button>
        </div>
      </div>

      <p v-if="note" class="note">{{ note }}</p>
    </section>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: rgba(6, 6, 8, 0.66);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
}
.modal {
  width: 100%;
  max-width: 420px;
  max-height: calc(100dvh - 32px);
  overflow-y: auto;
  background: var(--panel);
  border: 1px solid var(--line-hi);
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.6);
}
.mhead {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--line-hi);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.2em;
  text-transform: uppercase;
  color: var(--ink-dim);
}
.x {
  border: none;
  background: transparent;
  color: var(--ink-faint);
  font-size: 14px;
  padding: 2px 6px;
}
.grp {
  padding: 14px 16px;
  border-bottom: 1px solid var(--line);
}
.glabel {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.2em;
  text-transform: uppercase;
  color: var(--ink-faint);
  margin-bottom: 6px;
}
.desc {
  margin: 0 0 9px;
  font-size: 12px;
  color: var(--ink-dim);
  line-height: 1.45;
}
.danger {
  width: 100%;
  border-color: var(--danger);
  color: var(--danger);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 9px;
}
.danger:hover:not(:disabled) {
  background: var(--danger);
  color: #fff;
  border-color: var(--danger);
}
.prow {
  display: flex;
  gap: 8px;
}
.prow input {
  flex: 1 1 auto;
  min-width: 0;
  background: var(--panel-2);
  border: 1px solid var(--line-hi);
  color: var(--ink);
  font-family: var(--mono);
  font-size: 13px;
  padding: 8px 10px;
}
.prow button {
  flex: 0 0 auto;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 8px 10px;
}
.chime {
  width: 100%;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.1em;
  padding: 9px;
}
.chime.on {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}
.note {
  margin: 0;
  padding: 12px 16px;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--ink);
}
.relay-status {
  font-size: 10px;
  margin-right: 4px;
}
.rs-disabled { color: var(--ink-faint); }
.rs-connecting { color: #f0a030; }
.rs-connected { color: #40c870; }
.rs-error { color: var(--danger); }
.relay-btns {
  margin-top: 4px;
}
.relay-clear {
  flex: 0 0 auto;
  width: auto;
  padding: 8px 12px;
  font-size: 10px;
}
</style>
