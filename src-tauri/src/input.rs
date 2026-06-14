//! 模拟鼠标 / 键盘输入（spec §11.8）。
//!
//! Windows 下用 Win32 `SendInput` 实现；其它平台返回 `Unsupported`，
//! 这样服务层可以在 Linux/macOS 上正常编译与联调（点击会被拒绝）。

#[derive(Debug)]
pub enum InputError {
    Unsupported,
    #[allow(dead_code)]
    Failed(String),
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputError::Unsupported => write!(f, "当前平台不支持模拟输入（仅 Windows）"),
            InputError::Failed(e) => write!(f, "模拟输入失败: {e}"),
        }
    }
}

impl std::error::Error for InputError {}

/// 把物理屏幕坐标换算为 SendInput 绝对坐标（虚拟桌面 0..=65535）。
pub fn to_virtual_desktop_abs(
    x: i32,
    y: i32,
    vleft: i32,
    vtop: i32,
    vwidth: i32,
    vheight: i32,
) -> (i32, i32) {
    let w = (vwidth - 1).max(1) as i64;
    let h = (vheight - 1).max(1) as i64;
    let ax = ((x - vleft) as i64 * 65535 / w) as i32;
    let ay = ((y - vtop) as i64 * 65535 / h) as i32;
    (ax.clamp(0, 65535), ay.clamp(0, 65535))
}

#[cfg(windows)]
mod imp {
    use super::InputError;

    /// 在屏幕坐标 (x, y) 处模拟一次左键单击。
    pub fn click_screen(x: i32, y: i32) -> Result<(), InputError> {
        // TODO(windows): SetCursorPos(x, y) + SendInput(LEFTDOWN/LEFTUP)，
        // 必要时点击前后保存/恢复鼠标位置（spec §11.8）。
        let _ = (x, y);
        Ok(())
    }

    pub fn key_next() -> Result<(), InputError> {
        // TODO(windows): SendInput VK_RIGHT（或 Space / PageDown）。
        Ok(())
    }

    pub fn key_prev() -> Result<(), InputError> {
        // TODO(windows): SendInput VK_LEFT（或 PageUp）。
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::InputError;

    pub fn click_screen(_x: i32, _y: i32) -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }

    pub fn key_next() -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }

    pub fn key_prev() -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }
}

pub use imp::{click_screen, key_next, key_prev};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_monitor_corners() {
        // 虚拟桌面 1920x1080，原点 (0,0)
        assert_eq!(to_virtual_desktop_abs(0, 0, 0, 0, 1920, 1080), (0, 0));
        assert_eq!(to_virtual_desktop_abs(1919, 1079, 0, 0, 1920, 1080), (65535, 65535));
    }

    #[test]
    fn negative_origin_multimonitor() {
        // 左侧副屏使虚拟桌面原点为负
        assert_eq!(to_virtual_desktop_abs(-1920, 0, -1920, 0, 3840, 1080).0, 0);
    }

    #[test]
    fn clamps_outside() {
        let (ax, _) = to_virtual_desktop_abs(99999, 0, 0, 0, 1920, 1080);
        assert_eq!(ax, 65535);
    }
}
