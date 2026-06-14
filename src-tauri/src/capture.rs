//! 截取目标窗口画面并编码为 JPEG（spec §11.7）。
//!
//! MVP 阶段：未选窗口时返回占位图，使"手机看到画面"这条链路在任何平台都能联调。
//! Windows 下用屏幕 DC `BitBlt` 截取窗口实时客户区矩形（截观众所见、避开逐窗口黑帧）。

use image::{
    codecs::jpeg::JpegEncoder, imageops::FilterType, ExtendedColorType, ImageBuffer, ImageEncoder,
    Rgb, RgbImage,
};

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

/// 一帧紧排 BGRA 像素 + 尺寸。WGC 会话写入、snapshot 读取。跨平台纯数据。
#[derive(Clone, Debug)]
pub struct CapturedFrame {
    pub bgra: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 把一帧 BGRA 编码为 JPEG（复用纯函数 bgra_to_rgb + encode_scaled_jpeg）。
pub fn encode_captured_frame(
    frame: &CapturedFrame,
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    let rgb = bgra_to_rgb(&frame.bgra, frame.width, frame.height);
    encode_scaled_jpeg(&rgb, frame.width, frame.height, max_w, max_h, quality)
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
    encode_scaled_jpeg(img.as_raw(), w, h, w, h, quality)
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

    #[test]
    fn encodes_captured_frame_to_decodable_jpeg() {
        // 2x2 BGRA: all red (B=0,G=0,R=255,A=255)
        let frame = CapturedFrame { bgra: vec![0u8, 0, 255, 255].repeat(4), width: 2, height: 2 };
        let jpeg = encode_captured_frame(&frame, 1280, 720, 80).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
    }
}
