# 截图 / 推流优化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把"逐帧请求 + 每消费者重复编码"的截图链路改成"一处编码、变化才推、两端推送"——主持人走 MJPEG 流,导播走 Tauri Channel,静止≈零 CPU、切页秒更、换帧无闪烁。

**Architecture:** 新增帧 broker(`tokio::sync::watch` 广播最新编码帧)。一个编码线程在"帧来源"(WGC 主 / BitBlt 兜底)通知时,对原始 BGRA 做采样 hash 去重,变化才编码一次(JPEG q82),publish 到 broker。主持人 `/api/stream.mjpg` 与导播 `subscribe_preview` Channel 各自订阅 broker 扇出。

**Tech Stack:** Rust(tokio watch、std thread、windows-capture 2、image/jpeg、axum 0.7 streaming body、tauri 2 ipc::Channel)、Vue 3、vanilla JS。

**验证命令:**
- 原生测试:`P=/data/sunyunbo/miniconda3/envs/tauri-dev; PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml`
- 交叉编译(Win32/COM/WGC 真目标):`cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
- 前端:`cd /data/sunyunbo/www/StageDeckRemote && pnpm build`;presenter.js 另跑 `node --check src/presenter/presenter.js`

**说明:** WGC/Channel/MJPEG 端到端只能在 Windows 真机验证;本机 `cargo test` 跑 broker/hash/mjpeg 纯逻辑,交叉编译 0 error 证明 Win 目标可编。Windows-only 任务无单测,以"交叉编译 0 error + 既有 27 测试仍绿"为验收。

---

## File Structure
| 文件 | 责任 |
|---|---|
| `src-tauri/src/frame_broker.rs`(新) | `EncodedFrame`、`FrameBroker`(watch 扇出)、`sample_hash`(变化检测)、`mjpeg_part`(分帧字节)——纯逻辑,Linux 可测 |
| `src-tauri/src/capture.rs` | 拆 `capture_window_raw`(BGRA);`CaptureManager` 持 broker + 编码线程 + WGC/BitBlt 源;复用编码纯函数 |
| `src-tauri/src/capture_session.rs` | WGC 帧到达时 `try_send(())` 通知编码线程(Flags 携带 signal sender) |
| `src-tauri/src/routes.rs` | 新 `/api/stream.mjpg` MJPEG 流路由 |
| `src-tauri/src/commands.rs` | 新 `subscribe_preview(channel)`(Tauri Channel 推原始 JPEG 字节);保留 `get_snapshot` |
| `src-tauri/src/lib.rs` | 注册 `subscribe_preview` |
| `src-tauri/Cargo.toml` | 加 `tokio-stream = "0.1"` |
| `src/components/DirectorPreview.vue` | Channel 订阅 → canvas;删 400ms 轮询 |
| `src/presenter/presenter.js` | SYNC 用 `<img src=stream.mjpg>`;删 `loadFrame` 轮询链 |

---

### Task 1: 帧 broker 核心(纯逻辑,Linux 可测)

**Files:**
- Create: `src-tauri/src/frame_broker.rs`
- Modify: `src-tauri/src/lib.rs`(加 `mod frame_broker;`)

- [ ] **Step 1: 写失败测试**(在 `frame_broker.rs` 文件末尾 `#[cfg(test)]`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_hash_is_deterministic_and_sensitive() {
        let a = vec![7u8; 100_000];
        let mut b = a.clone();
        assert_eq!(sample_hash(&a), sample_hash(&a)); // 同缓冲同值
        b[50_000] = b[50_000].wrapping_add(1);        // 改采样命中区一字节
        assert_ne!(sample_hash(&a), sample_hash(&b));
        assert_eq!(sample_hash(&[]), sample_hash(&[])); // 空不 panic
    }

    #[test]
    fn mjpeg_part_frames_with_headers() {
        let part = mjpeg_part(&[0xAA, 0xBB, 0xCC]);
        let text = String::from_utf8_lossy(&part);
        assert!(text.starts_with("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: 3\r\n\r\n"));
        assert_eq!(&part[part.len() - 5..], &[0xAA, 0xBB, 0xCC, b'\r', b'\n']); // 数据 + 收尾 CRLF
    }

    #[test]
    fn broker_publish_then_latest_and_subscribe_get_newest() {
        let broker = FrameBroker::new();
        assert_eq!(broker.latest().seq, 0); // 初始占位帧 seq 0
        broker.publish(vec![1, 2, 3], 4, 4);
        broker.publish(vec![9, 9], 8, 8);
        let latest = broker.latest();
        assert_eq!((latest.seq, latest.jpeg.clone(), latest.width), (2, vec![9, 9], 8));
        // 新订阅者立即可读最新帧
        let rx = broker.subscribe();
        assert_eq!(rx.borrow().seq, 2);
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `P=/data/sunyunbo/miniconda3/envs/tauri-dev; PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH cargo test --manifest-path src-tauri/Cargo.toml frame_broker`
Expected: 编译失败(`sample_hash`/`mjpeg_part`/`FrameBroker` 未定义)。

- [ ] **Step 3: 实现 `frame_broker.rs`**(文件顶部,测试之前)

```rust
//! 帧 broker:一处编码、watch 扇出最新编码帧;采样 hash 做变化检测;MJPEG 分帧。
use std::sync::Arc;
use tokio::sync::watch;

/// 已编码的一帧(JPEG)。seq 单调递增,消费者据此判新。
#[derive(Debug)]
pub struct EncodedFrame {
    pub seq: u64,
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 对缓冲做采样 FNV-1a hash(变化检测用)。只取 ~1024 个等距采样点,
/// 静止时哪怕 WGC 高频来帧,这步也几乎零成本;幻灯片切页是大面积变化,采样必然命中。
pub fn sample_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let n = bytes.len();
    if n == 0 {
        return h;
    }
    let samples = 1024usize.min(n);
    let stride = (n / samples).max(1);
    let mut i = 0;
    while i < n {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x100000001b3);
        i += stride;
    }
    // 末字节兜底(stride 跨过尾部时仍纳入长度敏感)
    h ^= (n as u64).wrapping_mul(0x100000001b3);
    h
}

/// 把一段 JPEG 包成一个 multipart/x-mixed-replace 帧段。
pub fn mjpeg_part(jpeg: &[u8]) -> Vec<u8> {
    let header = format!(
        "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
        jpeg.len()
    );
    let mut out = Vec::with_capacity(header.len() + jpeg.len() + 2);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(jpeg);
    out.extend_from_slice(b"\r\n");
    out
}

/// watch 扇出最新编码帧。多消费者订阅;慢消费者自动只看最新(无积压)。
pub struct FrameBroker {
    tx: watch::Sender<Arc<EncodedFrame>>,
}

impl FrameBroker {
    pub fn new() -> Self {
        let (tx, _rx) = watch::channel(Arc::new(EncodedFrame {
            seq: 0,
            jpeg: Vec::new(),
            width: 0,
            height: 0,
        }));
        Self { tx }
    }

    /// 发布一帧(seq 自增)。无订阅者时 send 返回 Err,忽略即可。
    pub fn publish(&self, jpeg: Vec<u8>, width: u32, height: u32) {
        let seq = self.tx.borrow().seq.wrapping_add(1); // 临时 borrow 在本语句末即 drop,不与 send 争锁
        let _ = self.tx.send(Arc::new(EncodedFrame { seq, jpeg, width, height }));
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<EncodedFrame>> {
        self.tx.subscribe()
    }

    pub fn latest(&self) -> Arc<EncodedFrame> {
        self.tx.borrow().clone()
    }
}

impl Default for FrameBroker {
    fn default() -> Self {
        Self::new()
    }
}
```

在 `src-tauri/src/lib.rs` 模块声明区(`mod config;` 附近)加一行 `mod frame_broker;`。

- [ ] **Step 4: 跑测试确认通过**

Run: `... cargo test --manifest-path src-tauri/Cargo.toml frame_broker`
Expected: 3 测试 PASS。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/frame_broker.rs src-tauri/src/lib.rs
git commit -m "feat(capture): frame broker — watch fan-out, sample_hash change detect, mjpeg framing"
```

---

### Task 2: 拆出 `capture_window_raw`(原始 BGRA),编码复用

**Files:**
- Modify: `src-tauri/src/capture.rs`

把现 `capture_window`(BitBlt 直接出 JPEG)拆成"抓原始 BGRA 帧"+"复用 `encode_captured_frame`",供编码线程/兜底循环统一走 broker。

- [ ] **Step 1: 加纯函数测试**(`capture.rs` 的 `#[cfg(test)] mod tests` 内)

```rust
    #[test]
    fn raw_capture_unavailable_off_windows_is_err() {
        // 非 Windows 下 capture_window_raw 返回 Err(占位走 snapshot_jpeg 那条),不 panic
        let win = crate::app_state::TargetWindow {
            title: "x".into(), left: 0, top: 0, width: 0, height: 0, handle: 0,
        };
        assert!(capture_window_raw(&win).is_err());
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `... cargo test --manifest-path src-tauri/Cargo.toml raw_capture_unavailable`
Expected: 失败(`capture_window_raw` 未定义)。

- [ ] **Step 3: 实现拆分**

在 `capture.rs` 把 `#[cfg(windows)] fn capture_window` 的"抓像素"部分抽成新函数 `capture_window_raw`,返回 `CapturedFrame`;原 `capture_window` 改为调用它再编码:

```rust
/// 抓窗口当前矩形为原始 BGRA 帧(Windows:屏幕 DC BitBlt)。
#[cfg(windows)]
pub fn capture_window_raw(win: &TargetWindow) -> Result<CapturedFrame, CaptureError> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };
    let rect = crate::window_manager::window_rect(win.handle)
        .ok_or_else(|| CaptureError::Encode("窗口已不存在".into()))?;
    let (left, top, width, height) = (rect.left, rect.top, rect.width, rect.height);
    if width <= 0 || height <= 0 {
        return Err(CaptureError::Encode("窗口尺寸非法".into()));
    }
    unsafe {
        let screen_dc = GetDC(HWND::default());
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, width, height);
        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));
        let ok = BitBlt(mem_dc, 0, 0, width, height, screen_dc, left, top, SRCCOPY).is_ok();
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let scanlines = GetDIBits(
            mem_dc, bmp, 0, height as u32,
            Some(pixels.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS,
        );
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
    Err(CaptureError::Encode("非 Windows:无窗口截图".into()))
}
```

把现有 `#[cfg(windows)] fn capture_window` 改写为复用:

```rust
#[cfg(windows)]
fn capture_window(
    win: &TargetWindow, max_w: u32, max_h: u32, quality: u8,
) -> Result<Vec<u8>, CaptureError> {
    let frame = capture_window_raw(win)?;
    encode_captured_frame(&frame, max_w, max_h, quality)
}
```

(`#[cfg(not(windows))] fn capture_window` 保持原占位实现不变。)

- [ ] **Step 4: 跑测试 + 交叉编译**

Run: `... cargo test --manifest-path src-tauri/Cargo.toml`(既有 + 新测试全绿,27→28)
Run 交叉编译,Expected 0 error。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/capture.rs
git commit -m "refactor(capture): split capture_window_raw (BGRA) for broker reuse"
```

---

### Task 3: CaptureManager 接 broker + 编码线程 + WGC 信号 + BitBlt 兜底

**Files:**
- Modify: `src-tauri/src/capture.rs`(`CaptureManager`)
- Modify: `src-tauri/src/capture_session.rs`(Flags 携带 signal sender)

这是 Windows 重点任务,无单测;验收 = 交叉编译 0 error + 既有测试仍绿。

- [ ] **Step 1: `capture_session.rs` —— Flags 携带 signal,帧到达即通知**

把 `Handler` 的 Flags 从 `Arc<Mutex<Option<CapturedFrame>>>` 改为同时携带一个 `std::sync::mpsc::SyncSender<()>`:

```rust
use std::sync::mpsc::SyncSender;
// ...
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
        &mut self, frame: &mut WgcFrame, _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let (w, h) = (frame.width(), frame.height());
        let fb = frame.buffer()?;
        let bgra = fb.as_nopadding_buffer(&mut self.scratch);
        if let Ok(mut g) = self.latest.lock() {
            *g = Some(CapturedFrame { bgra: bgra.to_vec(), width: w, height: h });
        }
        let _ = self.signal.try_send(()); // 满则丢(合并),不阻塞捕获线程
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        if let Ok(mut g) = self.latest.lock() { *g = None; }
        Ok(())
    }
}
```

`start` 签名加 signal 参数,传入 Settings:

```rust
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
```

- [ ] **Step 2: `capture.rs` —— CaptureManager 持 broker + 编码线程**

`CaptureManager` 加字段并重写 `start`/`stop`。编码线程是一个 std thread:WGC 模式 block 在 signal 上,BitBlt 兜底模式定时循环。两种模式都做"采样 hash 去重 → 变化才编码 q82 → broker.publish"。

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use crate::frame_broker::{sample_hash, FrameBroker};

pub struct CaptureManager {
    latest: Arc<Mutex<Option<CapturedFrame>>>,
    broker: Arc<FrameBroker>,
    #[cfg(windows)]
    control: Mutex<Option<crate::capture_session::Control>>,
    stop_flag: Arc<AtomicBool>,
    encoder: Mutex<Option<std::thread::JoinHandle<()>>>,
    #[cfg(windows)]
    signal_tx: Mutex<Option<SyncSender<()>>>,
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
            #[cfg(windows)]
            signal_tx: Mutex::new(None),
        }
    }

    /// 主持人 MJPEG / 导播 Channel 订阅入口。
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Arc<crate::frame_broker::EncodedFrame>> {
        self.broker.subscribe()
    }
    pub fn latest_encoded(&self) -> Arc<crate::frame_broker::EncodedFrame> {
        self.broker.latest()
    }

    pub fn latest_frame(&self) -> Option<CapturedFrame> {
        self.latest.lock().ok().and_then(|g| g.clone())
    }
}
```

