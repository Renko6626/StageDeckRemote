# 设计：WGC 精确窗口捕获（替换 BitBlt 屏幕区域截图）

- 状态：Approved（2026-06-15），待转 writing-plans
- 范围：把截图从"屏幕 DC BitBlt 窗口矩形"升级为 **Windows.Graphics.Capture（WGC）精确窗口捕获**，
  GPU 内容正确、与窗口位置/遮挡无关；BitBlt 保留为回退。
- 关联：[architecture.md §4.1](../../architecture.md)、[adr/0005](../../adr/0005-mvp-snapshot-polling.md)、
  上一增量 [2026-06-14-minimal-loop-design.md](2026-06-14-minimal-loop-design.md)
- **不在本范围**：操作/控制改 PowerPoint COM（另起 spec）。

---

## 1. 目标与动机

当前截图是"方案 B"：`GetDC(NULL)` + `BitBlt` 抠目标窗口所在的**屏幕矩形**，截的是"那个屏幕位置当前
显示的像素"。它对全屏放映够用，但：

- **依赖窗口位置、怕遮挡**：窗口被遮挡/移出屏幕/最小化就截错；导播也无法一边放 PPT 一边用屏幕。
- 是"屏幕区域"而非"窗口本身内容"。

GPU 加速渲染已是常态，`PrintWindow` 对 D3D 内容（PowerPoint 放映正是）易黑屏。**WGC 是面向 GPU
内容的正确精确捕获**：从 DWM 合成直接拿"这个窗口"的帧，位置/遮挡无关、GPU 内容正确。

**成功标准（Windows 实跑）：**
1. 选中窗口后，主持人端显示该**窗口本身**的真实内容。
2. **遮挡测试**：另一个窗口盖住目标窗口时，手机看到的仍是目标窗口内容（不是遮挡物）。
3. **移动测试**：拖动目标窗口，手机画面仍正确跟随其内容。
4. WGC 不可用（旧系统/初始化失败）时自动回退 BitBlt，不黑屏。

---

## 2. 关键决策

- **WGC 为主捕获**，支持 **Win10 1803+ / Win11**。
- **捕获边框**：`IsBorderRequired=false`——Win11 隐藏黄框；Win10 该设置 no-op，**接受边框**
  （已确认）。注意：边框是桌面覆盖层，**WGC 交付的帧本身不含边框**，手机快照永远干净。
- **光标**：`IsCursorCaptureEnabled=false`，快照不含鼠标（与现有 BitBlt 一致）。
- **实现用 `windows-capture` crate**（高层封装 WGC + D3D11 + 捕获线程），不手写 raw WinRT/D3D。
- **分层回退**：`WGC 最新帧 →（无会话/旧系统/窗口关/还没首帧）BitBlt 方案B →（都不行）占位图`。
  即保留现有 `capture_window`（BitBlt）作为回退。

---

## 3. 架构：从"按需拉"到"会话推 + 取最新帧"

```
选窗 commands::select_window(hwnd)
   └─ capture.start(hwnd)  → CaptureManager 起 WGC 会话（windows-capture 后台线程）
                               每帧: D3D 表面 → 紧排 BGRA → 覆盖写入 latest
                               latest: Arc<Mutex<Option<Frame>>>
GET /api/snapshot.jpg → snapshot_jpeg(state)
   └─ 读 latest:
        Some(frame) → bgra_to_rgb → encode_scaled_jpeg(≤1280×720, q70)
        None        → BitBlt 回退（用 selected_window 矩形）→ 占位图
```

要点：WGC 的高帧率只更新内存里的 `latest`；**JPEG 编码只在手机轮询时发生（~3fps）**，不随 WGC 帧率。
编码复用现有纯函数 `bgra_to_rgb` / `scaled_dims` / `encode_scaled_jpeg`（不改）。

---

## 4. 组件与接口

### 4.1 新增 `Frame` 与 `CaptureManager`（`capture.rs` / `capture/session.rs`）

```rust
/// 最新一帧（紧排 BGRA + 尺寸）。跨平台纯数据，Send+Sync。
pub struct Frame { pub bgra: Vec<u8>, pub width: u32, pub height: u32 }

pub struct CaptureManager {
    latest: Arc<Mutex<Option<Frame>>>,
    #[cfg(windows)]
    control: Mutex<Option<SessionControl>>, // windows-capture 的停控句柄
}

impl CaptureManager {
    pub fn new() -> Self;
    pub fn latest_frame(&self) -> Option<Frame>;   // 克隆出最新帧
    pub fn start(&self, hwnd: isize);              // 停旧会话 → 对新 hwnd 起会话
    pub fn stop(&self);
}
```

