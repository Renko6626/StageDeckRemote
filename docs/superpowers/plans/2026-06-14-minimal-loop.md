# 最短闭环 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 `window_manager`/`capture`/`input` 三个原生模块的 Windows 真实逻辑，打通"导播选窗 → 手机看真实画面 → 手机点击 → PPT 真实点击"并加上/下翻页。

**Architecture:** 每个原生模块拆成**纯函数（跨平台、`#[cfg(test)]` 单测）** + **Win32 FFI 薄壳（`#[cfg(windows)]`）**。截图用屏幕 DC `BitBlt` 取窗口实时客户区矩形；点击用 `SendInput` 绝对坐标并还原鼠标；翻页由调用方先 `SetForegroundWindow` 再发方向键。坐标统一物理像素（进程设 PMv2 DPI 感知）。

**Tech Stack:** Rust、windows-rs 0.58、image 0.25、Axum（已有）、Tauri v2（已有）。

> ⚠️ **构建/测试约束**：`src-tauri` 依赖 `tauri`，在开发用的 Linux 机器上**无法编译**（缺 webkit2gtk）。
> 因此**所有 `cargo test` / `cargo build` 步骤都在 Windows 上执行**（`cd src-tauri && cargo test`）。
> 纯函数任务（1–5）是严格 TDD：先写失败测试，再写实现，在 Windows 上跑红→绿。
> FFI 任务（7–10）无法纯 TDD，给出完整实现 + Windows 集成/手动验证步骤；
> windows-rs 的精确 import 路径以 Windows 上 `cargo build` 为准（编译器/rust-analyzer 会指明）。

---

## 文件结构

| 文件 | 责任 | 改动 |
| --- | --- | --- |
| `src-tauri/src/window_manager.rs` | 纯：`map_normalized_to_screen`、`is_capturable_window`；FFI：枚举/矩形/置前 | 大改 |
| `src-tauri/src/capture.rs` | 纯：`bgra_to_rgb`、`encode_scaled_jpeg`、`scaled_dims`；FFI：`capture_window` | 大改 |
| `src-tauri/src/input.rs` | 纯：`to_virtual_desktop_abs`；FFI：`click_screen`/`key_next`/`key_prev` | 大改 |
| `src-tauri/src/routes.rs` | click 改用实时矩形 + `map_normalized_to_screen`；翻页前 `bring_to_front` | 中改 |
| `src-tauri/src/commands.rs` | `local_next/prev` 翻页前 `bring_to_front` | 小改 |
| `src-tauri/src/lib.rs` | 启动设 PMv2 DPI 感知（`#[cfg(windows)]`） | 小改 |
| `src-tauri/Cargo.toml` | `windows` 增 `Win32_UI_HiDpi` feature | 小改 |

纯函数放在各模块顶层（`#[cfg(windows)] mod imp` 之外），保证跨平台编译；FFI 实现放进 `imp`。

---

## Task 1: 纯函数 `map_normalized_to_screen`（window_manager）

**Files:**
- Modify: `src-tauri/src/window_manager.rs`（在文件顶层、`mod imp` 之前加纯函数与测试）

- [ ] **Step 1: 写失败测试**

在 `window_manager.rs` 末尾追加：

```rust
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
}
```

- [ ] **Step 2: 在 Windows 跑，确认失败**

Run（Windows）: `cd src-tauri && cargo test window_manager::tests -- --nocapture`
Expected: 编译失败 `cannot find function map_normalized_to_screen`。

- [ ] **Step 3: 写实现**

在 `window_manager.rs` 顶层（`use crate::app_state::TargetWindow;` 之后、`#[cfg(windows)] mod imp` 之前）加：

```rust
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
```

- [ ] **Step 4: 在 Windows 跑，确认通过**

