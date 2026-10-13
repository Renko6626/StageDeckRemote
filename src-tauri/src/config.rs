//! 配置持久化（spec §11.11）。

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server_port: u16,
    #[serde(default)]
    pub relay: Option<RelayConfigPersist>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_port: 8787,
            relay: None,
        }
    }
}

/// Relay 配置的持久化形式（config.json 中的 "relay" 字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfigPersist {
    pub base_url: String,
    pub session_id: String,
    pub secret: String,
    pub enabled: bool,
}

impl RelayConfigPersist {
    pub fn validated(mut self) -> Result<Self, String> {
        let url = tauri::Url::parse(self.base_url.trim())
            .map_err(|_| "Relay 地址无效，请填写 ws://主机:端口".to_string())?;
        // 当前 WS 客户端未启用 TLS；不要让不支持的地址保存后无限重连。
        if url.scheme() != "ws" || url.host_str().is_none() {
            return Err("当前 Relay 支持 ws://主机:端口地址".into());
        }
        if url.path() != "/" || url.query().is_some() || url.fragment().is_some()
            || !url.username().is_empty() || url.password().is_some()
        {
            return Err("Relay 地址只填写主机和端口，不要包含路径、查询参数或账号".into());
        }
        self.base_url = url.as_str().trim_end_matches('/').to_string();
        self.session_id = self.session_id.trim().to_string();
        if !(4..=16).contains(&self.session_id.len())
            || !self.session_id.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err("会话 ID 需为 4–16 位大写字母、数字或连字符".into());
        }
        if self.secret.trim().is_empty() {
            return Err("请填写 Relay 密钥".into());
        }
        Ok(self)
    }
}

impl Config {
    /// 修改磁盘待生效 Relay 设置，保留端口等其他配置。
    pub fn save_relay(path: &Path, relay: Option<RelayConfigPersist>) -> Result<(), String> {
        let relay = relay.map(RelayConfigPersist::validated).transpose()?;
        let mut config = Self::load_for_edit(path).map_err(|e| format!("无法读取原配置：{e}"))?;
        config.relay = relay;
        config.save(path).map_err(|e| format!("Relay 配置保存失败：{e}"))
    }

    /// 从磁盘读取配置；文件不存在或解析失败时返回默认值。
    pub fn load(path: &Path) -> Self {
        Self::load_for_edit(path).unwrap_or_default()
    }

    /// 编辑时只把文件不存在视为初始配置；禁止把损坏/不可读文件当默认值覆盖。
    pub fn load_for_edit(path: &Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// 写回磁盘（自动创建父目录）。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        let pending = path.with_extension(format!("{}.tmp", rand::random::<u64>()));
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&pending)?;
        use std::io::Write;
        let result = file.write_all(&json).and_then(|_| file.sync_all());
        drop(file);
        let result = result.and_then(|_| std::fs::rename(&pending, path));
        if result.is_err() { let _ = std::fs::remove_file(&pending); }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relay(base: &str) -> RelayConfigPersist {
        RelayConfigPersist { base_url: base.into(), session_id: "SHOW-2026".into(), secret: "secret".into(), enabled: true }
    }

    #[test]
    fn relay_address_is_normalized_and_invalid_settings_are_rejected() {
        assert_eq!(relay(" ws://localhost:8080/ ").validated().unwrap().base_url, "ws://localhost:8080");
        for url in ["http://localhost:8080", "wss://localhost:8080", "ws://localhost:8080/path", "ws://localhost:8080/?x=1"] {
            assert!(relay(url).validated().is_err(), "{url}");
        }
        let mut cfg = relay("ws://localhost:8080");
        cfg.secret.clear();
        assert!(cfg.validated().is_err());
    }

    #[test]
    fn relay_save_preserves_port_and_rejects_invalid_settings_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("stagedeck-config-{}", rand::random::<u64>()));
        let path = dir.join("config.json");
        let runtime = Config { server_port: 4321, relay: Some(relay("ws://old:8080")) };
        runtime.save(&path).unwrap();
        Config::save_relay(&path, Some(relay("ws://new:8080/"))).unwrap();
        let disk = Config::load(&path);
        assert_eq!(disk.server_port, 4321);
        assert_eq!(disk.relay.unwrap().base_url, "ws://new:8080");
        assert!(Config::save_relay(&path, Some(relay("http://invalid"))).is_err());
        assert_eq!(Config::load(&path).relay.unwrap().base_url, "ws://new:8080");
        // 无法替换目录时应返回错误，并清理未完成文件。
        assert!(runtime.save(&dir).is_err());
        assert_eq!(Config::load(&path).relay.unwrap().base_url, "ws://new:8080");
        Config::save_relay(&path, None).unwrap();
        assert!(Config::load(&path).relay.is_none());
        assert_eq!(Config::load(&path).server_port, 4321);
        let damaged = b"{broken configuration";
        std::fs::write(&path, damaged).unwrap();
        assert!(Config::load_for_edit(&path).is_err());
        assert!(Config::save_relay(&path, Some(relay("ws://new:8080"))).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), damaged);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
