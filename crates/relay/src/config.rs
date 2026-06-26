use serde::Deserialize;
use std::net::SocketAddr;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub listen: SocketAddr,
    #[serde(default = "default_grace_secs")]
    pub session_grace_secs: u64,
    #[serde(default = "default_tunnel_timeout_secs")]
    pub tunnel_timeout_secs: u64,
    /// 只读 admin 状态页的访问 token。`None` = 状态页禁用(404)。
    /// 与 session token 互不相关;只用于 `GET /admin/status?token=<admin_token>`。
    #[serde(default)]
    pub admin_token: Option<String>,
}

fn default_grace_secs() -> u64 { 60 }
fn default_tunnel_timeout_secs() -> u64 { 30 }

impl Config {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_else(Self::default)
    }
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:8080".parse().unwrap(),
            session_grace_secs: 60,
            tunnel_timeout_secs: 30,
            admin_token: None,
        }
    }
}
