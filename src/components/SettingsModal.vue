<script setup lang="ts">
// 通用设置：重置访问码、服务端口（重启生效）、提示音。
import { ref } from "vue";
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

</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <section class="modal">
      <header class="mhead">
        <span>通用设置</span>
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
</style>
