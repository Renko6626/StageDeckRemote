<script setup lang="ts">
// 导播端主面板：组合各控制组件，并定时轮询后端状态（spec §12.1）。
import { onMounted, onUnmounted } from "vue";

import { useAppState } from "../stores/appState";
import ControlStatus from "../components/ControlStatus.vue";
import WindowPicker from "../components/WindowPicker.vue";
import QrCodePanel from "../components/QrCodePanel.vue";
import LocalControlPanel from "../components/LocalControlPanel.vue";
import LogPanel from "../components/LogPanel.vue";
import ComStatusPanel from "../components/ComStatusPanel.vue";

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
    <div class="col">
      <WindowPicker />
      <QrCodePanel />
    </div>
    <div class="col">
      <ComStatusPanel />
      <ControlStatus />
      <LocalControlPanel />
    </div>
    <div class="col">
      <LogPanel />
    </div>
  </div>
</template>

<style scoped>
.dashboard {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
  padding: 16px;
  align-items: start;
}

.col {
  display: grid;
  gap: 16px;
}

@media (max-width: 980px) {
  .dashboard {
    grid-template-columns: 1fr;
  }
}
</style>
