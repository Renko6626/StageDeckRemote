//! 窗口枚举与 PPT 放映窗口识别（spec §11.6）。
//!
//! Windows 下用 Win32 `EnumWindows` / `GetWindowRect` 等实现；
//! 其它平台返回空集合，保证服务层可跨平台编译。

use crate::app_state::TargetWindow;

#[cfg(windows)]
mod imp {
    use super::TargetWindow;

    /// 枚举当前可见、带标题的顶层窗口。
    pub fn list_windows() -> Vec<TargetWindow> {
        // TODO(windows): EnumWindows + IsWindowVisible + GetWindowTextW + GetWindowRect。
        Vec::new()
    }

    /// 自动识别 PowerPoint 放映窗口（标题含 "PowerPoint" / 放映窗口类名 "screenClass"）。
    pub fn find_powerpoint() -> Option<TargetWindow> {
        // TODO(windows)
        None
    }

    /// 读取指定句柄窗口当前矩形（位置可能变化）。
    pub fn window_rect(handle: isize) -> Option<TargetWindow> {
        let _ = handle;
        None
    }

    /// 检查目标窗口是否仍然存在。
    pub fn window_exists(handle: isize) -> bool {
        let _ = handle;
        false
    }

    /// 将目标窗口置前（翻页前确保 PPT 窗口获得焦点，spec §18.2）。
    pub fn bring_to_front(handle: isize) {
        let _ = handle;
    }
}

#[cfg(not(windows))]
mod imp {
    use super::TargetWindow;

    pub fn list_windows() -> Vec<TargetWindow> {
        Vec::new()
    }

    pub fn find_powerpoint() -> Option<TargetWindow> {
        None
    }

    pub fn window_rect(_handle: isize) -> Option<TargetWindow> {
        None
    }

    pub fn window_exists(_handle: isize) -> bool {
        false
    }

    pub fn bring_to_front(_handle: isize) {}
}

pub use imp::{bring_to_front, find_powerpoint, list_windows, window_exists, window_rect};