`start(hwnd)`:先 `stop()`,清缓冲,重置 stop_flag,起 WGC(成功→起 signal 驱动编码线程;失败→起 BitBlt 兜底线程):

```rust
    pub fn start(&self, hwnd: isize) {
        self.stop();
        if let Ok(mut g) = self.latest.lock() { *g = None; }
        self.stop_flag.store(false, Ordering::SeqCst);

        #[cfg(windows)]
        {
            let (sig_tx, sig_rx) = sync_channel::<()>(1);
            match crate::capture_session::start(hwnd, self.latest.clone(), sig_tx.clone()) {
                Ok(ctrl) => {
                    if let Ok(mut c) = self.control.lock() { *c = Some(ctrl); }
                    if let Ok(mut s) = self.signal_tx.lock() { *s = Some(sig_tx); }
                    let latest = self.latest.clone();
                    let broker = self.broker.clone();
                    let handle = std::thread::spawn(move || {
                        let mut last_hash = 0u64;
                        // sig_rx 在所有 sender drop(stop/WGC 关闭)后结束循环
                        for _ in sig_rx {
                            encode_changed(&latest, &broker, &mut last_hash);
                        }
                    });
                    if let Ok(mut e) = self.encoder.lock() { *e = Some(handle); }
                }
                Err(e) => {
                    eprintln!("[stagedeck] WGC 启动失败,回退 BitBlt 定时抓取:{e}");
                    self.start_bitblt_fallback(hwnd);
                }
            }
        }
        #[cfg(not(windows))]
        { let _ = hwnd; }
    }

    /// BitBlt 兜底:定时抓原始帧 → 去重 → 变化才编码 → publish。
    #[cfg(windows)]
    fn start_bitblt_fallback(&self, hwnd: isize) {
        let broker = self.broker.clone();
        let stop = self.stop_flag.clone();
        let handle = std::thread::spawn(move || {
            let mut last_hash = 0u64;
            let win = crate::app_state::TargetWindow {
                title: String::new(), left: 0, top: 0, width: 0, height: 0, handle: hwnd,
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
        if let Ok(mut e) = self.encoder.lock() { *e = Some(handle); }
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        #[cfg(windows)]
        {
            if let Ok(mut s) = self.signal_tx.lock() { *s = None; } // drop sender → WGC 编码线程退出
            if let Ok(mut c) = self.control.lock() {
                if let Some(ctrl) = c.take() { let _ = ctrl.stop(); } // 关 WGC → handler drop → 其 sender drop
            }
        }
        if let Ok(mut e) = self.encoder.lock() {
            if let Some(h) = e.take() { let _ = h.join(); }
        }
    }
}
```

