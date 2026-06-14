# WGC 精确窗口捕获 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把截图从"屏幕 DC BitBlt 窗口矩形"升级为 Windows.Graphics.Capture（WGC）精确窗口捕获，GPU 内容正确、与位置/遮挡无关，BitBlt 保留为回退。

**Architecture:** WGC 在后台线程持续推帧，写入共享 `Arc<Mutex<Option<CapturedFrame>>>`；`snapshot_jpeg` 读最新帧、按需编码（复用现有纯函数），无帧时回退 BitBlt→占位图。WGC 会话由 `CaptureManager` 管理，随选窗启停，存于 `AppState`。

**Tech Stack:** Rust、`windows-capture` 2.0（WGC 高层封装，仅 `#[cfg(windows)]`）、image 0.25、现有 windows-rs 0.58 / Tauri v2。

> ⚠️ **构建/测试约束（见 `~/.claude/CLAUDE.md`）**：`src-tauri` 依赖 tauri，裸 Linux 编不了。
> - **纯函数任务（1–3）红绿**：用 conda `tauri-dev` 环境原生 `cargo test`：
>   ```bash
>   P=/data/sunyunbo/miniconda3/envs/tauri-dev
>   export PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH
>   cd src-tauri && cargo test capture::tests
>   ```
> - **FFI 任务（4–8）验证编译**：用 conda `xwin` + cargo-xwin 交叉编译到 Windows：
>   ```bash
>   cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
>     cargo xwin build --target x86_64-pc-windows-msvc
>   ```
> 下面命令省略环境前缀，按任务类型套用上面两段之一。运行时行为仍需 Windows 实跑（出 debug 安装包走手动清单）。

---

## 文件结构

| 文件 | 责任 | 改动 |
| --- | --- | --- |
| `src-tauri/src/capture.rs` | `CapturedFrame`、`encode_captured_frame`、`CaptureManager`、`snapshot_jpeg`（改）；现有纯函数 + BitBlt 不动 | 大改 |
| `src-tauri/src/capture_session.rs` | **新建**，`#[cfg(windows)]`：windows-capture 的 `Handler` + `start` + `Control` 类型别名 | 新建 |
| `src-tauri/src/app_state.rs` | `AppState` 加 `capture: CaptureManager` 字段 | 小改 |
| `src-tauri/src/commands.rs` | `select_window`/`auto_detect_ppt` 选窗后启动会话 | 小改 |
| `src-tauri/src/lib.rs` | `#[cfg(windows)] mod capture_session;` | 小改 |
| `src-tauri/Cargo.toml` | windows 目标加 `windows-capture` | 小改 |

---

## Task 1: `CapturedFrame` 类型 + `encode_captured_frame` 纯函数（capture.rs）

**Files:**
- Modify: `src-tauri/src/capture.rs`

- [ ] **Step 1: 写失败测试**

在 `capture.rs` 的 `#[cfg(test)] mod tests` 里追加：

```rust
    #[test]
    fn encodes_captured_frame_to_decodable_jpeg() {
        // 2x2 BGRA：全红 (B=0,G=0,R=255,A=255)
        let frame = CapturedFrame { bgra: vec![0u8, 0, 255, 255].repeat(4), width: 2, height: 2 };
        let jpeg = encode_captured_frame(&frame, 1280, 720, 80).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
    }
```

- [ ] **Step 2: 跑，确认失败**

Run（tauri-dev 环境）: `cd src-tauri && cargo test capture::tests::encodes_captured_frame`
Expected: 编译失败 `cannot find type CapturedFrame` / `function encode_captured_frame`。

- [ ] **Step 3: 写实现**

在 `capture.rs` 顶层（`use ...` 之后、`bgra_to_rgb` 附近）加：

```rust
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
```

- [ ] **Step 4: 跑，确认通过**

Run: `cd src-tauri && cargo test capture::tests::encodes_captured_frame`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/capture.rs
git commit -m "feat(capture): CapturedFrame + encode_captured_frame with test"
```

---

## Task 2: `CaptureManager` 骨架（跨平台，capture.rs）

**Files:**
- Modify: `src-tauri/src/capture.rs`

- [ ] **Step 1: 写失败测试**

在 `mod tests` 里追加：

```rust
    #[test]
    fn capture_manager_latest_frame_roundtrip() {
        let mgr = CaptureManager::new();
        assert!(mgr.latest_frame().is_none());
        // 直接写入共享缓冲（测试在同模块，可访问私有字段）
        *mgr.latest.lock().unwrap() = Some(CapturedFrame { bgra: vec![1, 2, 3, 4], width: 1, height: 1 });
        let f = mgr.latest_frame().unwrap();
        assert_eq!((f.width, f.height, f.bgra.len()), (1, 1, 4));
        mgr.stop(); // 非 Windows 下应为空操作、不 panic
    }
