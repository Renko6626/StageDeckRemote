//! StageDeck Remote —— 导播端应用入口（spec §11.1）。
//!
//! 启动流程：生成 token → 初始化全局状态 → 在 Tauri 的 async 运行时中
//! spawn Axum 本地服务 → 注册导播端 IPC 命令 → 运行 Tauri 窗口。

mod app_state;
mod auth;
mod capture;
#[cfg(windows)]
mod capture_session;
mod commands;
mod config;
mod input;
mod log;
mod network;
mod obs;
mod ppt_com;
mod routes;
mod web_server;
mod window_manager;

use std::sync::Arc;

use app_state::AppState;

const DEFAULT_PORT: u16 = 8787;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Windows：进程设为 Per-Monitor-DPI-Aware v2，使窗口矩形/屏幕坐标/SendInput 统一物理像素。
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let token = auth::generate_token();
    let state = Arc::new(AppState::new(token, DEFAULT_PORT));
    let server_state = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |_app| {
            // 在后台启动局域网 Web 服务（主持人端入口）。
            let st = server_state.clone();
            tauri::async_runtime::spawn(async move {
                web_server::run(st).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::list_windows,
            commands::auto_detect_ppt,
            commands::select_window,
            commands::set_remote_enabled,
            commands::force_lock,
            commands::local_next,
            commands::local_prev,
            commands::local_black,
            commands::local_restore,
            commands::get_logs,
            commands::vba_list_macros,
            commands::vba_run_macro,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
