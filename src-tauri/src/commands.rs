//! 导播端 Tauri IPC 命令（spec §6.1）。
//!
//! 这些是**本机特权**操作，只能被 Tauri 窗口里的导播 GUI 调用，
//! 不经过网络、也不暴露在局域网 API 上 —— 这是导播 vs 主持人的安全边界。

use std::sync::Arc;

use serde::Serialize;
use tauri::{Manager, State};

use crate::app_state::{AppState, TargetWindow};
use crate::log::LogEntry;
use crate::{input, log, network, obs, window_manager};

/// 导播端状态快照（喂给前端展示）。
#[derive(Serialize)]
pub struct DirectorStatus {
    pub remote_enabled: bool,
    pub control_locked: bool,
    pub presenter_unlocked: bool,
    pub selected_window: Option<TargetWindow>,
    pub active_client_id: Option<String>,
    pub token: String,
    pub server_port: u16,
    pub local_ip: Option<String>,
    pub access_url: Option<String>,
}

#[tauri::command]
pub fn get_status(state: State<Arc<AppState>>) -> DirectorStatus {
    let i = state.inner.lock().expect("state lock poisoned");
    let local_ip = network::local_ipv4();
    let access_url = local_ip
        .as_ref()
        .map(|ip| network::access_url(ip, i.server_port, &i.token));
    DirectorStatus {
        remote_enabled: i.remote_enabled,
        control_locked: i.control_locked,
        presenter_unlocked: i.presenter_unlocked,
        selected_window: i.selected_window.clone(),
        active_client_id: i.active_client_id.clone(),
        token: i.token.clone(),
        server_port: i.server_port,
        local_ip,
        access_url,
    }
}

#[tauri::command]
pub fn list_windows() -> Vec<TargetWindow> {
    window_manager::list_windows()
}

#[tauri::command]
pub fn auto_detect_ppt(state: State<Arc<AppState>>) -> Option<TargetWindow> {
    let found = window_manager::find_powerpoint();
    if let Some(win) = found.clone() {
        let handle = win.handle;
        if let Ok(mut i) = state.inner.lock() {
            i.selected_window = Some(win);
        }
        state.capture.start(handle);
        log::event(&state, "自动识别到 PowerPoint 放映窗口");
    }
    found
}

#[tauri::command]
pub fn select_window(state: State<Arc<AppState>>, window: TargetWindow) {
    let title = window.title.clone();
    let handle = window.handle;
    if let Ok(mut i) = state.inner.lock() {
        i.selected_window = Some(window);
    }
    state.capture.start(handle);
    log::event(&state, format!("已选择窗口：{title}"));
}

#[tauri::command]
pub fn set_remote_enabled(state: State<Arc<AppState>>, enabled: bool) {
    if let Ok(mut i) = state.inner.lock() {
        i.remote_enabled = enabled;
        if !enabled {
            // 关闭远程时一并清掉主持人解锁，确保彻底接管。
            i.presenter_unlocked = false;
        }
    }
    log::event(&state, format!("远程控制：{}", if enabled { "开启" } else { "关闭" }));
}

#[tauri::command]
pub fn force_lock(state: State<Arc<AppState>>, locked: bool) {
    if let Ok(mut i) = state.inner.lock() {
        i.control_locked = locked;
        if locked {
            i.presenter_unlocked = false;
        }
    }
    log::event(&state, format!("强制锁定：{}", if locked { "锁定" } else { "释放" }));
}

#[tauri::command]
pub fn local_next(state: State<Arc<AppState>>) -> Result<(), String> {
    activate_selected(&state);
    input::key_next().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn local_prev(state: State<Arc<AppState>>) -> Result<(), String> {
    activate_selected(&state);
    input::key_prev().map_err(|e| e.to_string())
}

/// 翻页前激活选中窗口（本地兜底翻页同样需要焦点）。
fn activate_selected(state: &AppState) {
    if let Ok(i) = state.inner.lock() {
        if let Some(w) = i.selected_window.as_ref() {
            window_manager::bring_to_front(w.handle);
        }
    }
}

#[tauri::command]
pub fn local_black() -> Result<(), String> {
    obs::black().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn local_restore() -> Result<(), String> {
    obs::restore().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_logs(state: State<Arc<AppState>>) -> Vec<LogEntry> {
    let app: &AppState = &state;
    log::recent(app, 100)
}

/// 列出当前演示文稿的 VBA 宏名（导播专用，VBA spike）。
#[tauri::command]
pub fn vba_list_macros() -> Result<Vec<String>, String> {
    crate::ppt_com::list_macros()
}

/// 触发一个 VBA 宏（导播专用，VBA spike）。
#[tauri::command]
pub fn vba_run_macro(name: String) -> Result<(), String> {
    crate::ppt_com::run_macro(&name)
}

/// 导播 GUI：读当前页状态（COM）。
#[tauri::command]
pub fn get_slide_state() -> crate::ppt_com::SlideState {
    crate::ppt_com::slide_state().unwrap_or_else(|_| crate::ppt_com::SlideState::unavailable())
}

/// 导播 GUI:读取预制快捷消息列表。
#[tauri::command]
pub fn get_presets(state: State<Arc<AppState>>) -> Vec<crate::app_state::PresetMessage> {
    state.inner.lock().map(|i| i.presets.clone()).unwrap_or_default()
}

/// 导播 GUI:用一组文本替换预制消息(id 服务端派生),并持久化。
#[tauri::command]
pub fn set_presets(app: tauri::AppHandle, state: State<Arc<AppState>>, texts: Vec<String>) {
    let presets = crate::presets::from_texts(&texts);
    if let Ok(mut i) = state.inner.lock() {
        i.presets = presets.clone();
    }
    if let Ok(dir) = app.path().app_config_dir() {
        if let Err(e) = crate::presets::save(&dir.join("presets.json"), &presets) {
            log::event(&state, format!("快捷消息持久化失败（重启后会丢失）：{e}"));
        }
    }
    log::event(&state, "已更新快捷消息预制");
}
