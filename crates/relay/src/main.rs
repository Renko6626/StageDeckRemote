use relay::{build_router, config, AppCtx};
use relay::session::SessionTable;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let cfg_path = std::env::var("STAGEDECK_RELAY_CONFIG")
        .unwrap_or_else(|_| "/etc/stagedeck-relay/config.toml".into());
    let cfg = config::Config::load(std::path::Path::new(&cfg_path));
    let ctx = Arc::new(AppCtx {
        sessions: SessionTable::new(),
        config: cfg.clone(),
        started_at: std::time::Instant::now(),
    });
    eprintln!(
        "[stagedeck-relay] listening on {} (admin status: {})",
        cfg.listen,
        if cfg.admin_token.is_some() {
            "enabled at /admin/status"
        } else {
            "disabled"
        }
    );
    let app = build_router(ctx);
    let listener = tokio::net::TcpListener::bind(cfg.listen).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
