//! 窗口枚举与 PPT 放映窗口识别（spec §11.6）。
//!
//! Windows 下用 Win32 `EnumWindows` / `GetWindowRect` 等实现；
//! 其它平台返回空集合，保证服务层可跨平台编译。

use crate::app_state::TargetWindow;

/// 把归一化坐标 (x,y)∈[0,1] 映射为窗口矩形内的屏幕坐标（物理像素）。
pub fn map_normalized_to_screen(
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    x: f64,
    y: f64,
) -> (i32, i32) {
    let cx = x.clamp(0.0, 1.0);
    let cy = y.clamp(0.0, 1.0);
    let sx = left + (cx * width as f64).round() as i32;
    let sy = top + (cy * height as f64).round() as i32;
    (sx, sy)
}

/// 枚举过滤判定：是否是值得列出的可截取窗口。纯函数，便于单测。
pub fn is_capturable_window(title: &str, _style: u32, exstyle: u32, width: i32, height: i32) -> bool {
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    !title.trim().is_empty()
        && (exstyle & WS_EX_TOOLWINDOW) == 0
        && width >= 100
        && height >= 100
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_center_and_corners() {
        // 1920x1080 窗口在 (0,0)
        assert_eq!(map_normalized_to_screen(0, 0, 1920, 1080, 0.0, 0.0), (0, 0));
        assert_eq!(map_normalized_to_screen(0, 0, 1920, 1080, 1.0, 1.0), (1920, 1080));
        assert_eq!(map_normalized_to_screen(0, 0, 1920, 1080, 0.5, 0.5), (960, 540));
    }

    #[test]
    fn applies_window_offset() {
        assert_eq!(map_normalized_to_screen(100, 200, 800, 600, 0.5, 0.5), (500, 500));
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(map_normalized_to_screen(0, 0, 1000, 1000, 1.5, -0.2), (1000, 0));
    }

    #[test]
    fn rejects_empty_title_toolwindow_and_tiny() {
        const TOOL: u32 = 0x0000_0080; // WS_EX_TOOLWINDOW
        assert!(!is_capturable_window("", 0, 0, 800, 600)); // 空标题
        assert!(!is_capturable_window("x", 0, TOOL, 800, 600)); // 工具窗
        assert!(!is_capturable_window("x", 0, 0, 50, 50)); // 太小
    }

    #[test]
    fn accepts_normal_window() {
        assert!(is_capturable_window("活动答题.pptx - PowerPoint", 0, 0, 1920, 1080));
    }
}
