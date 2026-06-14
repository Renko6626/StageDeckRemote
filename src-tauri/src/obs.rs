//! OBS WebSocket 控制（spec §11.9 / §6.3）—— P2 功能，当前为占位实现。
//!
//! 真实实现建议使用 `obws` crate 连接 `ws://127.0.0.1:4455`，
//! 通过切换场景实现黑屏 / 恢复 / 待机。

#[derive(Debug)]
pub enum ObsError {
    NotImplemented,
}

impl std::fmt::Display for ObsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObsError::NotImplemented => write!(f, "OBS 控制尚未实现"),
        }
    }
}

impl std::error::Error for ObsError {}

/// 切到黑屏场景。
pub fn black() -> Result<(), ObsError> {
    Err(ObsError::NotImplemented)
}

/// 恢复到 PPT 场景。
pub fn restore() -> Result<(), ObsError> {
    Err(ObsError::NotImplemented)
}
