//! 预制快捷消息的默认种子与持久化(spec §2/§5)。

use std::path::Path;

use crate::app_state::PresetMessage;

/// 从文本派生稳定 id(FNV-1a 32 位 → 8 位十六进制)。纯函数,便于单测。
pub fn preset_id(text: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for b in text.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

/// 把一组文本构造为预制消息(去首尾空白、跳过空串;id 由文本派生)。
pub fn from_texts(texts: &[String]) -> Vec<PresetMessage> {
    texts
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| PresetMessage { id: preset_id(t), text: t.to_string() })
        .collect()
}

/// 首次默认种子。
pub fn default_presets() -> Vec<PresetMessage> {
    from_texts(&[
        "请加快节奏".into(),
        "请放慢".into(),
        "时间到".into(),
        "下一位准备".into(),
        "麦克风没声音".into(),
        "收到 / OK".into(),
    ])
}

/// 从磁盘读取;不存在或解析失败/为空则返回默认种子。
pub fn load(path: &Path) -> Vec<PresetMessage> {
    let parsed: Option<Vec<PresetMessage>> = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    match parsed {
        Some(v) if !v.is_empty() => v,
        _ => default_presets(),
    }
}

/// 写回磁盘(自动建父目录)。
pub fn save(path: &Path, presets: &[PresetMessage]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(presets).unwrap_or_else(|_| "[]".into());
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_stable_unique_hex() {
        assert_eq!(preset_id("时间到"), preset_id("时间到"));
        assert_ne!(preset_id("a"), preset_id("b"));
        assert_eq!(preset_id("x").len(), 8);
    }

    #[test]
    fn from_texts_trims_and_skips_empty() {
        let v = from_texts(&["  hi  ".into(), "".into(), "  ".into(), "yo".into()]);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].text, "hi");
        assert_eq!(v[1].text, "yo");
    }

    #[test]
    fn default_nonempty() {
        assert!(!default_presets().is_empty());
    }
}
