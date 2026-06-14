# 设计：最短闭环 —— 手机点击 → PPT 真实点击（含翻页）

- 状态：Approved（2026-06-14），待转 writing-plans
- 范围：实现 `window_manager` / `capture` / `input` 三个原生模块的 Windows 真实逻辑，
  打通 [`project.md` §19](../../project.md) 的最短闭环并加翻页。
- 关联：[architecture.md §4.2/§6](../../architecture.md)、[`project.md` §11.6–§11.8/§15/§18](../../project.md)

---

## 1. 目标与成功标准

**目标**：导播手动选定一个窗口后，主持人浏览器看到该窗口的**真实画面**，点击预览图能在
导播电脑上触发**真实鼠标点击**，并能上/下翻页。

**成功标准**（在 Windows 上验证）：
1. 导播端"枚举窗口"列出真实可选窗口。
2. 选中某窗口后，主持人端 `snapshot.jpg` 显示该窗口的真实内容（非占位图）。
3. 主持人点击预览图某处，导播电脑在对应位置发生真实左键单击（用可视目标验证，如记事本/画图按钮）。
4. 主持人"上一页/下一页"能驱动放映翻页。
5. 点击后鼠标光标回到原位。

**非目标 / 本轮不做**：`find_powerpoint` 自动识别、OBS 黑屏、WebSocket 推流、多控制端独占。

---

## 2. 约束

- 原生代码 **仅 Windows** 可编译运行；开发机为 Linux，**本机无法构建/运行 Win32**
  （见记忆 [`build-environment`]）。
- 验证方式：纯逻辑单测在任意平台 `cargo test`；Win32 部分由**用户在 Windows 上**
  `cargo test` + `pnpm tauri dev` 跑并回报，迭代修正。
- 因此架构上**把纯逻辑从 FFI 中抽离**，最大化可在本机测试的比例（见 §6）。

---

## 3. 关键设计决策

### 3.1 截图：截屏幕 DC 上窗口所在矩形（方案 B）

用 `BitBlt` 从**屏幕 DC** 取目标窗口当前**客户区屏幕矩形**的像素。

- 理由：放映窗口全屏、不被遮挡，截屏幕区域即"观众看到的画面"，最可靠；与点击坐标
  同为屏幕坐标系，天然一致；避开逐窗口 `PrintWindow` 对 GPU 合成内容的黑帧风险。
- 取舍：窗口被其他窗口遮挡时会截到遮挡物（放映场景不存在该问题）。
- 备选（已否决，留作后续升级）：`PrintWindow(PW_RENDERFULLCONTENT)`（可截后台窗口，
  但 D3D 放映可能黑帧）；`Windows.Graphics.Capture`（最稳但 WinRT+D3D，MVP 过重）。

### 3.2 实时窗口矩形（修正现有逻辑）

`routes.rs` 的 click 当前用**选窗时保存**的矩形。改为：**截图与点击都用 `window_rect(handle)`
实时读取**目标窗口的客户区屏幕矩形（`GetClientRect` + `ClientToScreen`）。这样：

- 截图区域 == 点击映射区域 == 观众所见，三者口径统一；
- 窗口移动/改变大小后坐标不串位；
- `selected_window` 仍存 `handle`+`title`，矩形以实时读取为准。

### 3.3 DPI 一致性

进程启动早期（`lib.rs::run()` 开头，`#[cfg(windows)]`）调用
`SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`，使 `GetWindowRect`/`GetClientRect`/
屏幕坐标/`SendInput` 统一为**物理像素**。运维侧仍建议显示缩放 100%（[`project.md` §18.1](../../project.md)）作双保险。

### 3.4 输入

- **点击**：`SendInput` 鼠标事件，使用绝对坐标
  `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | MOUSEEVENTF_MOVE`，把物理屏幕坐标换算为
  虚拟桌面 `0..=65535` 归一值后 move，再 `LEFTDOWN` + `LEFTUP`。多显示器更稳。
  点击前 `GetCursorPos` 存位，点击后**还原鼠标到原位**（减小光标出现在观众屏的概率，
  [`project.md` §18.3](../../project.md)；根治仍是 OBS 关捕获光标）。
- **翻页**：翻页前 `SetForegroundWindow(hwnd)` 激活放映窗口（[`project.md` §18.2](../../project.md)），
  再 `SendInput` 键盘事件 `VK_RIGHT`（下一页）/ `VK_LEFT`（上一页）。
  风险：`SetForegroundWindow` 受前台锁限制时可能不生效，备选 `PostMessage(WM_KEYDOWN/UP)`
  直接投递到窗口（见 §8 风险）。

---

## 4. 模块与接口变更

签名尽量保持现状（见各模块当前 stub），只填实现 + 抽纯函数。

### 4.1 `window_manager.rs`
- `list_windows() -> Vec<TargetWindow>`：`EnumWindows` 回调收集，过滤用纯函数
  `is_capturable_window`；`handle` 存 `HWND as isize`，矩形用 `GetClientRect`+`ClientToScreen`。
- `window_rect(handle: isize) -> Option<TargetWindow>`：实时读取客户区屏幕矩形。
- `window_exists(handle) -> bool`：`IsWindow`。
- `bring_to_front(handle)`：`SetForegroundWindow`。
- `find_powerpoint`：本轮保持 stub（返回 `None`）。
- **纯函数（跨平台、可测）**：
  - `is_capturable_window(title: &str, style: u32, exstyle: u32, width: i32, height: i32) -> bool`
  - `map_normalized_to_screen(left: i32, top: i32, width: i32, height: i32, x: f64, y: f64) -> (i32, i32)`

