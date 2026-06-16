// 导播端 IPC 客户端：封装对 Rust Tauri 命令的调用（commands.rs）。
// 注意：这些命令只在 Tauri 窗口内可用；主持人端走的是 HTTP API（见后端 routes.rs）。
import { invoke } from "@tauri-apps/api/core";

export interface TargetWindow {
  title: string;
  left: number;
  top: number;
  width: number;
  height: number;
  handle: number;
}

export interface DirectorStatus {
  remote_enabled: boolean;
  control_locked: boolean;
  presenter_unlocked: boolean;
  selected_window: TargetWindow | null;
  active_client_id: string | null;
  token: string;
  server_port: number;
  local_ip: string | null;
  access_url: string | null;
}

export interface LogEntry {
  time_ms: number;
  message: string;
}

export interface SlideAction {
  id: string;
  label: string;
  macro_name: string;
}

export interface SlideState {
  available: boolean;
  in_show: boolean;
  current: number;
  total: number;
  actions: SlideAction[];
}

export interface PresetMessage {
  id: string;
  text: string;
}

export const api = {
  getStatus: () => invoke<DirectorStatus>("get_status"),
  listWindows: () => invoke<TargetWindow[]>("list_windows"),
  autoDetectPpt: () => invoke<TargetWindow | null>("auto_detect_ppt"),
  selectWindow: (window: TargetWindow) => invoke<void>("select_window", { window }),
  setRemoteEnabled: (enabled: boolean) => invoke<void>("set_remote_enabled", { enabled }),
  forceLock: (locked: boolean) => invoke<void>("force_lock", { locked }),
  localNext: () => invoke<void>("local_next"),
  localPrev: () => invoke<void>("local_prev"),
  localBlack: () => invoke<void>("local_black"),
  localRestore: () => invoke<void>("local_restore"),
  getLogs: () => invoke<LogEntry[]>("get_logs"),
  getSlideState: () => invoke<SlideState>("get_slide_state"),
  getPresets: () => invoke<PresetMessage[]>("get_presets"),
  setPresets: (texts: string[]) => invoke<void>("set_presets", { texts }),
  sendToPresenter: (id: string) => invoke<void>("send_to_presenter", { id }),
  getSnapshot: () => invoke<string>("get_snapshot"),
};
