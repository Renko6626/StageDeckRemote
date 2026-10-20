<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useAppState } from "../stores/appState";
import { useNetworkState } from "../stores/networkState";
import { normalizeRelayAddress, relayStatusText } from "../utils/relay";

const props = defineProps<{ mode: "lan" | "relay" }>();
const emit = defineEmits<{ close: []; selectMode: [mode: "lan" | "relay"] }>();
const app = useAppState();
const net = useNetworkState();
const dialog = ref<HTMLDialogElement | null>(null);
const tab = ref(props.mode);
const address = ref("");
const session = ref("");
const secret = ref("");
const showSecret = ref(false);
const loading = ref(true);
const loadFailed = ref(false);
const saving = ref(false);
const note = ref("");
const failed = ref(false);
const saved = ref(false);
const addressPreview = computed(() => {
  if (!address.value.trim()) return "可粘贴带前缀或不带前缀的服务器地址";
  try { return `连接地址：${normalizeRelayAddress(address.value)}`; }
  catch (error) { return (error as Error).message; }
});
const connectionError = computed(() => {
  if (!app.relayStatus.startsWith("error:")) return "";
  const reason = app.relayStatus.slice(6).trim();
  return reason === "secret_mismatch"
    ? "此会话已使用其他密钥，请核对密钥或更换会话编号。"
    : `无法连接服务器，请检查地址、网络和服务器是否启动。详细原因：${reason}`;
});
const activeRelay = computed(() => {
  if (!app.relayAccessUrl) return null;
  try {
    const url = new URL(app.relayAccessUrl);
    return `${url.pathname.split("/")[2]}，${url.host}`;
  } catch { return null; }
});

async function load() {
  loading.value = true;
  loadFailed.value = false;
  try {
    await app.loadRelay();
    address.value = app.relayConfig?.base_url ?? "";
    session.value = app.relayConfig?.session_id ?? "";
    secret.value = app.relayConfig?.secret ?? "";
  } catch (error) {
    note.value = `无法读取已保存配置：${String(error)}`;
    failed.value = true;
    loadFailed.value = true;
  } finally { loading.value = false; }
}

async function save() {
  if (tab.value === "lan") {
    emit("selectMode", "lan");
    emit("close");
    return;
  }
  failed.value = false;
  note.value = "";
  try {
    const normalized = normalizeRelayAddress(address.value);
    const sid = session.value.trim();
    if (!/^[A-Z0-9-]{4,16}$/.test(sid)) throw new Error("会话编号需为 4–16 位大写字母、数字或连字符");
    if (!secret.value.trim()) throw new Error("请填写连接密钥");
    saving.value = true;
    await app.saveRelay({ base_url: normalized, session_id: sid, secret: secret.value });
    address.value = normalized;
    session.value = sid;
    saved.value = true;
    note.value = "配置已保存，重启导播后生效。当前连接和二维码保持不变。";
  } catch (error) {
    failed.value = true;
    note.value = String(error instanceof Error ? error.message : error);
  } finally { saving.value = false; }
}

async function disableRelay() {
  saving.value = true;
  failed.value = false;
  try {
    await app.saveRelay(null);
    address.value = session.value = secret.value = "";
    saved.value = true;
    note.value = "已保存停用设置，重启导播后停止 Relay 连接。当前二维码保持不变。";
  } catch (error) {
    failed.value = true;
    note.value = `停用设置保存失败：${String(error)}`;
  } finally { saving.value = false; }
}

function backdropClick(event: MouseEvent) {
  if (event.target !== dialog.value || !dialog.value) return;
  const rect = dialog.value.getBoundingClientRect();
  if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) emit("close");
}

onMounted(() => { dialog.value?.showModal(); void load(); });
onBeforeUnmount(() => { dialog.value?.close(); });
</script>