Run（Windows）: `cd src-tauri && cargo test window_manager::tests`
Expected: PASS（3 tests）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/window_manager.rs
git commit -m "feat(window_manager): pure map_normalized_to_screen with tests"
```

---

## Task 2: 纯函数 `to_virtual_desktop_abs`（input）

**Files:**
- Modify: `src-tauri/src/input.rs`

- [ ] **Step 1: 写失败测试**

在 `input.rs` 末尾追加：

```rust
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
```

- [ ] **Step 2: 在 Windows 跑，确认失败**

Run（Windows）: `cd src-tauri && cargo test input::tests`
Expected: 编译失败 `cannot find function to_virtual_desktop_abs`。

- [ ] **Step 3: 写实现**

在 `input.rs` 顶层（`impl std::error::Error for InputError {}` 之后、`#[cfg(windows)] mod imp` 之前）加：

```rust
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
```

- [ ] **Step 4: 在 Windows 跑，确认通过**

Run（Windows）: `cd src-tauri && cargo test input::tests`
Expected: PASS（3 tests）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/input.rs
git commit -m "feat(input): pure to_virtual_desktop_abs with tests"
```

---

## Task 3: 纯函数 `is_capturable_window`（window_manager）

**Files:**
- Modify: `src-tauri/src/window_manager.rs`

- [ ] **Step 1: 写失败测试**

在 `window_manager.rs` 的 `mod tests` 里追加：

```rust
    #[test]
    fn rejects_empty_title_toolwindow_and_tiny() {
        const TOOL: u32 = 0x0000_0080; // WS_EX_TOOLWINDOW
        assert!(!is_capturable_window("", 0, 0, 800, 600));        // 空标题
        assert!(!is_capturable_window("x", 0, TOOL, 800, 600));    // 工具窗
        assert!(!is_capturable_window("x", 0, 0, 50, 50));         // 太小
    }

    #[test]
    fn accepts_normal_window() {
        assert!(is_capturable_window("活动答题.pptx - PowerPoint", 0, 0, 1920, 1080));
    }
```

- [ ] **Step 2: 在 Windows 跑，确认失败**

Run（Windows）: `cd src-tauri && cargo test window_manager::tests::rejects`
Expected: 编译失败 `cannot find function is_capturable_window`。

- [ ] **Step 3: 写实现**

在 `window_manager.rs` 顶层加：

```rust
/// 枚举过滤判定：是否是值得列出的可截取窗口。纯函数，便于单测。
pub fn is_capturable_window(title: &str, _style: u32, exstyle: u32, width: i32, height: i32) -> bool {
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    !title.trim().is_empty()
        && (exstyle & WS_EX_TOOLWINDOW) == 0
        && width >= 100
        && height >= 100
}
```

- [ ] **Step 4: 在 Windows 跑，确认通过**

Run（Windows）: `cd src-tauri && cargo test window_manager::tests`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/window_manager.rs
git commit -m "feat(window_manager): pure is_capturable_window filter with tests"
```

---

## Task 4: 纯函数 `bgra_to_rgb`（capture）

**Files:**
- Modify: `src-tauri/src/capture.rs`

- [ ] **Step 1: 写失败测试**

在 `capture.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_bgra_to_rgb_swapping_channels() {
        // 1 像素 BGRA = (B=10, G=20, R=30, A=255) → RGB (30,20,10)
        let bgra = [10u8, 20, 30, 255];
        assert_eq!(bgra_to_rgb(&bgra, 1, 1), vec![30, 20, 10]);
    }

    #[test]
    fn output_len_is_three_per_pixel() {
        let bgra = vec![0u8; 4 * 6]; // 6 像素
        assert_eq!(bgra_to_rgb(&bgra, 3, 2).len(), 3 * 6);
    }
}
```

- [ ] **Step 2: 在 Windows 跑，确认失败**

Run（Windows）: `cd src-tauri && cargo test capture::tests::converts`
Expected: 编译失败 `cannot find function bgra_to_rgb`。

- [ ] **Step 3: 写实现**

在 `capture.rs` 顶层（`use ...;` 之后、`snapshot_jpeg` 之前）加：

