<script setup lang="ts">
// 临时 VBA spike 面板：探测当前 PPT 的 VBA 宏，点一个触发（导播本机 IPC）。
import { ref } from "vue";
import { api } from "../utils/api";

const macros = ref<string[]>([]);
const error = ref<string | null>(null);
const loading = ref(false);

async function detect() {
  loading.value = true;
  error.value = null;
  try {
    macros.value = await api.vbaListMacros();
  } catch (e) {
    error.value = String(e);
    macros.value = [];
  } finally {
    loading.value = false;
  }
}

async function run(name: string) {
  error.value = null;
  try {
    await api.vbaRunMacro(name);
  } catch (e) {
    error.value = String(e);
  }
}
</script>

<template>
  <section class="card">
    <h2>VBA 探测（spike）</h2>
    <button @click="detect" :disabled="loading">{{ loading ? "探测中…" : "探测 VBA 宏" }}</button>
    <p v-if="error" class="error">{{ error }}</p>
    <ul v-if="macros.length" class="macros">
      <li v-for="m in macros" :key="m">
        <button class="macro" @click="run(m)">{{ m }}</button>
      </li>
    </ul>
    <p v-else-if="!loading && !error" class="hint">（点上面按钮列出当前 .pptm 的宏；需开启信任设置）</p>
  </section>
</template>

<style scoped>
.macros {
  list-style: none;
  padding: 0;
  margin: 10px 0 0;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.macro {
  background: #1f6f43;
}
.error {
  color: #ff453a;
  font-size: 13px;
  word-break: break-all;
}
.hint {
  font-size: 12px;
  opacity: 0.5;
}
</style>