加共享的"变化才编码"辅助(WGC 编码线程用;在锁内做廉价采样 hash,仅变化时才克隆 8MB 出来编码):

```rust
/// 锁内采样 hash 去重;变化才克隆缓冲、编码 q82、publish。
#[cfg(windows)]
fn encode_changed(
    latest: &Arc<Mutex<Option<CapturedFrame>>>,
    broker: &Arc<FrameBroker>,
    last_hash: &mut u64,
) {
    let changed = {
        let g = match latest.lock() { Ok(g) => g, Err(_) => return };
        match &*g {
            Some(f) => {
                let h = sample_hash(&f.bgra);
                if h != *last_hash { *last_hash = h; Some(f.clone()) } else { None }
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
```

(保留现有 `snapshot_jpeg` 不动 —— 它仍优先 `latest_frame()` 编码、否则 BitBlt、否则占位,作 `get_snapshot` IPC 与 `/api/snapshot.jpg` 的一次性兜底。)

- [ ] **Step 3: 交叉编译 + 原生测试**

Run 交叉编译 → Expected 0 error(这是本任务唯一权威验收:WGC 信号、线程、cfg 分支在 Windows 目标编过)。
Run 原生测试 → 既有测试仍绿(非 Windows 下 CaptureManager 走空分支)。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/capture.rs src-tauri/src/capture_session.rs
git commit -m "feat(capture): broker-fed encoder thread (WGC signal + BitBlt fallback), change-gated single encode q82"
```

---

### Task 4: 主持人 MJPEG 路由 `/api/stream.mjpg`

**Files:**
- Modify: `src-tauri/Cargo.toml`(加 `tokio-stream`)
- Modify: `src-tauri/src/routes.rs`

- [ ] **Step 1: 加依赖**

`src-tauri/Cargo.toml` `[dependencies]` 区加:

```toml
tokio-stream = { version = "0.1", features = ["sync"] }
```

- [ ] **Step 2: 实现路由**(`routes.rs`)

在 `router()` 的路由表加(`/api/snapshot.jpg` 那行附近):

```rust
        .route("/api/stream.mjpg", get(stream_mjpeg))
