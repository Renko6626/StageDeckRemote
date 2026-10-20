//! 窗口枚举与 PPT 放映窗口识别（spec §11.6）。
//!
//! Windows 下用 Win32 `EnumWindows` / `GetClientRect` 等实现；
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

/// PowerPoint 窗口按类名定优先级:数字越小越优先;非 PPT 窗口返回 None。
/// 类名按 Win32 约定大小写精确匹配。纯函数,便于单测。
pub fn ppt_window_rank(class: &str) -> Option<u8> {
    match class {
        "screenClass" => Some(0),   // 放映窗口(投影画面)
        "PPTFrameClass" => Some(1), // 编辑主窗口
        _ => None,
    }
}

#[cfg(windows)]
mod imp {
    use super::{is_capturable_window, TargetWindow};
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetClientRect, GetWindowLongW, GetWindowTextLengthW,
        GetWindowTextW, IsWindowVisible, SetForegroundWindow, GWL_EXSTYLE, GWL_STYLE,
    };

    fn hwnd_to_isize(h: HWND) -> isize {
        h.0 as isize
    }
    fn isize_to_hwnd(v: isize) -> HWND {
        HWND(v as *mut core::ffi::c_void)
    }

    /// 读取窗口标题。
    fn window_title(hwnd: HWND) -> String {
        unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                return String::new();
            }
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(hwnd, &mut buf);
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }

    /// 客户区在屏幕坐标系下的矩形（物理像素）：(left, top, width, height)。
    fn client_screen_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc).ok()?;
            let mut origin = POINT { x: 0, y: 0 };
            if !ClientToScreen(hwnd, &mut origin).as_bool() {
                return None;
            }
            let width = rc.right - rc.left;
            let height = rc.bottom - rc.top;
            if width <= 0 || height <= 0 {
                return None;
            }
            Some((origin.x, origin.y, width, height))
        }
    }

    fn to_target(hwnd: HWND) -> Option<TargetWindow> {
        let (left, top, width, height) = client_screen_rect(hwnd)?;
        Some(TargetWindow {
            title: window_title(hwnd),
            left,
            top,
            width,
            height,
            handle: hwnd_to_isize(hwnd),
        })
    }

    extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let out = &mut *(lparam.0 as *mut Vec<TargetWindow>);
            if !IsWindowVisible(hwnd).as_bool() {
                return TRUE;
            }
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let title = window_title(hwnd);
            if let Some((_, _, w, h)) = client_screen_rect(hwnd) {
                if is_capturable_window(&title, style, exstyle, w, h) {
                    if let Some(t) = to_target(hwnd) {
                        out.push(t);
                    }
                }
            }
            TRUE
        }
    }

    pub fn list_windows() -> Vec<TargetWindow> {
        let mut out: Vec<TargetWindow> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut out as *mut _ as isize));
        }
        out
    }

    /// 读取窗口类名。
    fn window_class(hwnd: HWND) -> String {
        unsafe {
            let mut buf = [0u16; 256];
            let n = GetClassNameW(hwnd, &mut buf);
            if n <= 0 {
                return String::new();
            }
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }

    struct PptCandidate {
        rank: u8,
        win: TargetWindow,
    }

    extern "system" fn find_ppt_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let best = &mut *(lparam.0 as *mut Option<PptCandidate>);
            if !IsWindowVisible(hwnd).as_bool() {
                return TRUE;
            }
            // 按类名识别 PowerPoint;放映窗口(screenClass)常无标题,故不套用
            // is_capturable_window(它要求非空标题),只要求有有效客户区矩形。
            let Some(rank) = super::ppt_window_rank(&window_class(hwnd)) else {
                return TRUE;
            };
            if let Some(t) = to_target(hwnd) {
                let better = match best {
                    Some(c) => rank < c.rank,
                    None => true,
                };
                if better {
                    *best = Some(PptCandidate { rank, win: t });
                }
            }
            TRUE
        }
    }

    /// 自动识别 PowerPoint 窗口:优先放映窗口(screenClass),其次编辑主窗口(PPTFrameClass)。
    pub fn find_powerpoint() -> Option<TargetWindow> {
        let mut best: Option<PptCandidate> = None;
        unsafe {
            let _ = EnumWindows(Some(find_ppt_proc), LPARAM(&mut best as *mut _ as isize));
        }
        best.map(|c| c.win)
    }

    pub fn window_rect(handle: isize) -> Option<TargetWindow> {
        to_target(isize_to_hwnd(handle))
    }

    pub fn bring_to_front(handle: isize) -> bool {
        unsafe {
            SetForegroundWindow(isize_to_hwnd(handle)).as_bool()
        }
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

    pub fn bring_to_front(_handle: isize) -> bool { false }
}

pub use imp::{bring_to_front, find_powerpoint, list_windows, window_rect};

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

    #[test]
    fn ranks_ppt_window_classes() {
        assert_eq!(ppt_window_rank("screenClass"), Some(0)); // 放映窗口最优先
        assert_eq!(ppt_window_rank("PPTFrameClass"), Some(1)); // 编辑主窗口次之
        assert_eq!(ppt_window_rank("Chrome_WidgetWin_1"), None);
        assert_eq!(ppt_window_rank(""), None);
    }

    #[cfg(windows)]
    #[test]
    fn enumerate_returns_some_visible_windows() {
        // 桌面上通常存在可见有标题窗口（如资源管理器/终端）。
        let wins = list_windows();
        assert!(wins.iter().all(|w| !w.title.trim().is_empty()));
        assert!(wins.iter().all(|w| w.width >= 100 && w.height >= 100));
    }
}