```

- [ ] **Step 2: 跑，确认失败**

Run: `cd src-tauri && cargo test capture::tests::capture_manager_latest`
Expected: 编译失败 `cannot find type CaptureManager`。

- [ ] **Step 3: 写实现**

在 `capture.rs` 顶层加（`use std::sync::{Arc, Mutex};` 若未引入则加到文件顶部 use 区）：

```rust
use std::sync::{Arc, Mutex};

/// 管理 WGC 后台捕获会话与最新帧缓冲。存于 AppState，被 snapshot 读、被选窗启停。
pub struct CaptureManager {
    /// 后台会话写入的最新一帧；snapshot 读取。
    latest: Arc<Mutex<Option<CapturedFrame>>>,
}

impl CaptureManager {
    pub fn new() -> Self {
        Self { latest: Arc::new(Mutex::new(None)) }
    }

    /// 取最新帧的克隆（无则 None → snapshot 回退）。
    pub fn latest_frame(&self) -> Option<CapturedFrame> {
        self.latest.lock().ok().and_then(|g| g.clone())
    }

    /// 对指定窗口启动捕获会话（停旧、清最新帧）。WGC 接入在 Task 6 填充。
    pub fn start(&self, hwnd: isize) {
        self.stop();
        if let Ok(mut g) = self.latest.lock() {
            *g = None;
        }
        let _ = hwnd;
    }

    /// 停止当前会话。WGC 接入在 Task 6 填充。
    pub fn stop(&self) {}
}