```

加处理函数(复用既有 `token_ok`、`TokenQuery`、`json_err`、`frame_broker::mjpeg_part`):

```rust
async fn stream_mjpeg(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TokenQuery>,
) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    use tokio_stream::StreamExt;
    let rx = state.capture.subscribe();
    // WatchStream 立即产出当前最新帧,之后每次变化产出一帧;跳过空占位(seq 0 / 空 jpeg)。
    let body = tokio_stream::wrappers::WatchStream::new(rx).filter_map(|frame| {
        if frame.jpeg.is_empty() {
            None
        } else {
            Some(Ok::<Vec<u8>, std::io::Error>(crate::frame_broker::mjpeg_part(&frame.jpeg)))
        }
    });
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "multipart/x-mixed-replace; boundary=frame",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::from_stream(body))
        .unwrap()
}
```

(确认 `routes.rs` 顶部已 `use axum::extract::Query;` 与 `use axum::http::header;`——若无则补;`header::CACHE_CONTROL` 在 `axum::http::header` 下。)

- [ ] **Step 3: 交叉编译 + 原生构建**

Run 交叉编译 → 0 error。
Run `... cargo test --manifest-path src-tauri/Cargo.toml`(含 Task 1 的 `mjpeg_part` 测试)→ 绿。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/routes.rs
git commit -m "feat(http): /api/stream.mjpg multipart stream fed by frame broker"
```