### 4.2 `capture.rs`
- `#[cfg(windows)] capture_window(win)`：`window_rect(win.handle)` 取实时矩形 → 屏幕 DC `BitBlt`
  → `GetDIBits` 取 BGRA → `bgra_to_rgb` → `encode_scaled_jpeg`。
- **纯函数（跨平台、可测）**：
  - `bgra_to_rgb(bgra: &[u8], w: u32, h: u32) -> Vec<u8>`
  - `encode_scaled_jpeg(rgb: &[u8], w: u32, h: u32, max_w: u32, max_h: u32, q: u8) -> Result<Vec<u8>, CaptureError>`
    （按比例缩到 ≤max，`image::imageops` resize，再 JPEG 编码；占位图也复用它）
- `snapshot_jpeg` 签名不变；未选窗口仍返回占位图。

### 4.3 `input.rs`
- `#[cfg(windows)] click_screen(x, y)`：存鼠标位 → 绝对坐标 `SendInput` move+down+up → 还原。
- `#[cfg(windows)] key_next/key_prev()`：`SendInput` 键盘 `VK_RIGHT`/`VK_LEFT`。
  激活窗口的职责归**调用方**：`commands`/`routes` 在翻页前先调
  `window_manager::bring_to_front(handle)`，`input` 只负责按键，职责更清。
- **纯函数（跨平台、可测）**：
  - `to_virtual_desktop_abs(x: i32, y: i32, vleft: i32, vtop: i32, vwidth: i32, vheight: i32) -> (i32, i32)`

### 4.4 `routes.rs` / `commands.rs`
- click 处理：改用 `window_rect(handle)` 实时矩形 + `map_normalized_to_screen` 映射（替换内联映射）。
- 翻页（`/api/next|prev` 与 `commands::local_next|prev`）：先 `bring_to_front(handle)` 再 `key_*`。

### 4.5 `lib.rs`
- 启动早期 `#[cfg(windows)]` 设 PMv2 DPI 感知。

### 4.6 `Cargo.toml`
- `windows` 增加 feature：`Win32_UI_HiDpi`（DPI）。其余（Foundation/WindowsAndMessaging/
  Input_KeyboardAndMouse/Graphics_Gdi）已具备。

---

## 5. 数据流（更新后）

点击：`POST /api/click {x,y}` → token + 范围 + 三层锁 → `window_rect(handle)` 实时矩形
→ `map_normalized_to_screen` → `to_virtual_desktop_abs` → `SendInput` 点击 → 还原鼠标 → `{ok,screen_x,screen_y}`。

截图：`GET /api/snapshot.jpg` → `window_rect` → `BitBlt` 屏幕区域 → `bgra_to_rgb`
→ `encode_scaled_jpeg(≤1280×720,q70)` → `image/jpeg`。

翻页：`POST /api/next` → token + `button_allowed` → `bring_to_front` → `key_next` → `{ok}`。

---

## 6. 可测性设计

把逻辑分两层：**纯函数（无 FFI，跨平台编译与测试）** + **Win32 薄壳（仅在 Windows 手测/集成）**。

| 纯函数 | 测什么 |
| --- | --- |
| `map_normalized_to_screen` | 角点/中心/越界裁剪、四舍五入 |
| `to_virtual_desktop_abs` | 单/多显示器、负坐标原点、边界 0 与 65535 |
| `is_capturable_window` | 空标题、工具窗、过小尺寸、正常窗 |
| `bgra_to_rgb` | 通道顺序、长度、stride |
| `encode_scaled_jpeg` | 缩放保持比例、输出可被 image 解码回预期尺寸 |

这些用 `#[cfg(test)]` 单测，在本机（Linux）即可红绿迭代。

---

## 7. 错误处理

沿用现有 `reason` 码（[`protocol.md` §3](../../protocol.md)），新增/明确：
- 截图：`BitBlt`/`GetDIBits` 失败 → `CaptureError`（500 `capture error`）。
- 点击：`SendInput` 返回注入数为 0 → `InputError::Failed` → `input_failed`(500)。
- 翻页：`SetForegroundWindow` 失败仅记日志、仍尝试发键（不阻断）；发键失败 → `input_failed`。
- `window_rect` 返回 `None`（窗口已关闭）→ click 走 `ppt_window_not_found`(409)。

---

## 8. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| `SetForegroundWindow` 受前台锁限制不激活 | 备选 `PostMessage` 投递按键；或文档要求先点一下放映窗 |
| `BitBlt` 对某些全屏独占放映拿到黑帧 | 放映用"窗口化全屏"而非独占；必要时升级 WGC（ADR 候选） |
| 鼠标还原仍可能闪现光标 | 运维 OBS 关捕获光标；点击瞬间最小化移动 |
| 多显示器坐标/DPI 不一致 | PMv2 + 绝对虚拟桌面坐标；纯函数覆盖多屏用例 |

---

## 9. 后续增量（不在本轮）

`find_powerpoint` 自动识别 → WGC 截图升级 → 翻页 `PostMessage` 兜底 → OBS 黑屏 → WS 推流。

---

## 关联

- 需求：[`project.md` §11/§15/§18/§19](../../project.md)
- 架构：[`architecture.md`](../../architecture.md) ｜ 协议：[`protocol.md`](../../protocol.md)
- 决策背景：[`adr/0004`](../../adr/0004-cfg-windows-native.md)（截图方式可后续补 ADR-0006）
