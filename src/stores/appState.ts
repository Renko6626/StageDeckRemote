// 导播端运行状态（spec §6.1.3）：控制权限、目标窗口、日志，以及轮询刷新。
import { defineStore } from "pinia";
import { ref } from "vue";

import { api, type DirectorStatus, type LogEntry, type TargetWindow, type TimerView } from "../utils/api";
import { useNetworkState } from "./networkState";

export const useAppState = defineStore("app", () => {
  const remoteEnabled = ref(false);
  const controlLocked = ref(true);
  const presenterUnlocked = ref(false);
  const selectedWindow = ref<TargetWindow | null>(null);
  const activeClientId = ref<string | null>(null);
  const logs = ref<LogEntry[]>([]);
  const timer = ref<TimerView | null>(null);
  const presenterOnline = ref(false);

  function applyStatus(s: DirectorStatus) {
    remoteEnabled.value = s.remote_enabled;
    controlLocked.value = s.control_locked;
    presenterUnlocked.value = s.presenter_unlocked;
    selectedWindow.value = s.selected_window;
    activeClientId.value = s.active_client_id;
    timer.value = s.timer;
    presenterOnline.value = s.presenter_online;

    const net = useNetworkState();
    net.update({
      localIp: s.local_ip,
      serverPort: s.server_port,
      token: s.token,
      accessUrl: s.access_url,
    });
  }

  /** 拉取一次后端状态 + 日志，更新 store。 */
  async function refresh() {
    const net = useNetworkState();
    try {
      const [status, logEntries] = await Promise.all([api.getStatus(), api.getLogs()]);
      applyStatus(status);
      logs.value = logEntries;
      net.online = true;
    } catch (e) {
      net.online = false;
      console.error("refresh failed", e);
    }
  }

  // --- 导播操作（封装 IPC，调用后立即刷新一次） ---
  async function setRemoteEnabled(enabled: boolean) {
    await api.setRemoteEnabled(enabled);
    await refresh();
  }
  async function forceLock(locked: boolean) {
    await api.forceLock(locked);
    await refresh();
  }
  async function selectWindow(window: TargetWindow) {
    await api.selectWindow(window);
    await refresh();
  }
  async function autoDetectPpt() {
    await api.autoDetectPpt();
    await refresh();
  }
  async function timerStart(ms: number) {
    await api.timerStart(ms);
    await refresh();
  }
  async function timerToggle() {
    await api.timerToggle();
    await refresh();
  }
  async function timerReset() {
    await api.timerReset();
    await refresh();
  }
  async function releaseController() {
    await api.releaseController();
    await refresh();
  }

  return {
    remoteEnabled,
    controlLocked,
    presenterUnlocked,
    selectedWindow,
    activeClientId,
    logs,
    refresh,
    setRemoteEnabled,
    forceLock,
    selectWindow,
    autoDetectPpt,
    timer,
    timerStart,
    timerToggle,
    timerReset,
    presenterOnline,
    releaseController,
  };
});