```rust
/// 把自上而下的 BGRA 像素转换为 RGB（丢弃 alpha）。
pub fn bgra_to_rgb(bgra: &[u8], w: u32, h: u32) -> Vec<u8> {
    let px = (w as usize) * (h as usize);
    let mut rgb = Vec::with_capacity(px * 3);
    for i in 0..px {
        let b = bgra.get(i * 4).copied().unwrap_or(0);
        let g = bgra.get(i * 4 + 1).copied().unwrap_or(0);
        let r = bgra.get(i * 4 + 2).copied().unwrap_or(0);
        rgb.push(r);
        rgb.push(g);
        rgb.push(b);
    }
    rgb
}
```

- [ ] **Step 4: 在 Windows 跑，确认通过**

Run（Windows）: `cd src-tauri && cargo test capture::tests`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/capture.rs
git commit -m "feat(capture): pure bgra_to_rgb with tests"
```

---

## Task 5: 纯函数 `scaled_dims` + `encode_scaled_jpeg`，并复用到占位图（capture）

**Files:**
- Modify: `src-tauri/src/capture.rs`

- [ ] **Step 1: 写失败测试**

在 `capture.rs` 的 `mod tests` 里追加：

```rust
    #[test]
    fn scaled_dims_keeps_aspect_within_bounds() {
        assert_eq!(scaled_dims(1920, 1080, 1280, 720), (1280, 720));
        assert_eq!(scaled_dims(800, 600, 1280, 720), (800, 600)); // 已在界限内不放大
        assert_eq!(scaled_dims(2000, 1000, 1000, 1000), (1000, 500));
    }

    #[test]
    fn encodes_decodable_jpeg_at_scaled_size() {
        // 4x4 纯红 → 编码 → 用 image 解回，校验尺寸
        let rgb = vec![255u8, 0, 0].repeat(16);
        let jpeg = encode_scaled_jpeg(&rgb, 4, 4, 1280, 720, 80).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert_eq!((decoded.width(), decoded.height()), (4, 4));
    }
```

- [ ] **Step 2: 在 Windows 跑，确认失败**

Run（Windows）: `cd src-tauri && cargo test capture::tests::scaled`
Expected: 编译失败 `cannot find function scaled_dims`。

- [ ] **Step 3: 写实现 + 重构占位图**

在 `capture.rs` 顶层加（注意更新顶部 `use`）：

```rust
use image::{imageops::FilterType, RgbImage};

/// 等比缩放目标尺寸：超出界限则缩小，未超出则原样（不放大）。
pub fn scaled_dims(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if w <= max_w && h <= max_h {
        return (w, h);
    }
    let r = (max_w as f64 / w as f64).min(max_h as f64 / h as f64);
    let tw = ((w as f64 * r).round() as u32).max(1);
    let th = ((h as f64 * r).round() as u32).max(1);
    (tw, th)
}

/// 把 RGB 缓冲等比缩放到 ≤max 后编码为 JPEG。
pub fn encode_scaled_jpeg(
    rgb: &[u8],
    w: u32,
    h: u32,
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    let img = RgbImage::from_raw(w, h, rgb.to_vec())
        .ok_or_else(|| CaptureError::Encode("RGB 缓冲尺寸不匹配".into()))?;
    let (tw, th) = scaled_dims(w, h, max_w, max_h);
    let out = if (tw, th) == (w, h) {
        img
    } else {
        image::imageops::resize(&img, tw, th, FilterType::Triangle)
    };
    let mut buf: Vec<u8> = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .write_image(out.as_raw(), out.width(), out.height(), ExtendedColorType::Rgb8)
        .map_err(|e| CaptureError::Encode(e.to_string()))?;
    Ok(buf)
}
```

然后把 `placeholder_jpeg` 改为复用 `encode_scaled_jpeg`（替换其内部编码逻辑）：

```rust
fn placeholder_jpeg(w: u32, h: u32, quality: u8) -> Result<Vec<u8>, CaptureError> {
    let w = w.max(16);
    let h = h.max(16);
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| {
        let dx = (x as i64 - w as i64 / 2).abs();
        let dy = (y as i64 - h as i64 / 2).abs();
        if dx < w as i64 / 8 && dy < h as i64 / 8 {
            Rgb([90, 90, 110])
        } else {
            Rgb([24, 24, 28])
        }
    });
    encode_scaled_jpeg(img.as_raw(), w, h, w, h, quality)
}
```

- [ ] **Step 4: 在 Windows 跑，确认通过**

Run（Windows）: `cd src-tauri && cargo test capture::tests`
Expected: PASS（含 Task 4 的用例）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/capture.rs
git commit -m "feat(capture): scaled_dims + encode_scaled_jpeg; reuse in placeholder"
```

