//! 截取目标窗口画面并编码为 JPEG（spec §11.7）。
//!
//! MVP 阶段：未选窗口时返回占位图，使"手机看到画面"这条链路在任何平台都能联调。
//! Windows 下用屏幕 DC `BitBlt` 截取窗口实时客户区矩形（截观众所见、避开逐窗口黑帧）。

use image::{
    codecs::jpeg::JpegEncoder, imageops::FilterType, ExtendedColorType, ImageBuffer, ImageEncoder,
    Rgb, RgbImage,
};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};

use crate::app_state::{AppState, TargetWindow};
use crate::frame_broker::{EncodedFrame, FrameBroker};
#[cfg(windows)]
use crate::frame_broker::sample_hash;

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

/// 管理捕获源（WGC 主 / BitBlt 兜底）、最新原始帧缓冲、帧 broker 与编码线程。
/// 存于 AppState，被 snapshot 读、被选窗启停；编码线程把变化帧 publish 到 broker。
pub struct CaptureManager {
    /// 捕获源写入的最新一帧（原始 BGRA）；snapshot 与编码线程读取。
    latest: Arc<Mutex<Option<CapturedFrame>>>,
    /// 一处编码、watch 扇出最新编码帧；主持人 MJPEG / 导播 Channel 各自订阅。
    broker: Arc<FrameBroker>,
    #[cfg(windows)]
    control: Mutex<Option<crate::capture_session::Control>>,
    /// BitBlt 兜底循环的停止信号（WGC 编码线程不看它，靠 sender drop 退出）。
    stop_flag: Arc<AtomicBool>,
    /// 编码线程句柄（WGC signal 驱动 或 BitBlt 定时循环），stop() 时 join。
    encoder: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl CaptureManager {
    pub fn new() -> Self {
        Self {
            latest: Arc::new(Mutex::new(None)),
            broker: Arc::new(FrameBroker::new()),
            #[cfg(windows)]
            control: Mutex::new(None),
            stop_flag: Arc::new(AtomicBool::new(false)),
            encoder: Mutex::new(None),
        }
    }

    /// 主持人 MJPEG / 导播 Channel 订阅入口：watch 扇出最新编码帧。
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Arc<EncodedFrame>> {
        self.broker.subscribe()
    }

    /// 取当前最新编码帧（订阅前先推一帧用）。
    pub fn latest_encoded(&self) -> Arc<EncodedFrame> {
        self.broker.latest()
    }

    /// 取最新原始帧的克隆（无则 None → snapshot 回退）。
    pub fn latest_frame(&self) -> Option<CapturedFrame> {
        self.latest.lock().ok().and_then(|g| g.clone())
    }

    /// 对指定窗口启动捕获（停旧、清最新帧、重置 stop）。
    /// WGC 成功 → 起 signal 驱动编码线程；失败 → 起 BitBlt 兜底定时线程。两者都喂同一 broker。
    pub fn start(&self, hwnd: isize) {
        self.stop();
        if let Ok(mut g) = self.latest.lock() {
            *g = None;
        }
        self.stop_flag.store(false, Ordering::SeqCst);

        #[cfg(windows)]
        {
            let (sig_tx, sig_rx) = sync_channel::<()>(1);
            // sig_tx 整体移交给 WGC handler 持有（不留本端副本）：这样 handler 是唯一 sender，
            // 无论 stop() 关 WGC、还是 WGC 自行终止（捕获窗口关闭/帧错误），handler 一 drop sig_rx 即关闭,
            // 编码线程的 `for _ in sig_rx` 随之结束、线程自退,不会滞留。
            match crate::capture_session::start(hwnd, self.latest.clone(), sig_tx) {
                Ok(ctrl) => {
                    if let Ok(mut c) = self.control.lock() {
                        *c = Some(ctrl);
                    }
                    let latest = self.latest.clone();
                    let broker = self.broker.clone();
                    let handle = std::thread::spawn(move || {
                        let mut last_hash = 0u64;
                        for _ in sig_rx {
                            encode_changed(&latest, &broker, &mut last_hash);
                        }
                    });
                    if let Ok(mut e) = self.encoder.lock() {
                        *e = Some(handle);
                    }
                }
                // 失败（旧系统/WGC 不可用）→ BitBlt 定时兜底，仍喂同一 broker。
                Err(e) => {
                    eprintln!("[stagedeck] WGC 启动失败，回退 BitBlt 定时抓取：{e}");
                    self.start_bitblt_fallback(hwnd);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = hwnd;
        }
    }

    /// BitBlt 兜底：定时抓原始帧 → 采样去重 → 变化才编码 q82 → publish。
    #[cfg(windows)]
    fn start_bitblt_fallback(&self, hwnd: isize) {
        let broker = self.broker.clone();
        let stop = self.stop_flag.clone();
        let handle = std::thread::spawn(move || {
            let mut last_hash = 0u64;
            let win = crate::app_state::TargetWindow {
                title: String::new(),
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                handle: hwnd,
            };
            while !stop.load(Ordering::SeqCst) {
                if let Ok(frame) = capture_window_raw(&win) {
                    let h = sample_hash(&frame.bgra);
                    if h != last_hash {
                        last_hash = h;
                        if let Ok(jpeg) = encode_captured_frame(&frame, 1280, 720, 82) {
                            broker.publish(jpeg, frame.width, frame.height);
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        });
        if let Ok(mut e) = self.encoder.lock() {
            *e = Some(handle);
        }
    }

    /// 停止当前会话：停 BitBlt 循环 + 关 WGC（handler 持有的唯一 sender drop → sig_rx 关闭 → 编码线程退出）+ join。
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        #[cfg(windows)]
        {
            if let Ok(mut c) = self.control.lock() {
                if let Some(ctrl) = c.take() {
                    let _ = ctrl.stop(); // 关 WGC → handler drop → 其 sender drop → sig_rx 关闭
                }
            }
        }
        if let Ok(mut e) = self.encoder.lock() {
            if let Some(h) = e.take() {
                let _ = h.join();
            }
        }
    }
}

impl Default for CaptureManager {
    fn default() -> Self {
        Self::new()
    }
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

/// WGC 编码线程用：锁内做廉价采样 hash 去重，仅变化时才克隆整缓冲（~8MB）出来编码 q82、publish。
/// "drop lock before heavy work"：lock 作用域只到决定是否变化，编码在锁外。
#[cfg(windows)]
fn encode_changed(
    latest: &Arc<Mutex<Option<CapturedFrame>>>,
    broker: &Arc<FrameBroker>,
    last_hash: &mut u64,
) {
    let changed = {
        let g = match latest.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match &*g {
            Some(f) => {
                let h = sample_hash(&f.bgra);
                if h != *last_hash {
                    *last_hash = h;
                    Some(f.clone())
                } else {
                    None
                }
            }
            None => None,
        }
    };
    if let Some(f) = changed {
        if let Ok(jpeg) = encode_captured_frame(&f, 1280, 720, 82) {
            broker.publish(jpeg, f.width, f.height);
        }
    }
}

/// 优先返回 WGC 最新帧；无帧则回退 BitBlt（用 selected_window）；再不行占位图。
pub fn snapshot_jpeg(
    state: &AppState,
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    // 1) WGC 最新帧
    if let Some(frame) = state.capture.latest_frame() {
        return encode_captured_frame(&frame, max_w, max_h, quality);
    }
    // 2) BitBlt 回退
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

/// 抓窗口当前矩形为原始 BGRA 帧（Windows：屏幕 DC BitBlt）。
/// 供编码线程的 BitBlt 兜底循环复用；编码走 `encode_captured_frame`。
#[cfg(windows)]
pub fn capture_window_raw(win: &TargetWindow) -> Result<CapturedFrame, CaptureError> {
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

        Ok(CapturedFrame { bgra: pixels, width: width as u32, height: height as u32 })
    }
}

#[cfg(not(windows))]
pub fn capture_window_raw(_win: &TargetWindow) -> Result<CapturedFrame, CaptureError> {
    Err(CaptureError::Encode("非 Windows：无窗口截图".into()))
}

#[cfg(windows)]
fn capture_window(
    win: &TargetWindow,
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    let frame = capture_window_raw(win)?;
    encode_captured_frame(&frame, max_w, max_h, quality)
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

    #[test]
    fn capture_manager_latest_frame_roundtrip() {
        let mgr = CaptureManager::new();
        assert!(mgr.latest_frame().is_none());
        // Write directly into the shared buffer (test is in-module → can access private field)
        *mgr.latest.lock().unwrap() = Some(CapturedFrame { bgra: vec![1, 2, 3, 4], width: 1, height: 1 });
        let f = mgr.latest_frame().unwrap();
        assert_eq!((f.width, f.height, f.bgra.len()), (1, 1, 4));
        mgr.stop(); // no-op on non-Windows, must not panic
    }

    #[test]
    fn raw_capture_unavailable_off_windows_is_err() {
        // 非 Windows 下 capture_window_raw 返回 Err(占位走 snapshot_jpeg 那条),不 panic
        let win = crate::app_state::TargetWindow {
            title: "x".into(), left: 0, top: 0, width: 0, height: 0, handle: 0,
        };
        assert!(capture_window_raw(&win).is_err());
    }

    #[test]
    fn snapshot_falls_back_to_placeholder_when_empty() {
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let state = crate::app_state::AppState::new("tok".into(), 8787, tx);
        // No WGC frame, no selected_window → placeholder (a valid JPEG)
        let jpeg = snapshot_jpeg(&state, 1280, 720, 70).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert!(decoded.width() >= 16 && decoded.height() >= 16);
    }
}
