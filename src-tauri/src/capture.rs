//! 截取目标窗口画面并编码为 JPEG（spec §11.7）。
//!
//! MVP 阶段：返回一张占位图，使"手机看到画面"这条链路在任何平台都能联调。
//! 真实截图（Windows GDI / DXGI）在 `capture_window` 的 `#[cfg(windows)]` 分支中实现。

use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, ImageBuffer, ImageEncoder, Rgb};

use crate::app_state::{AppState, TargetWindow};

#[derive(Debug)]
pub enum CaptureError {
    Locked,
    Encode(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::Locked => write!(f, "状态锁不可用"),
            CaptureError::Encode(e) => write!(f, "JPEG 编码失败: {e}"),
        }
    }
}

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

/// 截取当前选中窗口；未选窗口时返回占位图，方便链路联调。
pub fn snapshot_jpeg(
    state: &AppState,
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    let window = state
        .inner
        .lock()
        .map_err(|_| CaptureError::Locked)?
        .selected_window
        .clone();

    match window {
        Some(win) => capture_window(&win, max_w, max_h, quality),
        None => placeholder_jpeg(max_w, max_h, quality),
    }
}

#[cfg(windows)]
fn capture_window(
    win: &TargetWindow,
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    // TODO(windows): 用 PrintWindow / BitBlt 截取 win.handle (HWND) 客户区，
    // 缩放到 max_w×max_h 后编码为 JPEG。当前先返回占位图。
    let _ = win;
    placeholder_jpeg(max_w, max_h, quality)
}

#[cfg(not(windows))]
fn capture_window(
    _win: &TargetWindow,
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    // 非 Windows：无真实窗口截图能力，返回占位图。
    placeholder_jpeg(max_w, max_h, quality)
}

/// 生成一张占位 JPEG（深色底 + 居中浅色方块），用于确认推流链路。
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

    let mut buf: Vec<u8> = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .write_image(img.as_raw(), w, h, ExtendedColorType::Rgb8)
        .map_err(|e| CaptureError::Encode(e.to_string()))?;
    Ok(buf)
}

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
