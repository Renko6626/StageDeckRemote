# 设计：完整 COM 控制工具链 + 主持人 API + 黑白工业风前端

- 状态：Approved（2026-06-15），待转 writing-plans
- 范围：把 VBA spike 演进为成熟可用的 PowerPoint COM 控制工具链；通过 HTTP 暴露给主持人；
  前端重做为黑白工业风，导播端完整、主持人端两页（主控 + 兜底屏幕同步）。
- 前置（已证实）：spike 在 Windows 跑通——Rust 经 COM 连 PowerPoint、读 VBProject、`Application.Run` 都行。

---

## 1. 模板分析（grounds the design）

`quiz_template.pptm`（5 页 + VBA 工程）的典型答题脚本：**所有交互按钮都通过 ActionSettings 绑定 VBA 宏**
（`action="ppaction://macro?name=XXX"`）。实例：`Click_B1..T4`（方位选项点击）、`ToggleOption`×8
（`OptT_n`/`OptF_n` 判断题选项）、`CheckAnswer`、`BtnReset`/`ResetGame`、`Click_Back`。
**部分宏名带模块限定**（`模块2.Click_Back`）——所以必须**读绑定值、不能猜名字**：COM 的
`Shape.ActionSettings(ppMouseClick).Run` 返回 PowerPoint 实际使用的精确字符串，`Application.Run(它)` 必成功。

**结论**：按页 = 枚举当前页 Shapes → 读每个 `ActionSettings(ppMouseClick)` → `ppActionRunMacro` 的得
`{label, macro}` → 推给主持人 → 点击触发 `Application.Run(macro)`。零命名约定。

## 2. 控制优先级（与既有兜底并存）

- **主路（COM）**：精确翻页、读当前页、按页宏按钮、黑屏/恢复。
- **兜底（已实现）**：WGC/BitBlt 推截图 + 坐标 SendInput 点击（处理触发动画类、或 COM 不可用时）。
- 主持人端两页：**主控页**（COM 按钮）↔ **屏幕同步页**（截图+坐标点击）。

---

## 3. COM 工具链（`src-tauri/src/ppt_com.rs` 扩展）

复用 spike 的 `mod com`（connect / IDispatch invoke 助手）。新增助手：`get_str`（属性返回 BSTR）、
`set_i32`（PROPERTYPUT，用于 `View.State`）、`call_i32`/`call_void_args`。

### 数据结构（serde Serialize，跨 IPC/HTTP）
```rust
pub struct SlideAction { pub label: String, pub macro_name: String }
pub struct SlideState {
    pub available: bool,   // COM 连上 PowerPoint
    pub in_show: bool,     // 正在放映
    pub current: i32,      // 当前页号（1-based；未知=0）
    pub total: i32,
    pub actions: Vec<SlideAction>,  // 当前页的宏绑定按钮
}
```

### 公开入口（`#[cfg(windows)]` 真实 / 非 Windows 返回禁用态或 Err）
- `slide_state() -> Result<SlideState, String>`：
  - connect。`SlideShowWindows.Count >= 1` → `in_show=true`，`view = SlideShowWindows.Item(1).View`，
    `current = view.CurrentShowPosition`，`slide = view.Slide`；否则 `in_show=false`，
    `slide = Application.ActiveWindow.View.Slide`（编辑视图当前页），`current = slide.SlideIndex`。
    `total = ActivePresentation.Slides.Count`。
  - 枚举 `slide.Shapes`：对每个 shape，`ActionSettings(1 /*ppMouseClick*/).Action == 8 /*ppActionRunMacro*/`
    → `label = `（shape 的可见文字 `TextFrame.TextRange.Text`，空则 `shape.Name`），`macro_name = ActionSettings(1).Run`。
  - 连不上 PowerPoint → `Ok(SlideState{available:false,..})`（不报错，前端据此回退）。
- `trigger_action(macro_name: &str) -> Result<(), String>`：
  - 安全校验：重新枚举当前页 actions，确认 `macro_name` 在列表内（防止运行非当前页任意宏）→ `Application.Run(macro_name)`。
- `nav_next()/nav_prev()/nav_goto(n)`：`SlideShowWindows.Item(1).View.Next()/Previous()/GotoSlide(n)`（需在放映）。
- `set_black(on: bool)`：`view.State = if on {3 /*ppSlideShowBlackScreen*/} else {1 /*ppSlideShowRunning*/}`（PROPERTYPUT）。
- 保留 `list_macros`/`run_macro`（spike），但**主持人端不直接用 list_macros**（改用 slide_state）。

### 线程/COM
每个入口内 `ComGuard::init()`（已修复：仅成功时 CoUninitialize）。Axum 工作线程同样适用。

---

## 4. 主持人 HTTP API（`routes.rs`）

