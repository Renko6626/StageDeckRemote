//! 配置持久化（spec §11.11）。

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server_port: u16,
    pub last_selected_window_title: String,
    pub obs_enabled: bool,
    pub obs_url: String,
    pub obs_password: String,
    pub ppt_scene_name: String,
    pub black_scene_name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_port: 8787,
            last_selected_window_title: String::new(),
            obs_enabled: false,
            obs_url: "ws://127.0.0.1:4455".into(),
            obs_password: String::new(),
            ppt_scene_name: "PPT".into(),
            black_scene_name: "Black".into(),
        }
    }
}

impl Config {
    /// 从磁盘读取配置；文件不存在或解析失败时返回默认值。
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// 写回磁盘（自动创建父目录）。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .unwrap_or_else(|_| "{}".to_string());
        std::fs::write(path, json)
    }
}