impl Default for CaptureManager {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: 跑，确认通过**

Run: `cd src-tauri && cargo test capture::tests`
Expected: PASS（含 Task 1 用例）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/capture.rs
git commit -m "feat(capture): CaptureManager skeleton (latest-frame buffer) with test"
```

---

## Task 3: 接入 `AppState` + `snapshot_jpeg` 先读 WGC 帧再回退（app_state.rs, capture.rs）

**Files:**
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/capture.rs`

- [ ] **Step 1: 写失败测试**

在 `capture.rs` 的 `mod tests` 里追加（无帧 + 无窗口 → 占位图，可在 Linux 跑）：

```rust
    #[test]
    fn snapshot_falls_back_to_placeholder_when_empty() {
        let state = crate::app_state::AppState::new("tok".into(), 8787);
        // 无 WGC 帧、无 selected_window → 占位图（合法 JPEG）
        let jpeg = snapshot_jpeg(&state, 1280, 720, 70).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert!(decoded.width() >= 16 && decoded.height() >= 16);
    }
```

- [ ] **Step 2: 跑，确认失败**

Run: `cd src-tauri && cargo test capture::tests::snapshot_falls_back`
Expected: 失败 —— `AppState` 暂无 `capture` 字段 / `snapshot_jpeg` 仍按旧逻辑（编译错误或行为不符）。

- [ ] **Step 3: 改 `AppState` 加 `capture` 字段**

`app_state.rs`：在 `AppState` 结构体加字段，并在 `new` 构造。

结构体改为：

```rust
pub struct AppState {
    pub inner: Mutex<Inner>,
    pub capture: crate::capture::CaptureManager,
}
```

`AppState::new` 改为：

```rust
    pub fn new(token: String, server_port: u16) -> Self {
        AppState {
            inner: Mutex::new(Inner {
                selected_window: None,
                remote_enabled: false,
                control_locked: true,
                presenter_unlocked: false,
                active_client_id: None,
                token,
                server_port,
                logs: Vec::new(),
            }),
            capture: crate::capture::CaptureManager::new(),
        }
    }
```

- [ ] **Step 4: 改 `snapshot_jpeg` 先读最新帧再回退**

`capture.rs`：把 `snapshot_jpeg` 整个替换为：

```rust
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
```

- [ ] **Step 5: 跑，确认通过**

Run: `cd src-tauri && cargo test capture::tests`
Expected: PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/app_state.rs src-tauri/src/capture.rs
git commit -m "feat(capture): AppState.capture; snapshot reads WGC frame then falls back"
```

---

## Task 4: `Cargo.toml` 加 `windows-capture`（windows 目标）

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 加依赖**

在 `[target.'cfg(windows)'.dependencies]` 里、`windows = { ... }` 之后加一行：

```toml
windows-capture = "2"
```

- [ ] **Step 2: 交叉编译验证可解析/可编译**

Run（xwin 环境）: `cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
Expected: 编译通过（windows-capture 及其 windows 0.62 传递依赖被拉取编译；尚未使用）。
（若 `alloc-no-stdlib` 重新分裂报 brotli 错，重做 `cargo update -p alloc-no-stdlib@3.0.0 --precise 2.0.4`。）

- [ ] **Step 3: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build(deps): add windows-capture (WGC) for windows target"
```

---

## Task 5: WGC 会话模块 `capture_session.rs`（`#[cfg(windows)]`）

**Files:**
- Create: `src-tauri/src/capture_session.rs`
- Modify: `src-tauri/src/lib.rs`

> FFI：无法纯 TDD。交叉编译验证；运行时在 Windows 验。API 依据 windows-capture 2.0 源码。

- [ ] **Step 1: 新建 `capture_session.rs`**

```rust
//! WGC 后台捕获会话（仅 Windows）。把每帧紧排 BGRA 写入共享最新帧缓冲。
//! 基于 windows-capture 2.0 的 GraphicsCaptureApiHandler。

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

/// 停控句柄类型别名（Send+Sync：内部为 JoinHandle + Arc）。供 CaptureManager 持有。
pub type Control = CaptureControl<Handler, WgcError>;

/// 捕获事件处理器：每帧写入共享缓冲。Flags 携带共享缓冲进来。
pub struct Handler {
    latest: Arc<Mutex<Option<CapturedFrame>>>,
    scratch: Vec<u8>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = Arc<Mutex<Option<CapturedFrame>>>;
    type Error = WgcError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { latest: ctx.flags, scratch: Vec::new() })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut WgcFrame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let (w, h) = (frame.width(), frame.height());
        let mut fb = frame.buffer()?;
        // 去掉行 padding，得到紧排 BGRA（ColorFormat::Bgra8）。
        let bgra = fb.as_nopadding_buffer(&mut self.scratch);
        if let Ok(mut g) = self.latest.lock() {
            *g = Some(CapturedFrame { bgra: bgra.to_vec(), width: w, height: h });
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        if let Ok(mut g) = self.latest.lock() {
            *g = None;
        }
        Ok(())
    }
}

/// 对指定 HWND 起一个后台 WGC 会话；帧写入 `latest`。返回停控句柄。
pub fn start(hwnd: isize, latest: Arc<Mutex<Option<CapturedFrame>>>) -> Result<Control, String> {
    let window = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);
    let settings = Settings::new(
        window,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder, // Win11 隐藏黄框；Win10 no-op（接受边框）
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        latest,
    );
    Handler::start_free_threaded(settings).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: 在 `lib.rs` 声明模块**

在 `lib.rs` 的 `mod` 区加：

```rust
#[cfg(windows)]
mod capture_session;
```

- [ ] **Step 3: 交叉编译验证**

Run: `cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
Expected: 编译通过（如 windows-capture 个别 API 名有出入，按编译器提示对照 2.0 源码修正）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/capture_session.rs src-tauri/src/lib.rs
git commit -m "feat(capture): WGC capture session handler (windows)"
```

---

## Task 6: `CaptureManager` 接入 WGC 启停（capture.rs）

**Files:**
- Modify: `src-tauri/src/capture.rs`

- [ ] **Step 1: 给 `CaptureManager` 加会话控制字段**

把结构体改为（加 `#[cfg(windows)]` 控制字段）：

```rust
pub struct CaptureManager {
    latest: Arc<Mutex<Option<CapturedFrame>>>,
    #[cfg(windows)]
    control: Mutex<Option<crate::capture_session::Control>>,
}
```

`new` 改为：

```rust
    pub fn new() -> Self {
        Self {
            latest: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            control: Mutex::new(None),
        }
    }
```

- [ ] **Step 2: `start`/`stop` 接 WGC**

把 `start` / `stop` 改为：

```rust
    pub fn start(&self, hwnd: isize) {
        self.stop();
        if let Ok(mut g) = self.latest.lock() {
            *g = None;
        }
        #[cfg(windows)]
        {
            match crate::capture_session::start(hwnd, self.latest.clone()) {
                Ok(ctrl) => {
                    if let Ok(mut c) = self.control.lock() {
                        *c = Some(ctrl);
                    }
                }
                // 失败（旧系统/WGC 不可用）→ 不存句柄 → snapshot 自动回退 BitBlt。
                Err(e) => eprintln!("[stagedeck] WGC 启动失败，回退 BitBlt：{e}"),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = hwnd;
        }
    }

    pub fn stop(&self) {
        #[cfg(windows)]
        {
            if let Ok(mut c) = self.control.lock() {
                if let Some(ctrl) = c.take() {
                    let _ = ctrl.stop();
                }
            }
        }
    }
```

- [ ] **Step 3: Linux 编译 + 交叉编译双验证**

Run（tauri-dev）: `cd src-tauri && cargo test capture::tests`
Expected: PASS（非 Windows 分支不变，测试照过）。
Run（xwin）: `cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
Expected: 编译通过（Windows 分支接入 WGC）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/capture.rs
git commit -m "feat(capture): CaptureManager starts/stops WGC session (windows)"
```

---

## Task 7: 选窗时启动会话（commands.rs）

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: `select_window` 启动会话**

`select_window` 改为（设窗口后启动捕获）：

```rust
#[tauri::command]
pub fn select_window(state: State<Arc<AppState>>, window: TargetWindow) {
    let title = window.title.clone();
    let handle = window.handle;
    if let Ok(mut i) = state.inner.lock() {
        i.selected_window = Some(window);
    }
    state.capture.start(handle);
    log::event(&state, format!("已选择窗口：{title}"));
}
```

- [ ] **Step 2: `auto_detect_ppt` 启动会话**

`auto_detect_ppt` 改为（识别到后启动捕获）：

```rust
#[tauri::command]
pub fn auto_detect_ppt(state: State<Arc<AppState>>) -> Option<TargetWindow> {
    let found = window_manager::find_powerpoint();
    if let Some(win) = found.clone() {
        let handle = win.handle;
        if let Ok(mut i) = state.inner.lock() {
            i.selected_window = Some(win);
        }
        state.capture.start(handle);
        log::event(&state, "自动识别到 PowerPoint 放映窗口");
    }
    found
}
```

- [ ] **Step 3: 交叉编译验证**

Run: `cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
Expected: 编译通过。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(commands): start WGC capture session on window select"
```

---

## Task 8: 全量交叉编译 + 手动验收清单（occlusion/move）

**Files:**
- Create: `docs/superpowers/plans/2026-06-15-wgc-capture-manual-checklist.md`

- [ ] **Step 1: 全量交叉编译 + Linux 测试双绿**

Run（xwin）: `cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
Expected: 0 errors。
Run（tauri-dev）: `cd src-tauri && cargo test capture::tests`
Expected: 全绿。

- [ ] **Step 2: 写手动验收清单**

创建 `docs/superpowers/plans/2026-06-15-wgc-capture-manual-checklist.md`：

```markdown
# WGC 捕获 · Windows 手动验收

前置：出 debug 安装包并安装：
`tauri build --debug --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis`

1. 选窗（记事本/画图/PowerPoint 放映）→ 手机看到该**窗口本身**的真实内容（非占位、非黑屏）。
2. **遮挡测试**：用另一个窗口盖住目标窗口 → 手机仍显示**目标窗口内容**（不是遮挡物）。
3. **移动测试**：拖动 / 改变目标窗口大小 → 手机画面仍正确跟随其内容。
4. **GPU 内容**：PowerPoint 放映（D3D 加速）→ 手机看到真实幻灯片（非黑帧）。
5. **回退**：选一个 WGC 无法捕获的目标 / 旧系统 → 自动回退 BitBlt，不黑屏。
6. Win11 上目标窗口**无黄色捕获边框**；Win10 上有边框但**手机快照里无边框**。

任一不过：记录现象。WGC 起不来 → 看日志 `WGC 启动失败`，确认回退到 BitBlt；
帧错位/花屏 → 检查 `as_nopadding_buffer` 紧排与 `ColorFormat::Bgra8`。
```

- [ ] **Step 3: 提交**

```bash
git add docs/superpowers/plans/2026-06-15-wgc-capture-manual-checklist.md
git commit -m "test: WGC manual acceptance checklist (occlusion/move/GPU/fallback)"
```

---

## Self-Review

- **Spec 覆盖**：会话推+取最新帧（Task 2/3/6）、windows-capture（Task 4/5）、无边框/无光标（Task 5 settings）、
  分层回退 WGC→BitBlt→占位（Task 3 snapshot_jpeg）、选窗启停（Task 7）、紧排 BGRA（Task 5 `as_nopadding_buffer`）、
  AppState 持有（Task 3）、测试（纯函数 Task 1/3 + 交叉编译 Task 4-8 + 手动清单 Task 8）—— 均覆盖。
  spec 风险 #1（控制句柄 Send）已由源码确认 `CaptureControl` 为 Send+Sync，故直接进 `AppState`。
- **占位扫描**：无 TBD/TODO 式占位；每步给完整代码。FFI 任务标注"按编译器提示对照 2.0 源码修正"，属约束。
- **类型一致**：`CapturedFrame`{bgra,width,height} 在 capture.rs 定义、capture_session.rs 与测试一致使用；
  `CaptureManager`{latest, (windows)control} 字段在 Task 2 建、Task 6 加 control；`encode_captured_frame`/
  `latest_frame`/`start`/`stop` 签名前后一致；`Control = CaptureControl<Handler, WgcError>` 在 session 定义、
  manager 引用一致。
