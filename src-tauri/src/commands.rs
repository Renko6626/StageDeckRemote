//! 导播端 Tauri IPC 命令（spec §6.1）。
//!
//! 这些是**本机特权**操作，只能被 Tauri 窗口里的导播 GUI 调用，
//! 不经过网络、也不暴露在局域网 API 上 —— 这是导播 vs 主持人的安全边界。

use std::sync::Arc;

use serde::Serialize;
use tauri::{Manager, State};
use base64::Engine;

use crate::app_state::{AppState, TargetWindow};
use crate::log::LogEntry;
use crate::{input, log, network, window_manager};
use protocol;

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
    pub timer: Option<crate::app_state::TimerView>,
    pub presenter_online: bool,
    /// Relay 访问 URL（HTTP 形式），仅 relay 已连接时为 Some(_)。
    pub relay_access_url: Option<String>,
}

/// Relay 配置 DTO（IPC 传输用）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RelayConfigDto {
    pub base_url: String,
    pub session_id: String,
    pub secret: String,
}

#[tauri::command]
pub fn get_status(state: State<Arc<AppState>>) -> DirectorStatus {
    let i = state.lock();
    let local_ip = network::local_ipv4();
    let access_url = local_ip
        .as_ref()
        .map(|ip| network::access_url(ip, i.server_port, &i.token));
    // 仅当 relay 已连接时构建可供浏览器使用的 HTTP 访问地址（ws:// → http://）。
    let relay_access_url = state.relay_config.lock().unwrap().as_ref().and_then(|c| {
        matches!(&*state.relay_status.lock().unwrap(), crate::relay_client::RelayStatus::Connected)
            .then(|| format!("{}/r/{}/presenter?token={}",
                c.base_url.replace("ws://", "http://").replace("wss://", "https://"),
                c.session_id, i.token))
    });
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
        timer: i
            .timer
            .as_ref()
            .map(|t| crate::app_state::timer_view_of(t, crate::app_state::now_ms())),
        presenter_online: crate::app_state::controller_online(&i, crate::app_state::now_ms()),
        relay_access_url,
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
        {
            let mut i = state.lock();
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
    {
        let mut i = state.lock();
        i.selected_window = Some(window);
    }
    state.capture.start(handle);
    log::event(&state, format!("已选择窗口：{title}"));
}

#[tauri::command]
pub fn set_remote_enabled(state: State<Arc<AppState>>, enabled: bool) {
    {
        let mut i = state.lock();
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
    {
        let mut i = state.lock();
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
    let i = state.lock();
    if let Some(w) = i.selected_window.as_ref() {
        window_manager::bring_to_front(w.handle);
    }
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
    let i = state.lock();
    i.presets.clone()
}

/// 导播 GUI:用一组文本替换预制消息(id 服务端派生),并持久化。
#[tauri::command]
pub fn set_presets(app: tauri::AppHandle, state: State<Arc<AppState>>, texts: Vec<String>) {
    let presets = crate::presets::from_texts(&texts);
    {
        let mut i = state.lock();
        i.presets = presets.clone();
    }
    if let Ok(dir) = app.path().app_config_dir() {
        if let Err(e) = crate::presets::save(&dir.join("presets.json"), &presets) {
            log::event(&state, format!("快捷消息持久化失败（重启后会丢失）：{e}"));
        }
    }
    log::event(&state, "已更新快捷消息预制");
}

/// 导播 GUI:从共享预制池里选一条(按 id)发给主持人(主持人页内横幅显示)。
#[tauri::command]
pub fn send_to_presenter(state: State<Arc<AppState>>, id: String) {
    let sent = {
        let mut i = state.lock();
        let Some(text) = i.presets.iter().find(|p| p.id == id).map(|p| p.text.clone()) else {
            return; // id 不在池中:不改动(前端按池内 id 调用,不会非法)
        };
        let seq = i.director_message.as_ref().map(|m| m.seq).unwrap_or(0) + 1;
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        i.director_message = Some(crate::app_state::DirectorMessage {
            seq,
            text: text.clone(),
            at_ms,
        });
        text
    };
    log::event(&state, format!("已通知主持人：{sent}"));
}

/// 导播 GUI:进程内取一帧捕获,返回 base64 JPEG(避免 webview 跨源拉环回 HTTP)。
#[tauri::command]
pub fn get_snapshot(state: State<Arc<AppState>>) -> Result<String, String> {
    let bytes = crate::capture::snapshot_jpeg(&state, 1280, 720, 70).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 导播 GUI:订阅实时预览帧。Rust 侧订阅 broker,每来新帧把原始 JPEG 字节推入 channel。
/// 前端 onmessage 收到 ArrayBuffer → createImageBitmap → 画 canvas。
/// 注意:Tauri Channel 在窗口/页面销毁后 send **不会**报错(仅整应用退出时才报错),
/// 故无法靠 send 失败收尾;改为「每次重新订阅就 abort 上一个推送任务」(AppState.preview_task)
/// 保证至多一个任务、不随组件重挂/页面重载堆积。
#[tauri::command]
pub fn subscribe_preview(
    state: State<Arc<AppState>>,
    on_frame: tauri::ipc::Channel<tauri::ipc::Response>,
) {
    let mut rx = state.capture.subscribe();
    // 先推一帧当前最新(若有),让导播一连上就看到当前画面。
    {
        let cur = state.capture.latest_encoded();
        if !cur.jpeg.is_empty() {
            let _ = on_frame.send(tauri::ipc::Response::new(cur.jpeg.clone()));
        }
    }
    let handle = tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let bytes = { rx.borrow().jpeg.clone() };
            if bytes.is_empty() {
                continue;
            }
            // 整应用退出时 send 失败 → 退出;窗口重挂的收尾靠下面 abort 旧任务。
            if on_frame.send(tauri::ipc::Response::new(bytes)).is_err() {
                break;
            }
        }
    });
    // 替换上一个推送任务:组件重挂/页面重载会再次调用本命令,abort 旧的,避免任务 + watch 接收端堆积。
    if let Ok(mut slot) = state.preview_task.lock() {
        if let Some(old) = slot.replace(handle) {
            old.abort();
        }
    }
}

/// 导播 GUI:启动计时(从满开始倒计时,ms 毫秒)。
#[tauri::command]
pub fn timer_start(state: State<Arc<AppState>>, ms: u64) {
    {
        let mut i = state.lock();
        i.timer = Some(crate::app_state::TimerState {
            duration_ms: ms,
            running: true,
            started_at_ms: crate::app_state::now_ms(),
            elapsed_before_ms: 0,
        });
    }
    log::event(&state, "计时:开始");
}

/// 导播 GUI:暂停 / 继续。
#[tauri::command]
pub fn timer_toggle(state: State<Arc<AppState>>) {
    let msg = {
        let mut i = state.lock();
        let Some(t) = i.timer.as_mut() else {
            return;
        };
        let now = crate::app_state::now_ms();
        if t.running {
            t.elapsed_before_ms = t.elapsed_before_ms.saturating_add(now.saturating_sub(t.started_at_ms));
            t.running = false;
            "计时:暂停"
        } else {
            t.started_at_ms = now;
            t.running = true;
            "计时:继续"
        }
    };
    log::event(&state, msg);
}

/// 导播 GUI:重置(清空)计时。
#[tauri::command]
pub fn timer_reset(state: State<Arc<AppState>>) {
    {
        let mut i = state.lock();
        i.timer = None;
    }
    log::event(&state, "计时:重置");
}

/// 导播 GUI:释放控制权(清空控制端,下一台轮询即接管)。
#[tauri::command]
pub fn release_controller(state: State<Arc<AppState>>) {
    {
        let mut i = state.lock();
        i.active_client_id = None;
    }
    log::event(&state, "导播:释放控制权");
}

/// 导播 GUI:重置访问 token(踢掉所有已连主持人,二维码刷新)。
#[tauri::command]
pub fn regenerate_token(state: State<Arc<AppState>>) {
    let new = crate::auth::generate_token();
    {
        let mut i = state.lock();
        i.token = new;
        i.active_client_id = None; // 都要重连,顺带释放控制端
    }
    log::event(&state, "已重置访问码(所有主持人需重新扫码)");
    // 若 relay 客户端在跑,把新 token 推给 relay。
    if let Some(tx) = state.relay_notify.lock().unwrap().clone() {
        let new_token = state.lock().token.clone();
        let _ = tx.send(protocol::ControlMessage::TokenUpdate { token: new_token });
    }
}

/// 导播 GUI:设置服务端口并持久化(下次启动生效;不改运行中的端口)。
#[tauri::command]
pub fn set_server_port(app: tauri::AppHandle, state: State<Arc<AppState>>, port: u16) {
    let saved = if let Ok(dir) = app.path().app_config_dir() {
        let path = dir.join("config.json");
        let mut cfg = crate::config::Config::load(&path);
        cfg.server_port = port;
        cfg.save(&path).is_ok()
    } else {
        false
    };
    if saved {
        log::event(&state, format!("服务端口已保存为 {port}(重启生效)"));
    } else {
        log::event(&state, format!("服务端口保存失败(端口 {port})"));
    }
}

/// 导播 GUI:读取当前 relay 配置（无配置则返回 null）。
#[tauri::command]
pub fn get_relay_config(state: State<Arc<AppState>>) -> Option<RelayConfigDto> {
    state.relay_config.lock().unwrap().as_ref().map(|c| RelayConfigDto {
        base_url: c.base_url.clone(),
        session_id: c.session_id.clone(),
        secret: c.secret.clone(),
    })
}

/// 导播 GUI:保存 relay 配置（null = 清除 relay）。改完需重启导播生效。
#[tauri::command]
pub fn set_relay_config(
    app: tauri::AppHandle,
    state: State<Arc<AppState>>,
    cfg: Option<RelayConfigDto>,
) {
    // 写入 AppState（内存）。
    *state.relay_config.lock().unwrap() = cfg.as_ref().map(|c| crate::relay_client::RelayConfig {
        base_url: c.base_url.clone(),
        session_id: c.session_id.clone(),
        secret: c.secret.clone(),
    });
    // 持久化到 config.json。
    if let Ok(dir) = app.path().app_config_dir() {
        let path = dir.join("config.json");
        let mut conf = crate::config::Config::load(&path);
        conf.relay = cfg.map(|c| crate::config::RelayConfigPersist {
            base_url: c.base_url,
            session_id: c.session_id,
            secret: c.secret,
            enabled: true,
        });
        let _ = conf.save(&path);
    }
    // 注意：本 PR 不做 relay 客户端热重启；UI 文案已标注"重启生效"。
    log::event(&state, "Relay 配置已更新（重启生效）");
}

/// 导播 GUI:返回当前 relay 连接状态字符串。
#[tauri::command]
pub fn get_relay_status(state: State<Arc<AppState>>) -> String {
    match &*state.relay_status.lock().unwrap() {
        crate::relay_client::RelayStatus::Disabled => "disabled".into(),
        crate::relay_client::RelayStatus::Connecting => "connecting".into(),
        crate::relay_client::RelayStatus::Connected => "connected".into(),
        crate::relay_client::RelayStatus::Error(s) => format!("error: {s}"),
    }
}
