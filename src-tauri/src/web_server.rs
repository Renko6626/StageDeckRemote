//! 本地 Web 服务（spec §11.3）。在 `0.0.0.0:<port>` 上提供主持人端页面与 API。

use std::sync::Arc;

use crate::app_state::AppState;
use crate::{log, routes};

/// 启动 Axum 服务并阻塞运行（由 lib.rs 在 Tauri 的 async 运行时中 spawn）。
pub async fn run(state: Arc<AppState>) {
    let port = state.lock().server_port;

    let app = routes::router(state.clone());
    let addr = format!("0.0.0.0:{port}");

    match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            log::event(&state, format!("本地服务已启动：http://{addr}"));
            if let Err(e) = axum::serve(listener, app).await {
                log::event(&state, format!("服务异常退出：{e}"));
            }
        }
        Err(e) => {
            log::event(&state, format!("无法绑定 {addr}：{e}"));
        }
    }
}
