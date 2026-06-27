<script setup lang="ts">
// 紧急锁按钮。
// size="small" — Setup 模式 / 普通按钮高度。
// size="large" — Live 模式 / 顶部巨型按钮,占满整宽,红色高亮。
import { ref } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";

defineProps<{ size: "small" | "large" }>();

const app = useAppState();
const { controlLocked } = storeToRefs(app);

const lastError = ref<string | null>(null);

async function toggle() {
  lastError.value = null;
  try {
    await app.forceLock(!controlLocked.value);
  } catch (e) {
    lastError.value = String(e);
  }
}
</script>

<template>
  <section class="card">
    <button
      :data-size="$props.size"
      :class="{ danger: controlLocked }"
      @click="toggle"
    >
      {{ controlLocked ? "已紧急锁定(点击释放)" : "紧急锁定" }}
    </button>
    <p v-if="lastError" class="error">{{ lastError }}</p>
  </section>
</template>

<style scoped>
button {
  width: 100%;
  font-family: var(--mono);
  letter-spacing: 0.16em;
  text-transform: uppercase;
  font-weight: 600;
}

button[data-size="small"] {
  padding: 11px 12px;
  font-size: 11px;
}

button[data-size="large"] {
  padding: 28px 16px;
  font-size: 16px;
  letter-spacing: 0.22em;
}

button.danger {
  background: var(--danger);
  color: #fff;
  border-color: var(--danger);
}

.error {
  margin: 10px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
