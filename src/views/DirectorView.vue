<script setup lang="ts">
// 导播端主面板:方案 B —— 左控制栏 | 右(预览↑ 日志↓)。定时轮询后端状态(spec §12.1)。
import { onMounted, onUnmounted } from "vue";

import { useAppState } from "../stores/appState";
import ControlStatus from "../components/ControlStatus.vue";
import WindowPicker from "../components/WindowPicker.vue";
import QrCodePanel from "../components/QrCodePanel.vue";
import LocalControlPanel from "../components/LocalControlPanel.vue";
import LogPanel from "../components/LogPanel.vue";
import DirectorPreview from "../components/DirectorPreview.vue";

const app = useAppState();
let timer: number | undefined;

onMounted(async () => {
  await app.refresh();
  // 启动时未选窗口才自动识别;手动选择为权威,绝不被自动悄悄覆盖。
  if (!app.selectedWindow) {
    try {
      await app.autoDetectPpt();
    } catch {
      /* 自动识别失败不阻塞启动 */
    }
  }
  timer = window.setInterval(() => void app.refresh(), 1500);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <div class="dashboard">
    <aside class="rail">
      <WindowPicker />
      <ControlStatus />
      <LocalControlPanel />
      <QrCodePanel />
    </aside>
    <main class="main">
      <DirectorPreview />
      <LogPanel />
    </main>
  </div>
</template>

<style scoped>
.dashboard {
  display: grid;
  grid-template-columns: 340px minmax(0, 1fr);
  gap: 16px;
  padding: 16px;
  height: calc(100vh - 38px); /* 减去顶栏(App.vue .topbar,实测约 36px)高度 */
  overflow: hidden;
}
.rail {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  overflow-y: auto;
}
.main {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
}
/* 预览吃满剩余高度;日志贴底(自然高度,其内部列表自带 220px 滚动)。 */
.main > :first-child {
  flex: 1 1 0;
  min-height: 0;
}
.main > :last-child {
  flex: 0 0 auto;
}

@media (max-width: 980px) {
  .dashboard {
    grid-template-columns: 1fr;
    height: auto;
    overflow: visible;
  }
  /* 堆叠时预览给固定 16:9 高度 */
  .main > :first-child {
    flex: none;
    height: 56.25vw;
  }
}
</style>