---

## Task 6: `Cargo.toml` 增加 `Win32_UI_HiDpi` feature

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 改 features**

把 `[target.'cfg(windows)'.dependencies]` 的 `windows` features 改为：

```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
  "Win32_Foundation",
  "Win32_UI_WindowsAndMessaging",
  "Win32_UI_Input_KeyboardAndMouse",
  "Win32_UI_HiDpi",
  "Win32_Graphics_Gdi",
] }
```

- [ ] **Step 2: 在 Windows 验证可解析**

Run（Windows）: `cd src-tauri && cargo build`
Expected: 编译通过（功能未用，仅确认 feature 名有效）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/Cargo.toml
git commit -m "build(deps): enable windows Win32_UI_HiDpi feature"
```

---

## Task 7: 启动设置 PMv2 DPI 感知（lib.rs）

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 加 DPI 感知调用**

在 `lib.rs` 的 `pub fn run()` 第一行（生成 token 之前）插入：

```rust
    // Windows：进程设为 Per-Monitor-DPI-Aware v2，使窗口矩形/屏幕坐标/SendInput 统一物理像素。
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
```

- [ ] **Step 2: 在 Windows 构建验证**

Run（Windows）: `cd src-tauri && cargo build`
Expected: 编译通过。
（若 `SetProcessDpiAwarenessContext` 返回 `Result`，把 `let _ =` 保留即可忽略。）

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: set Per-Monitor-DPI-Aware v2 at startup (windows)"
```

---

## Task 8: `window_manager` FFI —— 枚举 / 实时矩形 / 存在性 / 置前

**Files:**
- Modify: `src-tauri/src/window_manager.rs`（`#[cfg(windows)] mod imp`）

> FFI 任务：无法纯 TDD。先实现，再用 Task 12 的集成/手动测试在 Windows 验证。
> windows-rs 精确 import 路径以 Windows 上 `cargo build` 为准。

- [ ] **Step 1: 实现 `#[cfg(windows)] mod imp`**

用以下实现替换 `window_manager.rs` 里 `#[cfg(windows)] mod imp { ... }` 整块：

```rust
#[cfg(windows)]
mod imp {
    use super::{is_capturable_window, TargetWindow};
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
        IsWindow, IsWindowVisible, SetForegroundWindow, GWL_EXSTYLE, GWL_STYLE,
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

    /// 客户区在屏幕坐标系下的矩形（物理像素）。
    fn client_screen_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc).ok()?;
            let mut origin = POINT { x: 0, y: 0 };
            // ClientToScreen 把客户区 (0,0) 映射到屏幕坐标。
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

    pub fn find_powerpoint() -> Option<TargetWindow> {
        // 本轮保持未实现（自动识别属下一增量）。
        None
    }

    pub fn window_rect(handle: isize) -> Option<TargetWindow> {
        to_target(isize_to_hwnd(handle))
    }

    pub fn window_exists(handle: isize) -> bool {
        unsafe { IsWindow(isize_to_hwnd(handle)).as_bool() }
    }

    pub fn bring_to_front(handle: isize) {
        unsafe {
            let _ = SetForegroundWindow(isize_to_hwnd(handle));
        }
    }
}
```

