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
mod frame_broker;
mod input;
mod log;
#[cfg(any(windows, test))]
mod macro_execution;
mod network;
mod op_cache;
mod ppt_com;
mod presets;
mod relay_client;
mod routes;
mod web_server;
mod window_manager;

use std::sync::Arc;

use app_state::AppState;
use tauri::{Emitter, Manager};

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
    let (message_tx, mut message_rx) =
        tokio::sync::mpsc::channel::<app_state::HostMessage>(64);
    let state = Arc::new(AppState::new(token, DEFAULT_PORT, message_tx));
    let server_state = state.clone();
    let preset_state = state.clone();
    let watch_state = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            // 先加载持久化配置(端口 + relay)+ 预制消息,再启动 Web 服务(使其绑定配置端口)。
            if let Ok(dir) = app.path().app_config_dir() {
                let cfg = config::Config::load(&dir.join("config.json"));
                let presets_loaded = presets::load(&dir.join("presets.json"));
                {
                    let mut i = preset_state.lock();
                    i.server_port = cfg.server_port;
                    i.presets = presets_loaded;
                }
                // 若持久化配置中 relay.enabled == true，写入 AppState.relay_config。
                if let Some(r) = cfg.relay.filter(|r| r.enabled) {
                    *preset_state.relay_config.lock().unwrap() =
                        Some(relay_client::RelayConfig {
                            base_url: r.base_url,
                            session_id: r.session_id,
                            secret: r.secret,
                        });
                }
            }

            // 构建一次 router,供 LAN 监听与 relay 客户端共享。
            let router = std::sync::Arc::new(crate::routes::router(server_state.clone()));

            // 后台启动局域网 Web 服务(主持人端入口)。
            let st_lan = server_state.clone();
            let rt_lan = router.clone();
            tauri::async_runtime::spawn(async move {
                web_server::run_with_router(st_lan, rt_lan).await;
            });

            // 若已配置 relay,启动 relay 客户端。
            if let Some(cfg) = server_state.relay_config.lock().unwrap().clone() {
                let _ = crate::relay_client::spawn(server_state.clone(), router.clone(), cfg);
            }

            // 接收主持人消息 → 顶部居中软弹窗 + 任务栏闪(均不抢焦点)。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(msg) = message_rx.recv().await {
                    if let Some(toast) = handle.get_webview_window("toast") {
                        let wsize = toast
                            .outer_size()
                            .unwrap_or(tauri::PhysicalSize { width: 380, height: 120 });
                        if let Ok(Some(mon)) = toast.primary_monitor() {
                            let msize = mon.size();
                            let x = ((msize.width as i32) - (wsize.width as i32)) / 2;
                            let _ = toast.set_position(tauri::PhysicalPosition { x, y: 40 });
                        }
                        let _ = toast.show();
                        let _ = handle.emit_to("toast", "host-message", msg.clone());
                    }
                    if let Some(main) = handle.get_webview_window("main") {
                        let _ = main
                            .request_user_attention(Some(tauri::UserAttentionType::Informational));
                    }
                }
            });

            // 控制端断线 watcher:在线→离线跳变 → 释放控制槽 + 日志 + 置顶 toast。
            let ws = watch_state.clone();
            tauri::async_runtime::spawn(async move {
                let mut was_online = false;
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
                loop {
                    interval.tick().await;
                    let now = app_state::now_ms();
                    let (online, still_assigned) = {
                        let mut i = ws.lock();
                        let on = app_state::controller_online(&i, now);
                        let assigned = i.active_client_id.is_some();
                        if was_online && !on {
                            i.active_client_id = None;
                        }
                        (on, assigned)
                    };
                    // 仅"还分配着但心跳过期"(真断线)才报警;导播主动释放(已置 None)不算断线。
                    if was_online && !online && still_assigned {
                        crate::log::event(&ws, "主持人已断开连接");
                        let _ = ws.message_tx.try_send(app_state::HostMessage {
                            from: "系统".to_string(),
                            text: "主持人已断开连接".to_string(),
                            at_ms: now,
                        });
                    }
                    was_online = online;
                }
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
            commands::get_logs,
            commands::vba_list_macros,
            commands::vba_run_macro,
            commands::get_slide_state,
            commands::get_presets,
            commands::set_presets,
            commands::send_to_presenter,
            commands::get_snapshot,
            commands::subscribe_preview,
            commands::timer_start,
            commands::timer_toggle,
            commands::timer_reset,
            commands::release_controller,
            commands::regenerate_token,
            commands::set_server_port,
            commands::get_relay_config,
            commands::set_relay_config,
            commands::get_relay_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