---

### Task 5: 导播 `subscribe_preview` 命令(Tauri Channel 推原始字节)

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`(注册)

- [ ] **Step 1: 实现命令**(`commands.rs` 文件末尾)

```rust
/// 导播 GUI:订阅实时预览帧。Rust 侧订阅 broker,每来新帧把原始 JPEG 字节推入 channel。
/// 前端 onmessage 收到 ArrayBuffer → createImageBitmap → 画 canvas。channel 关闭即结束。
#[tauri::command]
pub fn subscribe_preview(state: State<Arc<AppState>>, on_frame: tauri::ipc::Channel<tauri::ipc::Response>) {
    let mut rx = state.capture.subscribe();
    // 先推一帧当前最新(若有)。
    {
        let cur = state.capture.latest_encoded();
        if !cur.jpeg.is_empty() {
            let _ = on_frame.send(tauri::ipc::Response::new(cur.jpeg.clone()));
        }
    }
    tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let bytes = { rx.borrow().jpeg.clone() };
            if bytes.is_empty() {
                continue;
            }
            // 前端关闭 channel 后 send 返回 Err → 退出任务。
            if on_frame.send(tauri::ipc::Response::new(bytes)).is_err() {
                break;
            }
        }
    });
}
```

- [ ] **Step 2: 注册命令**(`lib.rs` 的 `generate_handler![ ... ]`,`commands::get_snapshot,` 之后)

```rust
            commands::subscribe_preview,