新增/升级（全部带 token；触发类走三层锁）：
| 方法 | 路径 | 说明 | 锁 |
| --- | --- | --- | --- |
| GET | `/api/slide_state` | `SlideState`（当前页 + 宏按钮） | token |
| POST | `/api/action` `{macro}` | 触发当前页某宏按钮 | `click_allowed`（三层锁全放行） |
| POST | `/api/next` `/prev` | 翻页：**优先 COM**，失败回退现有 SendInput | `button_allowed` |
| POST | `/api/black` `/restore` | **COM** `set_black`，失败回退（原 obs stub） | `button_allowed` |
| GET | `/api/snapshot.jpg` | 兜底屏幕同步（已实现 WGC/BitBlt） | token |
| POST | `/api/click` | 兜底坐标点击（已实现） | `click_allowed` |

`routes.rs` 直接调 `crate::ppt_com::*`（跨平台；非 Windows 返回禁用）。COM 调用失败 → 回退既有实现，
保证主持人永远有可用控制。

导播端 Tauri 命令同样加 `get_slide_state`（GUI 显示当前页/连接状态）。

---

## 5. 前端（黑白工业风，frontend-design 技能）

冷淡锐利的工业风，主题黑白（深黑底、白/灰文字、单一锐利强调，方正、等宽点缀、硬边框、无圆角滥用）。

### 5.1 导播端（app 服务端 GUI）完整功能
保留并重做：窗口选择、服务地址/二维码、控制状态、本地兜底控制、日志；**新增 COM 状态卡**
（PowerPoint 连接、当前页 current/total、in_show）。撤掉临时 `VbaSpikePanel`（其能力并入正式流程）。

### 5.2 主持人端两页 + 切换
顶部分段控件切换两页：
- **主控页（COM，主路）**：当前页指示（current/total）；**按页宏按钮区**（来自 `/api/slide_state` 的 actions，
  大按钮、标签即 shape 文字/名）；翻页（上一页/下一页）；黑屏/恢复；连接/锁状态。COM 不可用时提示并引导切到屏幕同步页。
- **屏幕同步页（兜底）**：现有截图预览 + 点哪点哪（坐标）+ 翻页 + 锁。即旧 presenter 能力。
- 两页都遵守三层锁与断线禁用；轮询 `/api/slide_state`（主控）/ `/api/snapshot.jpg`（同步）。

主持人端仍是 Axum 直接 serve 的原生页（保持零安装）；两页可用一个 HTML + JS 内切换（分段控件控制显隐）。

---

## 6. 错误处理与回退
- COM 任一步失败/未连 PowerPoint → 该能力回退（翻页→SendInput；黑屏→暂不可用提示；按钮区→空 + 提示切同步页）。
- `slide_state.available=false` 驱动主控页显示"COM 不可用，请用屏幕同步"。
- 安全：`/api/action` 只允许当前页已枚举的宏；token + 三层锁不变。

## 7. 测试
- **纯函数（Linux 单测）**：`parse_macro_names`（已有）；新增可抽的纯逻辑（如从 `ppaction://macro?name=` 解析宏名的 helper 若用到、SlideState 的序列化）。COM/IDispatch 走交叉编译 + Windows 实跑。
- **交叉编译**：`cargo xwin build` 全绿。
- **前端**：`pnpm typecheck` + `vite build`。
- **运行时（Windows）**：装 debug 包，放映 `quiz_template.pptm`，主控页应列出当前页按钮、点击触发、翻页/黑屏生效；切屏幕同步页截图+点击兜底可用。
- **最终**：开 subagent 真审一遍（架构一致性、COM 正确性、回退完整、安全、前端两页与锁）。

## 8. 风险
| 风险 | 缓解 |
| --- | --- |
| `View.State` PROPERTYPUT 的 IDispatch 命名参数（DISPID_PROPERTYPUT=-3） | 专门的 `set_i32` 助手按 IDispatch 规范实现；交叉编译验证 |
| 非放映态取"当前页"语义 | 用 `ActiveWindow.View.Slide`；未知则 current=0、actions 空、引导切同步页 |
| 触发动画类按钮（无 ActionSettings 宏） | 不在 actions 里出现；用屏幕同步页坐标点击兜底 |
| 主持人触发任意宏的安全 | `/api/action` 校验宏在当前页列表内 |
| COM 调用在 Axum 线程 | 每调用 `ComGuard` 初始化；调用短、不跨 await 持有 COM 指针 |

---

## 关联
- 模板：`quiz_template.pptm`（根目录）
- 复用：`ppt_com.rs`（spike）、WGC 捕获、SendInput 兜底
- 架构：[architecture.md](../../architecture.md) ｜ 协议：[protocol.md](../../protocol.md)（落地后更新端点）
