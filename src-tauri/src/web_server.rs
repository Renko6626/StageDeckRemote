//! 本地 Web 服务（spec §11.3）。在 `0.0.0.0:<port>` 上提供主持人端页面与 API。

use std::sync::Arc;

use crate::app_state::AppState;
use crate::{log, routes};

/// 启动 Axum 服务并阻塞运行（由 lib.rs 在 Tauri 的 async 运行时中 spawn）。
/// 传入预先构建的 router 实例，与 relay_client 共享同一路由逻辑。
pub async fn run_with_router(state: Arc<AppState>, router: Arc<axum::Router>) {
    let port = state.lock().server_port;
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            log::event(&state, format!("无法绑定 {addr}：{e}"));
            return;
        }
    };
    log::event(&state, format!("本地服务已启动：http://{addr}"));
    let app = (*router).clone(); // axum::Router 是 Clone
    if let Err(e) = axum::serve(listener, app).await {
        log::event(&state, format!("服务异常退出：{e}"));
    }
}

/// 向后兼容包装：内部构建 router 后调用 run_with_router。
pub async fn run(state: Arc<AppState>) {
    let router = Arc::new(routes::router(state.clone()));
    run_with_router(state, router).await;
}