<template>
  <dialog ref="dialog" class="connection-panel" aria-labelledby="connection-title" @cancel.prevent="emit('close')" @click="backdropClick">
    <header class="panel-head">
      <div><h1 id="connection-title">主持人连接</h1><p>选择连接方式，配置 Relay 服务器。</p></div>
      <button class="close" aria-label="关闭连接配置" @click="emit('close')">×</button>
    </header>
    <div class="panel-body">
      <div class="mode-tabs" aria-label="连接方式">
        <button :class="{ active: tab === 'lan' }" :aria-pressed="tab === 'lan'" @click="tab = 'lan'">局域网直连</button>
        <button :class="{ active: tab === 'relay' }" :aria-pressed="tab === 'relay'" @click="tab = 'relay'">Relay 中继</button>
      </div>
      <div class="running-status">
        <strong :class="{ error: connectionError }">{{ relayStatusText(app.relayStatus) }}</strong>
        <p v-if="activeRelay">当前运行：{{ activeRelay }}</p>
        <p v-if="connectionError" class="error">{{ connectionError }}</p>
      </div>
      <template v-if="tab === 'relay'">
        <p v-if="loading" class="hint">正在读取已保存配置…</p>
        <button v-if="loadFailed" @click="load">重新读取配置</button>
        <fieldset :disabled="loading || loadFailed || saving">
          <div class="field"><label for="relay-server">服务器地址</label>
            <input id="relay-server" v-model="address" placeholder="http://服务器IP:8080" spellcheck="false" autocomplete="off" />
            <p>自动将 HTTP、HTTPS、WS、WSS 前缀统一为 WS。</p><p class="address-preview">{{ addressPreview }}</p>
          </div>
          <div class="field"><label for="relay-session">会话编号</label>
            <input id="relay-session" v-model="session" placeholder="例如 STAGE-A" maxlength="16" spellcheck="false" autocomplete="off" />
            <p>4–16 位大写字母、数字或连字符。每台导播使用不同编号，重复编号与密钥会互相接管。</p>
          </div>
          <div class="field"><label for="relay-secret">连接密钥</label>
            <div class="secret-input"><input id="relay-secret" v-model="secret" :type="showSecret ? 'text' : 'password'" autocomplete="off" />
              <button :aria-label="showSecret ? '隐藏连接密钥' : '显示连接密钥'" @click="showSecret = !showSecret">{{ showSecret ? '隐藏' : '显示' }}</button></div>
            <p>用于导播重新连接此会话，与手机扫码访问码不同。</p>
          </div>
        </fieldset>
        <p v-if="saved" class="hint">以下为已保存、待重启生效的配置。</p>
        <p v-else class="hint">此表单展示已保存配置；当前连接仍使用启动时配置。</p>
      </template>
      <div v-else class="lan-info">
        <p>手机与导播连接同一局域网后，扫描主界面的二维码。</p>
        <p>本机 IP：{{ net.localIp ?? '等待服务地址' }}<br />服务端口：{{ net.serverPort }}</p>
        <p>选择局域网二维码不会关闭 Relay 连接。</p>
      </div>
      <p v-if="note" class="feedback" :class="{ error: failed }" role="status">{{ note }}</p>
    </div>
    <footer class="panel-foot">
      <p>{{ tab === 'relay' ? '保存后需重启导播，重启前连接与二维码保持不变。' : '局域网与 Relay 可同时使用，只切换展示给主持人的二维码。' }}</p>
      <div class="buttons">
        <button v-if="tab === 'relay'" class="disable" :disabled="loading || loadFailed || saving || !app.relayConfig" @click="disableRelay">停用 Relay（重启生效）</button>
        <button class="primary" :disabled="saving || (tab === 'relay' && (loading || loadFailed))" @click="save">{{ saving ? '正在保存…' : tab === 'relay' ? '保存配置' : '使用局域网二维码' }}</button>
      </div>
    </footer>
  </dialog>
</template>

<style scoped>
.connection-panel { position: fixed; inset: 0 0 0 auto; margin: 0; width: 470px; max-width: 100vw; height: 100dvh; max-height: 100dvh; padding: 0; color: var(--ink); background: var(--panel-2); border: 0; border-left: 1px solid var(--line-hi); }
.connection-panel[open] { display: flex; flex-direction: column; }
.connection-panel::backdrop { background: #0009; }
.panel-head { display: flex; justify-content: space-between; align-items: center; padding: 22px 24px; border-bottom: 1px solid var(--line); }
h1 { margin: 0 0 8px; font-size: 18px; font-weight: 600; }
.panel-head p, .panel-foot p, .hint { margin: 0; color: var(--ink-dim); font-size: 12px; line-height: 1.6; }
.close { border: 0; background: transparent; font-size: 25px; padding: 0 4px; }
.panel-body { padding: 22px 24px; overflow-y: auto; flex: 1; min-height: 0; }
.mode-tabs { display: flex; margin-bottom: 22px; }
.mode-tabs button { flex: 1; font-size: 13px; padding: 9px 12px; }
.mode-tabs .active, .primary { color: #111; background: var(--ink); border-color: var(--ink); font-weight: 600; }
.running-status { border-left: 2px solid var(--ink-faint); padding-left: 12px; margin-bottom: 22px; font-size: 12px; }
.running-status strong { font-weight: 500; }
.running-status p { margin: 7px 0 0; color: var(--ink-dim); font-size: 11px; line-height: 1.6; overflow-wrap: anywhere; }
fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
.field { margin-bottom: 20px; }
label { display: block; font-weight: 600; margin-bottom: 9px; font-size: 13px; }
input { width: 100%; min-width: 0; height: 41px; background: var(--bg); border: 1px solid var(--line-hi); color: var(--ink); padding: 0 11px; font-size: 13px; }
.field p { color: var(--ink-dim); font-size: 11px; line-height: 1.7; margin: 8px 0 0; }
.address-preview { font-family: var(--mono); overflow-wrap: anywhere; }
.secret-input { display: flex; }
.secret-input input { border-right: 0; }
.secret-input button { flex: 0 0 auto; white-space: nowrap; font-size: 11px; }
.lan-info { color: var(--ink-dim); font-size: 13px; line-height: 1.8; }
.feedback { margin: 14px 0 0; font-size: 12px; line-height: 1.7; overflow-wrap: anywhere; }
.error, .running-status p.error { color: #f19a9a; }
.panel-foot { padding: 18px 24px; border-top: 1px solid var(--line); }
.panel-foot p { font-size: 11px; margin-bottom: 14px; }
.buttons { display: flex; justify-content: space-between; gap: 10px; }
.buttons button { font-size: 12px; padding: 9px 10px; }
.disable { color: var(--ink-dim); }
button:focus-visible, input:focus-visible { outline: 2px solid var(--ink); outline-offset: 3px; }
@media (max-width: 480px) { .connection-panel { width: 100%; } .panel-head, .panel-body, .panel-foot { padding: 18px; } .buttons { flex-wrap: wrap; } }
</style>
