//! WGC 后台捕获会话（仅 Windows）。把每帧紧排 BGRA 写入共享最新帧缓冲。
//! 基于 windows-capture 2.0 的 GraphicsCaptureApiHandler。

use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame as WgcFrame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::capture::CapturedFrame;

type WgcError = Box<dyn std::error::Error + Send + Sync>;

/// 停控句柄类型别名（Send+Sync）。供 CaptureManager 持有。
pub type Control = CaptureControl<Handler, WgcError>;

/// 捕获事件处理器：每帧写入共享缓冲并通知编码线程。
/// Flags 携带共享缓冲 + 编码线程的 signal sender。
pub struct Handler {
    latest: Arc<Mutex<Option<CapturedFrame>>>,
    signal: SyncSender<()>,
    scratch: Vec<u8>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (Arc<Mutex<Option<CapturedFrame>>>, SyncSender<()>);
    type Error = WgcError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (latest, signal) = ctx.flags;
        Ok(Self { latest, signal, scratch: Vec::new() })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut WgcFrame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let (w, h) = (frame.width(), frame.height());
        let fb = frame.buffer()?;
        // 去掉行 padding，得到紧排 BGRA（ColorFormat::Bgra8）。
        let bgra = fb.as_nopadding_buffer(&mut self.scratch);
        if let Ok(mut g) = self.latest.lock() {
            *g = Some(CapturedFrame { bgra: bgra.to_vec(), width: w, height: h });
        }
        // 满则丢（合并），不阻塞捕获线程；编码线程据此醒来做去重 + 编码。
        let _ = self.signal.try_send(());
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        if let Ok(mut g) = self.latest.lock() {
            *g = None;
        }
        Ok(())
    }
}

/// 对指定 HWND 起一个后台 WGC 会话；帧写入 `latest`、每帧 `try_send(())` 通知编码线程。返回停控句柄。
pub fn start(
    hwnd: isize,
    latest: Arc<Mutex<Option<CapturedFrame>>>,
    signal: SyncSender<()>,
) -> Result<Control, String> {
    let window = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);
    let settings = Settings::new(
        window,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (latest, signal),
    );
    Handler::start_free_threaded(settings).map_err(|e| e.to_string())
}
