pub mod admin;
pub mod config;
pub mod director_ws;
pub mod phone_routes;
pub mod session;

use std::sync::Arc;

pub struct AppCtx {
    pub sessions: session::SessionTable,
    pub config: config::Config,
    /// 进程启动时刻;`/admin/status` 用它算 uptime。
    pub started_at: std::time::Instant,
}

pub fn build_router(ctx: Arc<AppCtx>) -> axum::Router {
    axum::Router::new()
        .merge(director_ws::router())
        .merge(phone_routes::router())
        .merge(admin::router())
        .with_state(ctx)
}
