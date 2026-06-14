# 设计：VBA 可行性 spike（PowerPoint COM → 列宏 → Run）

- 状态：Approved（2026-06-15），待转 writing-plans
- 范围：**可行性证明**。证明 Rust 经 COM 连 PowerPoint、读 `VBProject`、列出宏名、`Application.Run` 一个宏这条路通，再决定是否投入完整 COM 工具链。
- 背景：控制的长期方向是"PowerPoint COM 工具链作为优雅主路，推截图 + 模拟点击作为兜底"。本 spike 验证其中风险最高的一环——VBA 宏发现与触发。**不在范围**：完整导航/黑屏、主持人端 UI、回退优先级集成、控制锁。

---

## 1. 目标与动机

当前控制 = WGC/BitBlt 推画面 + SendInput（键翻页、鼠标点击）。用户希望把这个降为"最不优雅的兜底"，
主路改为 PowerPoint COM；其中最亮点也最不确定的是：**若 PPT 的按钮都纪律性地绑定 VBA 宏，就能把宏名列表推给主持人，做成一排"宏触发按钮"**。

这条路的可行性取决于三件未证实的事：
1. 导播电脑能否开启并使用 **信任中心 → "信任对 VBA 工程对象模型的访问"**（否则 `VBProject` 访问抛错）。
2. Rust 能否经 **IDispatch 晚绑定** 读 VBProject 组件、抽宏名、并 `Application.Run`。
3. 目标 PPT 是否真的把按钮绑定到了命名宏（"纪律性"假设）。

**先用最小代价证实 (1)(2)，再投入完整实现。**

## 2. 成功标准（spike 的全部意义）

在 Windows 上、打开一个含命名公开 `Sub` 宏的 `.pptm`、**信任设置已开**：
1. 导播 GUI 点"探测 VBA 宏" → 列出该演示文稿的宏名。
2. 点列表里某个宏 → 该宏**真的执行**（可观察效果，如跳页/弹框）。
3. 信任设置未开 → 弹**清晰提示**，指明去开启那个设置。

## 3. 关键决策

- **COM 接入：raw IDispatch（用现有 `windows` crate）** —— `CoInitializeEx` → `CLSIDFromProgID("PowerPoint.Application")` → `GetActiveObject` 拿运行中的实例 → `IDispatch::GetIDsOfNames` + `Invoke` 逐层调用。无新依赖，控制力满。（备选：第三方 COM 封装 crate / 从类型库生成绑定 —— 对 spike 过度，否决。）
- **连已运行的实例**（`GetActiveObject`），匹配"PowerPoint 已开着放映"的真实场景；不新启实例。
- **作用对象 = `ActivePresentation`**（spike 不区分放映窗口）。
- **导播专用**：经 Tauri IPC 调用，不涉及主持人、不走三层锁。
- **临时塞进导播 GUI**：一个临时 `VbaSpikePanel`，证完即可移除/演进。
- **宏名抽取：解析 CodeModule 代码文本**（正则抽 `Sub`/`Function` 名），spike 够用；正式版改用 VBIDE 过程枚举更准。

## 4. 组件与接口

### 4.1 `src-tauri/src/ppt_com.rs`（新）
- `#[cfg(windows)]` 真实实现；`#[cfg(not(windows))]` 所有入口返回 `Err("仅 Windows 支持")`。
- 纯函数（跨平台、可单测）：
  - `parse_macro_names(module_text: &str) -> Vec<String>` —— 从一段 VBA 模块代码抽出 `Sub`/`Function` 名（忽略大小写、可带 `Public`/`Private`/前导空白；去重、保序）。
- `#[cfg(windows)]` FFI：
  - 内部 IDispatch 助手 `invoke(disp, name, args, kind) -> Result<VARIANT>`（`GetIDsOfNames` + `Invoke`，支持 method / propertyget）。
  - `connect() -> Result<IDispatch /*Application*/, String>`。
  - `list_macros() -> Result<Vec<String>, String>`：App → `ActivePresentation` → `VBProject` → `VBComponents`（遍历）→ 各 `CodeModule.Lines(1, CountOfLines)` → `parse_macro_names` 汇总去重。
  - `run_macro(name: &str) -> Result<(), String>`：`Application.Run(name)`。
  - 错误映射：把 VBProject 访问被拒（信任未开）映射为明确中文提示。

### 4.2 Tauri 命令（`commands.rs`，导播专用、不走锁）
- `#[tauri::command] pub fn vba_list_macros() -> Result<Vec<String>, String>` → `ppt_com::list_macros()`。
- `#[tauri::command] pub fn vba_run_macro(name: String) -> Result<(), String>` → `ppt_com::run_macro(&name)`。
- 在 `lib.rs` 的 `generate_handler!` 注册。

### 4.3 临时前端
- `src/components/VbaSpikePanel.vue`：按钮"探测 VBA 宏"→ `api.vbaListMacros()` → 渲染宏名列表；每个宏一个按钮 → `api.vbaRunMacro(name)`；显示错误（含信任提示）。
- 挂到 `views/DirectorView.vue`（临时）。
- `utils/api.ts` 加 `vbaListMacros()` / `vbaRunMacro(name)`。

### 4.4 `Cargo.toml`
- `windows` crate 增 COM/OLE/自动化相关 feature（如 `Win32_System_Com` / `Win32_System_Ole` / `Win32_System_Variant`；精确名实现时按编译器与 0.58 文档确定）。

## 5. 可测性

- **纯函数单测（Linux，tauri-dev env）**：`parse_macro_names` —— 覆盖 `Sub Foo()`、`Public Sub Bar`、`Private Function Baz`、缩进、大小写、重复名去重、空文本。
- **编译验证**：`cargo xwin build` 交叉编译，确认 IDispatch/COM 接入与 feature 能编。
- **运行时验证（Windows，你来）**：出 debug 安装包 → 开 `.pptm`（信任已开）→ 点按钮列宏、点宏触发。

## 6. 前置条件（你需要准备）

1. 导播电脑 PowerPoint：信任中心 → 宏设置 → 勾选 **"信任对 VBA 工程对象模型的访问"**。
2. 一个 `.pptm`，含几个公开命名 `Sub` 宏（最好有可观察效果，便于确认 Run 生效）。

## 7. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| COM 公寓/线程：Tauri 工作线程调用 | 每次命令调用内 `CoInitializeEx(APARTMENTTHREADED)` → 干活 → `CoUninitialize` |
| `GetActiveObject` 要求 PowerPoint 已运行 | 未运行时返回明确提示 |
| 信任设置未开 → VBProject 抛错 | 捕获并映射为"去开启信任设置"的中文提示（这本身也是 spike 要验证的反馈） |
| 宏名解析漏掉花式声明 | spike 够用；正式版改 VBIDE 过程枚举（`ProcOfLine` 等） |
| IDispatch 晚绑定啰嗦易错 | 集中在一个 `invoke` 助手；交叉编译 + 运行时双验证 |

## 8. 后续（不在本范围）

spike 通过后，演进为完整 COM 工具链：连接放映会话、导航（next/prev/goto）、黑屏/恢复、读页码状态；把宏列表推给主持人做"宏触发按钮"；并把 COM 设为主、截图+点击设为兜底的优先级集成。每步各自 brainstorm→spec→plan→实现。

---

## 关联
- 架构：[architecture.md](../../architecture.md) ｜ 需求：[project.md §6.3/§15](../../project.md)
- 兜底（已实现）：WGC 捕获 [2026-06-15-wgc-capture-design.md](2026-06-15-wgc-capture-design.md) + SendInput 点击/翻页
