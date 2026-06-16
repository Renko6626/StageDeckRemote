<script setup lang="ts">
// 应用外壳：标题栏 + 齿轮设置 + 路由出口。
import { ref } from "vue";

import SettingsModal from "./components/SettingsModal.vue";

const settingsOpen = ref(false);
</script>

<template>
  <router-view v-if="$route.meta.bare" />
  <div v-else class="app">
    <header class="topbar">
      <strong class="brand">STAGEDECK<span class="brand-sub">&nbsp;·&nbsp;DIRECTOR</span></strong>
      <button class="gear" @click="settingsOpen = true" title="设置" aria-label="设置">⚙</button>
    </header>
    <router-view />
    <SettingsModal v-if="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>

<style>
:root {
  --bg: #0a0a0b;
  --panel: #0e0e10;
  --panel-2: #131316;
  --line: #26262b;
  --line-hi: #3a3a42;
  --ink: #ededee;
  --ink-dim: #a2a2aa;
  --ink-faint: #82828b;
  --danger: #d23b3b;
  --sans: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
    "Microsoft YaHei", system-ui, sans-serif;
  --mono: ui-monospace, "Cascadia Mono", "JetBrains Mono", "SF Mono",
    "Segoe UI Mono", Menlo, Consolas, monospace;
}

* {
  box-sizing: border-box;
}

html,
body {
  margin: 0;
  min-height: 100%;
  background: var(--bg);
  color: var(--ink);
  font-family: var(--sans);
}

body::before {
  content: "";
  position: fixed;
  inset: 0;
  background-image:
    linear-gradient(var(--line) 1px, transparent 1px),
    linear-gradient(90deg, var(--line) 1px, transparent 1px);
  background-size: 28px 28px;
  opacity: 0.16;
  pointer-events: none;
  z-index: 0;
}

.app {
  position: relative;
  z-index: 1;
  min-height: 100vh;
}

.topbar {
  display: flex;
  align-items: center;
  padding: 10px 18px;
  border-bottom: 1px solid var(--line-hi);
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
}

.topbar .brand {
  font-weight: 700;
  letter-spacing: 0.18em;
  color: var(--ink);
}

.topbar .brand-sub {
  color: var(--ink-faint);
  font-weight: 400;
}

.topbar .gear {
  margin-left: auto;
  font-size: 14px;
  line-height: 1;
  padding: 5px 10px;
  color: var(--ink-dim);
}

.card {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 0;
  padding: 14px 16px;
}

.card h2 {
  margin: 0 0 12px;
  font-family: var(--mono);
  font-size: 10px;
  font-weight: 400;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  color: var(--ink-dim);
}

button {
  appearance: none;
  border: 1px solid var(--line-hi);
  background: transparent;
  color: var(--ink);
  border-radius: 0;
  padding: 9px 12px;
  font-size: 14px;
  font-family: var(--sans);
  cursor: pointer;
  transition: background 0.06s linear, color 0.06s linear, border-color 0.06s linear;
}

button:hover:not(:disabled) {
  background: var(--panel-2);
  border-color: var(--ink-dim);
}

button:active:not(:disabled) {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

button:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
</style>
