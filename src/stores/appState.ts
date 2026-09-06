// 导播端运行状态（spec §6.1.3）：控制权限、目标窗口、日志，以及轮询刷新。
import { defineStore } from "pinia";
import { ref } from "vue";

import { api, type DirectorStatus, type LogEntry, type RelayConfigDto, type TargetWindow, type TimerView } from "../utils/api";
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
  const presenterSeenMsAgo = ref<number | null>(null);

  // Relay 状态
  const relayConfig = ref<RelayConfigDto | null>(null);
  const relayStatus = ref<string>("disabled");
  const relayAccessUrl = ref<string | null>(null);

  function applyStatus(s: DirectorStatus) {
    remoteEnabled.value = s.remote_enabled;
    controlLocked.value = s.control_locked;
    presenterUnlocked.value = s.presenter_unlocked;
    selectedWindow.value = s.selected_window;
    activeClientId.value = s.active_client_id;
    timer.value = s.timer;
    presenterOnline.value = s.presenter_online;
    presenterSeenMsAgo.value = s.presenter_seen_ms_ago ?? null;
    relayAccessUrl.value = s.relay_access_url ?? null;

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
    await Promise.allSettled([
      api.getStatus().then(status => {
        applyStatus(status);
        net.online = true;
      }).catch(e => {
        net.online = false;
        console.error("status refresh failed", e);
      }),
      api.getLogs().then(entries => { logs.value = entries; })
        .catch(e => console.error("log refresh failed", e)),
      api.getRelayStatus().then(status => { relayStatus.value = status; })
        .catch(e => console.error("relay refresh failed", e)),
    ]);
  }

  /** 从后端加载 relay 配置 + 状态（Settings 模态打开时调用）。 */
  async function loadRelay() {
    relayConfig.value = await api.getRelayConfig();
    relayStatus.value = await api.getRelayStatus();
  }

  /** 保存 relay 配置（null = 清除）并更新本地缓存。 */
  async function saveRelay(cfg: RelayConfigDto | null) {
    await api.setRelayConfig(cfg);
    relayConfig.value = cfg;
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
  async function regenerateToken() {
    await api.regenerateToken();
    await refresh();
  }
  async function setServerPort(port: number) {
    await api.setServerPort(port);
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
    presenterSeenMsAgo,
    releaseController,
    regenerateToken,
    setServerPort,
    relayConfig,
    relayStatus,
    relayAccessUrl,
    loadRelay,
    saveRelay,
  };
});
