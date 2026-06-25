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
        }
    }
}