```

- [ ] **Step 3: 交叉编译**

Run 交叉编译 → 0 error。
**若** `tauri::ipc::Channel<tauri::ipc::Response>` 在本 tauri 2.x 不可编(类型签名不符):退化为"事件 + base64"备选——命令改为 `spawn` 里 `app.emit_to("main", "preview-frame", base64::engine::general_purpose::STANDARD.encode(&bytes))`(参照 lib.rs 现有 `emit_to("toast", ...)`),Task 6 前端改用 `event.listen("preview-frame", e => img.src = "data:image/jpeg;base64,"+e.payload)`。在 commit message 注明用了哪条路径。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(director): subscribe_preview command pushing broker frames over Tauri Channel"
```

---

### Task 6: 导播预览改 Channel → canvas

**Files:**
- Modify: `src/components/DirectorPreview.vue`

- [ ] **Step 1: 改 script** —— 删 400ms `get_snapshot` 轮询,改 Channel 订阅画 canvas

把 `<script setup>` 里帧相关部分(`frameSrc`/`loadFrame`/`frameTimer`)换成 Channel + canvas。保留 `pollState`(头条带 slide_state)不动:

```vue
<script setup lang="ts">
// 实时捕获预览:帧走 Tauri Channel(broker 推送,变化才来)→ canvas 双缓冲;头条带轮询 slide_state。
import { ref, onMounted, onUnmounted } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";

import { api, type SlideState } from "../utils/api";

const slide = ref<SlideState | null>(null);
const frameOk = ref(false);
const canvasEl = ref<HTMLCanvasElement | null>(null);
let stateTimer: number | undefined;
let channel: Channel<ArrayBuffer> | null = null;

async function pollState() {
  try {
    slide.value = await api.getSlideState();
  } catch {
    slide.value = null;
  }
}

async function drawFrame(buf: ArrayBuffer) {
  const canvas = canvasEl.value;
  if (!canvas) return;
  try {
    const bmp = await createImageBitmap(new Blob([buf], { type: "image/jpeg" }));
    canvas.width = bmp.width;
    canvas.height = bmp.height;
    canvas.getContext("2d")?.drawImage(bmp, 0, 0);
    bmp.close();
    frameOk.value = true;
  } catch {
    /* 单帧解码失败忽略,保留上一帧 */
  }
}

onMounted(() => {
  void pollState();
  stateTimer = window.setInterval(pollState, 1500);
  channel = new Channel<ArrayBuffer>();
  channel.onmessage = (buf) => void drawFrame(buf);
  void invoke("subscribe_preview", { onFrame: channel });
});
onUnmounted(() => {
  if (stateTimer) window.clearInterval(stateTimer);
  channel = null; // 释放引用;Rust 侧下次 send 失败即结束推送任务
});
</script>
```

模板里把 `<img v-show="frameOk" ...>` 换成 canvas:

```vue
    <div class="stage">
      <canvas v-show="frameOk" ref="canvasEl" class="frame"></canvas>
      <div class="placeholder" v-if="!frameOk">等待捕获 · 未选择 PPT 窗口</div>
    </div>
```

样式把 `.stage img` 改成 `.stage .frame`(同等 `max-width/height:100%; object-fit:contain` 等价用 `max-width:100%; max-height:100%;`),canvas 行为一致。

(若 Task 5 走了"事件+base64"备选:这里改 `import { listen } from "@tauri-apps/api/event";` + `onMounted` 里 `unlisten = await listen<string>("preview-frame", e => { img.src = "data:image/jpeg;base64," + e.payload; })`,模板保留 `<img>`,`onUnmounted` 调 `unlisten()`。)

- [ ] **Step 2: 前端构建**

Run `cd /data/sunyunbo/www/StageDeckRemote && pnpm build` → vue-tsc + vite 通过。

- [ ] **Step 3: Commit**