- `#[cfg(windows)]`：`start` 用 `windows-capture` 对 `Window::from_raw_hwnd(hwnd)` 起会话，
  设置 `ColorFormat=Bgra8`、`CursorCaptureSettings::WithoutCursor`、`DrawBorderSettings::WithoutBorder`；
  帧回调里取**紧排 BGRA**（用 crate 去 padding 的访问器，避免 row stride 错位）写入 `latest`。
- `#[cfg(not(windows))]`：`start`/`stop` 为空操作，`latest` 恒 `None` → snapshot 走占位图。

### 4.2 `AppState`（`app_state.rs`）
- 新增字段 `pub capture: CaptureManager`（与 `inner: Mutex<Inner>` 并列）。
- `Inner.selected_window` 保留（BitBlt 回退仍需其矩形）。

### 4.3 `snapshot_jpeg`（`capture.rs`）
- 改为：先 `state.capture.latest_frame()`；`Some` 则编码该帧；`None` 则走现有 BitBlt 路径
  （`selected_window` → `capture_window`），再不行占位图。

### 4.4 `commands.rs`
- `select_window` / `auto_detect_ppt`：设 `selected_window` 后调 `state.capture.start(handle)`。

### 4.5 `lib.rs`
- 退出 / 进程结束时 `state.capture.stop()`（可选，进程退出会自然回收线程）。

### 4.6 `Cargo.toml`
- `[target.'cfg(windows)'.dependencies]` 增 `windows-capture`（版本在 plan 里 pin）。

---

## 5. 错误处理与回退

| 情况 | 行为 |
| --- | --- |
| WGC 初始化失败 / Win10 太老不支持 | `start` 记日志、不 panic；`latest` 保持 `None` → snapshot 回退 BitBlt |
| 目标窗口关闭 | WGC 会话 `on_closed` → 清 `latest` → 回退 BitBlt（其 `window_rect` 也会失败）→ 占位图 |
| 还没收到首帧 | `latest=None` → 本次回退 BitBlt |
| 帧 stride ≠ width×4 | 用 crate 的去 padding 缓冲，保证紧排后再 `bgra_to_rgb` |

**手机端永远有画面**（WGC→BitBlt→占位三层）。

---

## 6. 测试

- **纯函数**：`bgra_to_rgb` / `scaled_dims` / `encode_scaled_jpeg` 已有单测，复用，继续绿
  （Linux `cargo test`）。
- **可测的薄逻辑**：`latest_frame` 的读写、回退选择（有帧用帧、无帧回退）可抽成小函数单测。
- **编译验证**：`cargo xwin build` 交叉编译，确认 `windows-capture` 接入与 `#[cfg(windows)]` 会话能编。
- **Windows 实跑**（出 debug 安装包，扩充手动清单）：
  1. 选窗 → 手机看到该**窗口内容**；
  2. **遮挡**目标窗口 → 手机仍是目标窗口内容；
  3. **移动**目标窗口 → 仍正确；
  4. 拔掉/旧系统模拟 WGC 失败 → 自动回退 BitBlt 不黑屏。

---

## 7. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| `windows-capture` 的停控句柄是否 `Send`/`Sync`（要进 `AppState`） | 实现时确认；不行则把会话控制放线程内、用一个 `Arc<AtomicBool>` 停止标志驱动 |
| WGC 帧 row stride padding | 用 crate 去 padding 访问器，统一紧排 BGRA |
| Win10 黄色捕获边框 | 已接受；帧本身不含边框，手机干净；OBS 建议窗口捕获 |
| `windows-capture` 传递依赖与我们 `windows`/`Cargo.lock` 版本 | plan 里 pin、cargo xwin build 验证；沿用已锁的 `alloc-no-stdlib` 修复 |
| WGC 需 Win10 1803+ | 运行时初始化失败即回退 BitBlt，无需编译期门控 |

---

## 8. 后续（不在本范围）

操作/控制改 **PowerPoint COM**（翻页/黑屏/读页码可靠化、action-setting 按钮语义点击；触发动画类
仍保留坐标 SendInput 兜底）—— 等本 spec 落地后另起 brainstorm。

---

## 关联
- 架构：[architecture.md](../../architecture.md) ｜ 决策：[adr/0005](../../adr/0005-mvp-snapshot-polling.md)（本 spec 落地后补 ADR-0006 记 WGC 决策）
- 上一增量：[2026-06-14-minimal-loop-design.md](2026-06-14-minimal-loop-design.md)
