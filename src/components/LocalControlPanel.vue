<script setup lang="ts">
// 导播本地兜底控制（spec §6.1.4）。导播拥有最高优先级。
import { ref } from "vue";
import { storeToRefs } from "pinia";

import { api } from "../utils/api";
import { useAppState } from "../stores/appState";

const app = useAppState();
const { remoteEnabled, controlLocked } = storeToRefs(app);

const lastError = ref<string | null>(null);

// 统一捕获 IPC 失败并给出反馈（非 Windows 下 input 会直接报错）。
async function run(fn: () => Promise<unknown>) {
  lastError.value = null;
  try {
    await fn();
  } catch (e) {
    lastError.value = String(e);
  }
}

const toggleRemote = () => run(() => app.setRemoteEnabled(!remoteEnabled.value));
const toggleLock = () => run(() => app.forceLock(!controlLocked.value));

// 本地翻页直接走 IPC（不受远程开关影响，导播随时可操作）。
const next = () => run(() => api.localNext());
const prev = () => run(() => api.localPrev());
</script>

<template>
  <section class="card">
    <h2>本地控制（导播）</h2>

    <div class="master">
      <button class="big" :class="{ on: remoteEnabled }" @click="toggleRemote">
        远程控制：{{ remoteEnabled ? "已开启" : "已关闭" }}
      </button>
      <button class="big" :class="{ danger: controlLocked }" @click="toggleLock">
        {{ controlLocked ? "已紧急锁定（点击释放）" : "紧急锁定" }}
      </button>
    </div>

    <div class="grid">
      <button @click="prev">上一页</button>
      <button @click="next">下一页</button>
    </div>

    <p v-if="lastError" class="error">{{ lastError }}</p>
  </section>
</template>

<style scoped>
.master {
  display: grid;
  gap: 8px;
  margin-bottom: 10px;
}

.big {
  padding: 11px 12px;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  font-weight: 600;
  width: 100%;
}

/* remote enabled = inverted (active/on state) */
.big.on {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

/* force-lock active = danger red */
.big.danger {
  background: var(--danger);
  color: #fff;
  border-color: var(--danger);
}

.grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 8px;
}

.grid button {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 10px 8px;
}

.error {
  margin: 10px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
