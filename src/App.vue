<script setup lang="ts">
// 应用外壳：标题栏 + 路由出口。
</script>

<template>
  <div class="app">
    <header class="topbar">
      <strong>StageDeck Remote</strong>
      <span class="subtitle">导播端控制台</span>
    </header>
    <router-view />
  </div>
</template>

<style>
:root {
  font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  color: #e9e9f0;
  background: #0e0e12;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
}

.app {
  min-height: 100vh;
}

.topbar {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 12px 18px;
  background: #15151c;
  border-bottom: 1px solid #26262f;
}
.topbar .subtitle {
  font-size: 13px;
  opacity: 0.55;
}

.card {
  background: #16161d;
  border: 1px solid #26262f;
  border-radius: 14px;
  padding: 16px;
}
.card h2 {
  margin: 0 0 12px;
  font-size: 15px;
  font-weight: 600;
  opacity: 0.85;
}

button {
  appearance: none;
  border: 1px solid #33333f;
  background: #232330;
  color: #e9e9f0;
  border-radius: 9px;
  padding: 9px 12px;
  font-size: 14px;
  font-family: inherit;
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s;
}
button:hover:not(:disabled) {
  background: #2c2c3b;
  border-color: #44445a;
}
button:active:not(:disabled) {
  background: #1d1d28;
}
button:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
</style>