- [ ] **Step 2: 在 Windows 构建**

Run（Windows）: `cd src-tauri && cargo build`
Expected: 编译通过（如个别 import 路径报错，按编译器提示修正模块路径）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/window_manager.rs
git commit -m "feat(window_manager): EnumWindows listing + live client rect (windows)"
```

---

## Task 9: `capture` FFI —— 屏幕 DC BitBlt 截窗口实时矩形

**Files:**
- Modify: `src-tauri/src/capture.rs`（`#[cfg(windows)] capture_window`）

- [ ] **Step 1: 实现 `#[cfg(windows)] capture_window`**

用以下实现替换 `capture.rs` 里 `#[cfg(windows)] fn capture_window(...) { ... }`：

```rust
#[cfg(windows)]
fn capture_window(
    win: &TargetWindow,
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };

    // 实时矩形：窗口可能已移动/关闭。
    let rect = crate::window_manager::window_rect(win.handle)
        .ok_or_else(|| CaptureError::Encode("窗口已不存在".into()))?;
    let (left, top, width, height) = (rect.left, rect.top, rect.width, rect.height);
    if width <= 0 || height <= 0 {
        return Err(CaptureError::Encode("窗口尺寸非法".into()));
    }

    unsafe {
        // 屏幕 DC（整个桌面），从中 BitBlt 出窗口所在矩形。
        let screen_dc = GetDC(HWND::default());
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, width, height);
        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));

        let ok = BitBlt(mem_dc, 0, 0, width, height, screen_dc, left, top, SRCCOPY).is_ok();

        // 取出像素为自上而下的 32 位 BGRA。
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // 负 = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let scanlines = GetDIBits(
            mem_dc,
            bmp,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // 清理 GDI 资源。
        SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        ReleaseDC(HWND::default(), screen_dc);

        if !ok || scanlines == 0 {
            return Err(CaptureError::Encode("BitBlt/GetDIBits 失败".into()));
        }

        let rgb = bgra_to_rgb(&pixels, width as u32, height as u32);
        encode_scaled_jpeg(&rgb, width as u32, height as u32, max_w, max_h, quality)
    }
}
```

- [ ] **Step 2: 在 Windows 构建**

Run（Windows）: `cd src-tauri && cargo build`
Expected: 编译通过（如 `BITMAPINFOHEADER`/`BI_RGB` 等类型路径或字段名报错，按编译器提示修正）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/capture.rs
git commit -m "feat(capture): real window capture via screen-DC BitBlt (windows)"
```

---

## Task 10: `input` FFI —— 绝对坐标点击（含还原）+ 翻页按键

**Files:**
- Modify: `src-tauri/src/input.rs`（`#[cfg(windows)] mod imp`）

- [ ] **Step 1: 实现 `#[cfg(windows)] mod imp`**

用以下实现替换 `input.rs` 里 `#[cfg(windows)] mod imp { ... }`：

```rust
#[cfg(windows)]
mod imp {
    use super::{to_virtual_desktop_abs, InputError};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, VIRTUAL_KEY, VK_LEFT, VK_RIGHT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    fn send(inputs: &[INPUT]) -> Result<(), InputError> {
        let sent = unsafe { SendInput(inputs, core::mem::size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() {
            Ok(())
        } else {
            Err(InputError::Failed("SendInput 注入数不足".into()))
        }
    }

    fn mouse_abs(ax: i32, ay: i32, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: ax,
                    dy: ay,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(flags),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    pub fn click_screen(x: i32, y: i32) -> Result<(), InputError> {
        unsafe {
            // 记录当前鼠标位置以便还原。
            let mut prev = POINT::default();
            let _ = GetCursorPos(&mut prev);

            let vleft = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let vtop = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let vwidth = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let vheight = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            let (ax, ay) = to_virtual_desktop_abs(x, y, vleft, vtop, vwidth, vheight);

            let move_flags = MOUSEEVENTF_MOVE.0 | MOUSEEVENTF_ABSOLUTE.0 | MOUSEEVENTF_VIRTUALDESK.0;
            let inputs = [
                mouse_abs(ax, ay, move_flags),
                mouse_abs(ax, ay, MOUSEEVENTF_LEFTDOWN.0),
                mouse_abs(ax, ay, MOUSEEVENTF_LEFTUP.0),
            ];
            send(&inputs)?;

            // 还原鼠标到原位（减小光标出现在观众屏的概率）。
            let (rax, ray) = to_virtual_desktop_abs(prev.x, prev.y, vleft, vtop, vwidth, vheight);
            let _ = send(&[mouse_abs(rax, ray, move_flags)]);
        }
        Ok(())
    }

    fn key_press(vk: VIRTUAL_KEY) -> Result<(), InputError> {
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        send(&[down, up])
    }

    pub fn key_next() -> Result<(), InputError> {
        key_press(VK_RIGHT)
    }

    pub fn key_prev() -> Result<(), InputError> {
        key_press(VK_LEFT)
    }
}
```

