mod config;
mod director_ws;
mod session;
// phone_routes will be added in Tasks 3-5; placeholder:
// mod phone_routes;

use std::sync::Arc;

pub struct AppCtx {
    pub sessions: session::SessionTable,
    pub config: config::Config,
}

pub fn build_router(ctx: Arc<AppCtx>) -> axum::Router {
    axum::Router::new()
        .merge(director_ws::router())
        // .merge(phone_routes::router())  // Task 3-5
        .with_state(ctx)
}

#[tokio::main]
async fn main() {
    let cfg_path = std::env::var("STAGEDECK_RELAY_CONFIG")
        .unwrap_or_else(|_| "/etc/stagedeck-relay/config.toml".into());
    let cfg = config::Config::load(std::path::Path::new(&cfg_path));
    let ctx = Arc::new(AppCtx {
        sessions: session::SessionTable::new(),
        config: cfg.clone(),
    });
    eprintln!("[stagedeck-relay] listening on {}", cfg.listen);
    let app = build_router(ctx);
    let listener = tokio::net::TcpListener::bind(cfg.listen).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
