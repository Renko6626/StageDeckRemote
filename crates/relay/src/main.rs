use relay::session::SessionTable;
use relay::{build_router, config, AppCtx};
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    init_tracing();

    let cfg_path = std::env::var("STAGEDECK_RELAY_CONFIG")
        .unwrap_or_else(|_| "/etc/stagedeck-relay/config.toml".into());
    let cfg = config::Config::load(std::path::Path::new(&cfg_path));
    let ctx = Arc::new(AppCtx {
        sessions: SessionTable::new(),
        config: cfg.clone(),
        started_at: std::time::Instant::now(),
    });

    tracing::info!(
        listen = %cfg.listen,
        admin_status = cfg.admin_token.is_some(),
        tunnel_timeout_secs = cfg.tunnel_timeout_secs,
        session_grace_secs = cfg.session_grace_secs,
        config_path = %cfg_path,
        "relay starting"
    );

    let app = build_router(ctx);
    let listener = tokio::net::TcpListener::bind(cfg.listen).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// 默认走 compact 文本(人眼读);设 STAGEDECK_RELAY_LOG=json → JSON 行(给日志聚合)。
// RUST_LOG 控制级别(默认 info)。
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let json_mode = std::env::var("STAGEDECK_RELAY_LOG")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    let registry = tracing_subscriber::registry().with(filter);
    if json_mode {
        registry.with(fmt::layer().json().with_current_span(false)).init();
    } else {
        registry.with(fmt::layer().compact()).init();
    }
}