- [ ] **Step 2: 在 Windows 构建**

Run（Windows）: `cd src-tauri && cargo build`
Expected: 编译通过（`INPUT_0`/`MOUSE_EVENT_FLAGS` 等联合体/类型字段名以编译器提示为准）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/input.rs
git commit -m "feat(input): absolute SendInput click with restore + arrow-key page-turn (windows)"
```

---

## Task 11: 接线 —— click 用实时矩形；翻页前置前窗口

**Files:**
- Modify: `src-tauri/src/routes.rs`（`click` 与 `page_turn`）
- Modify: `src-tauri/src/commands.rs`（`local_next`/`local_prev`）

- [ ] **Step 1: 改 `routes.rs` 的 `click`**

把 `click` 里"读取目标窗口矩形并映射"那段（取 `selected_window` 后内联计算 `screen_x/screen_y`）替换为：用 handle 实时取矩形 + 纯函数映射：

```rust
    // 取目标窗口 handle，再实时读取矩形（窗口可能已移动）。
    let handle = {
        let i = state.inner.lock().expect("state lock poisoned");
        i.selected_window.as_ref().map(|w| w.handle)
    };
    let Some(handle) = handle else {
        return json_err(StatusCode::CONFLICT, "ppt_window_not_found");
    };
    let Some(rect) = crate::window_manager::window_rect(handle) else {
        return json_err(StatusCode::CONFLICT, "ppt_window_not_found");
    };
    let (screen_x, screen_y) =
        crate::window_manager::map_normalized_to_screen(rect.left, rect.top, rect.width, rect.height, req.x, req.y);
```

（其余 `input::click_screen(screen_x, screen_y)` 及日志保持不变。）

- [ ] **Step 2: 改 `routes.rs` 的 `page_turn`，翻页前置前窗口**

在 `page_turn` 里 `input::key_next/prev` 调用之前插入置前：

```rust
    // 翻页前激活目标窗口，确保按键送达放映窗口（spec §18.2）。
    if let Some(handle) = {
        let i = state.inner.lock().expect("state lock poisoned");
        i.selected_window.as_ref().map(|w| w.handle)
    } {
        crate::window_manager::bring_to_front(handle);
    }