```bash
git add src/components/DirectorPreview.vue
git commit -m "feat(director): preview via Tauri Channel → canvas (drop 400ms snapshot poll)"
```

---

### Task 7: 主持人 SYNC 改 MJPEG `<img>`,删轮询链

**Files:**
- Modify: `src/presenter/presenter.js`

- [ ] **Step 1: 改 SYNC 帧链** —— 用一个长连 MJPEG `<img>`,删掉 `loadFrame` 轮询

把"snapshot chain"那段(`loadFrame` + `el.preview` 的 `load`/`error` 监听)替换为:进入 SYNC 设流 src,离开 SYNC 清空 src(关连接)。

替换 `// ── snapshot chain (SYNC) ──` 整段为:

```js
  // ── live stream (SYNC):MJPEG 长连,浏览器原生换帧、无闪烁、空闲零流量 ──
  function startStream() {
    if (state.tokenInvalid) return;
    el.preview.src = "/api/stream.mjpg?token=" + encodeURIComponent(token);
  }
  function stopStream() {
    el.preview.removeAttribute("src"); // 断开 MJPEG 连接
  }
  el.preview.addEventListener("error", function () {
    // 连接异常(如切走/网络):SYNC 下短延迟重连,其它模式不管
    if (state.mode === "sync" && !state.tokenInvalid) setTimeout(startStream, 1000);
  });
```

模式切换里(`if (mode === "sync") loadFrame();` 那处)改为:

```js
      if (mode === "sync") startStream();
      else stopStream();
```

并把 401 自愈相关:`handle401` 已置 `tokenInvalid`;在它(或 `setDown(true)` 处)附近确保流也停 —— 在 `setDown` 里当 `down===true` 时 `stopStream()`(避免 token 失效后 `<img>` 仍在重试)。找到 `function setDown(` 在其设 `tokenInvalid`/online 的分支末尾加 `if (down) stopStream();`。token 恢复后 `poll()` 成功分支会回到 SYNC 时由用户停留触发——稳妥起见在 `poll()` 成功且 `state.mode==='sync'` 且 `el.preview` 无 src 时 `startStream()`。

(若 `SNAPSHOT_MIN` 常量此后无引用,删除该 `var` 行避免无用变量。)

- [ ] **Step 2: 语法检查 + 前端构建**

Run `node --check src/presenter/presenter.js` → ok。
Run `pnpm build` → 通过(presenter.js 不被 vite 处理,但确保 director 端无破坏)。

- [ ] **Step 3: Commit**

```bash
git add src/presenter/presenter.js
git commit -m "feat(presenter): SYNC uses MJPEG <img> stream; drop loadFrame poll chain"
```

---

## Self-Review
- **Spec 覆盖:** §3 broker→Task 1+3;§4 帧来源(WGC 信号 + BitBlt 兜底)→Task 3;§5 MJPEG→Task 1(framing)+Task 4;§6 导播 Channel→Task 5+6;§7 画质 q82→Task 3(encode 调用);§9 测试→Task 1(broker/hash/mjpeg 纯逻辑)+各任务交叉编译;§10 影响文件全覆盖。
- **类型一致:** `EncodedFrame{seq,jpeg,width,height}`、`FrameBroker::{new,publish(jpeg,w,h),subscribe()->watch::Receiver<Arc<EncodedFrame>>,latest()->Arc<EncodedFrame>}`、`CaptureManager::{subscribe,latest_encoded,latest_frame}`、`sample_hash(&[u8])->u64`、`mjpeg_part(&[u8])->Vec<u8>`、`capture_window_raw(&TargetWindow)->Result<CapturedFrame,CaptureError>` 在 Task 1/2/3/4/5 间一致。
- **Placeholder:** 无;新文件给整文件,Windows 段给完整实现 + 交叉编译验收。
- **风险点(spec §11):** Tauri Channel 二进制路径不确定 → Task 5 Step 3 给了"事件+base64"确定备选 + Task 6 对应分支。WGC/MJPEG 端到端需真机;本机以交叉编译 0 error + 纯逻辑测试 + pnpm build 为验收。
- **顺序/依赖:** Task 1→(2,3)→(4,5)→(6,7);Task 3 依赖 1+2;4 依赖 1+3;5 依赖 3;6 依赖 5;7 依赖 4。线性执行无冲突。