```

- [ ] **Step 3: 改 `commands.rs` 的 `local_next`/`local_prev`**

让本地翻页也先置前。把这两个命令改为接收 state 并置前：

```rust
#[tauri::command]
pub fn local_next(state: State<Arc<AppState>>) -> Result<(), String> {
    activate_selected(&state);
    input::key_next().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn local_prev(state: State<Arc<AppState>>) -> Result<(), String> {
    activate_selected(&state);
    input::key_prev().map_err(|e| e.to_string())
}

fn activate_selected(state: &AppState) {
    if let Ok(i) = state.inner.lock() {
        if let Some(w) = i.selected_window.as_ref() {
            window_manager::bring_to_front(w.handle);
        }
    }
}
```

（`commands.rs` 顶部已 `use crate::{... window_manager};`；`State`/`Arc`/`AppState` 已在用。）

- [ ] **Step 4: 在 Windows 构建**

Run（Windows）: `cd src-tauri && cargo build`
Expected: 编译通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/routes.rs src-tauri/src/commands.rs
git commit -m "feat: click uses live window rect; page-turn activates window first"
```

---

## Task 12: Windows 集成测试 + 手动验收清单

**Files:**
- Modify: `src-tauri/src/window_manager.rs`（加 `#[cfg(all(test, windows))]` 集成测试）
- Create: `docs/superpowers/plans/2026-06-14-minimal-loop-manual-checklist.md`

- [ ] **Step 1: 加 Windows-only 集成测试**

在 `window_manager.rs` 的 `mod tests` 里追加（仅 Windows 编译运行）：

```rust
    #[cfg(windows)]
    #[test]
    fn enumerate_returns_some_visible_windows() {
        // 桌面上通常存在可见有标题窗口（如资源管理器/终端）。
        let wins = list_windows();
        assert!(wins.iter().all(|w| !w.title.trim().is_empty()));
        assert!(wins.iter().all(|w| w.width >= 100 && w.height >= 100));
    }
```

- [ ] **Step 2: 在 Windows 跑全部单测**

Run（Windows）: `cd src-tauri && cargo test`
Expected: 纯函数测试 + 该集成测试 PASS。

- [ ] **Step 3: 写手动验收清单**

创建 `docs/superpowers/plans/2026-06-14-minimal-loop-manual-checklist.md`：

```markdown
# 最短闭环 · Windows 手动验收

前置：`pnpm install`；`cd src-tauri && cargo test` 全绿。

1. `pnpm tauri dev` 启动；导播 GUI 点"枚举窗口" → 出现真实窗口列表。
2. 打开记事本/画图，回到 GUI 选中它 → ControlStatus 显示"已选择"。
3. 手机扫码打开主持人页 → 预览图显示该窗口**真实画面**（非占位图）。
4. 导播 GUI 开启远程控制；手机点"释放控制"。
5. 手机点击预览图上某个可见控件（如画图工具按钮）→ 该控件被真实点击。
6. 手机点"下一页/上一页"（用 PowerPoint 放映验证）→ 翻页生效。
7. 点击后导播电脑鼠标回到原位。
8. 导播 GUI 关闭远程控制 → 手机点击/翻页失效。

任一不过：记录现象，回到对应 Task 修正（截图黑屏→见 spec §8 风险；翻页不生效→`SetForegroundWindow` 受限，考虑 `PostMessage` 兜底）。
```

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/window_manager.rs docs/superpowers/plans/2026-06-14-minimal-loop-manual-checklist.md
git commit -m "test: windows enumeration integration test + manual acceptance checklist"
```

---

## Self-Review

- **Spec 覆盖**：截图 B（Task 9）、实时矩形（Task 8 `window_rect` + Task 11 接线）、PMv2 DPI（Task 7）、
  绝对坐标点击+还原（Task 10）、翻页置前（Task 11）、纯函数切分与测试（Task 1–5、12）、
  `Win32_UI_HiDpi`（Task 6）—— 均有对应任务。`find_powerpoint` 按 spec 非目标保持 stub。
- **占位扫描**：无 TBD/TODO 式占位；每个代码步骤给了完整代码。FFI 任务显式标注"Windows 上
  `cargo build` 校正 import 路径"，这是约束所致、非内容缺失。
- **类型一致**：`map_normalized_to_screen`/`to_virtual_desktop_abs`/`is_capturable_window`/
  `bgra_to_rgb`/`scaled_dims`/`encode_scaled_jpeg` 签名在定义与调用处一致；`window_rect` 返回
  `Option<TargetWindow>`，调用方按此解构；`local_next/prev` 改签名后由 `lib.rs` 的
  `generate_handler!` 注册不变（命令名不变）。
