# 快捷预制消息 + 导播软弹窗(Quick Messages)设计

**日期:** 2026-06-16
**状态:** 待审阅
**范围:** 主持人→导播的预制消息;导播端置顶软弹窗;导播可编辑预制并同步。导播→主持人**本期不实装**(仅留模型钩子)。

---

## 1. 目标

主持人(手机)点击预制消息按钮发给导播,实现"最低限度交流"。导播即使**最小化**也能通过**置顶软弹窗**(不抢焦点、约 6 秒自关)明显看到。预制消息由**导播编辑**、**同步给主持人**。消息模型设计成**通用双向**(带 `from` 方向),为导播→主持人留钩子。

## 2. 数据模型

- `PresetMessage { id: String, text: String }` —— 预制消息。导播拥有、可编辑;持久化在应用配置目录 `presets.json`;首次缺失则种子默认若干条(如:请加快节奏 / 请放慢 / 时间到 / 下一位准备 / 麦克风没声 / 收到 OK)。`id` 由内容稳定派生或递增,服务端生成,客户端只回传 `id`。
- `HostMessage { from: String, text: String, at_ms: u64 }` —— 一条已收到的消息。本期 `from` 恒为 `"presenter"`;字段保留以备导播→主持人。

## 3. 同步与发送(HTTP,主持人面)

两个**新端点**,均**仅校验 token、不受三层锁限制**(通信不应被控制锁挡):

- `GET /api/messages`(token)→ 返回当前预制列表 `[{id,text}]`。主持人 MSG 页据此渲染按钮。
- `POST /api/message`(token,体 `{ id }`)→ 服务端**校验 id 属于预制列表**(主持人只能发允许的预制,发不了任意文本);命中则构造 `HostMessage` 推入消息通道 + 记日志(`log::event`),返回 `{ok:true}`;id 非法返回 400。

## 4. 导播接收 + 软弹窗(核心)

- **通道**:`AppState` 持 `tokio::sync::mpsc::UnboundedSender<HostMessage>`(在 `lib.rs` 建通道、`tx` 交给 `AppState`,`rx` 移入 setup 任务)。`routes.rs` 收到消息即 `tx.send(..)`——routes **不依赖 tauri**,只发通道。
- **接收任务**(`lib.rs` setup,持 `AppHandle`):从 `rx` 取消息 →
  1. 取 `toast` 窗口,按主屏宽度算**顶部居中**位置(`set_position`),`show()`,并 `emit("host-message", {text, from, at_ms})` 给该窗 webview;
  2. 对主窗口 `request_user_attention(Informational)`(任务栏闪烁,不抢焦点)。
- **toast 窗口**(`tauri.conf.json` 新增,启动即存在但隐藏):`label:"toast"`、`url:"index.html#/toast"`、约 `380×120`、`visible:false`、`decorations:false`、`alwaysOnTop:true`、`skipTaskbar:true`、`focus:false`、`resizable:false`。
- **toast 前端**(`ToastView.vue`,路由 `/toast`,bare 布局):收到 `host-message` 事件 → 显示大字消息 + 「主持人·HH:MM」+ 6 秒倒计时进度条 + ✕;**6 秒后自身 `hide()`**(`getCurrentWindow().hide()`);点 ✕ 立即 hide;点正文 → 调出主窗口(`get主窗.unminimize()+set_focus()`)并 hide 自身;**可选轻提示音**(WebAudio 生成短音,读 `localStorage['sd.dir.toastChime']`,默认开)。连发多条:替换为最新、重置计时,显示小计数 `+N`。
- **最小化也可见**:toast 是独立顶层窗,与主窗最小化状态无关。

> **运行时风险(须用户 Windows 实测):** "显示但绝不抢焦点"在 Windows 有平台细节。先用 `focus:false`+`always_on_top`+`skip_taskbar`+无边框 + 仅 `show()`(不 `set_focus`)实现;若实测仍抢焦点,后续以 `ShowWindow(SW_SHOWNOACTIVATE)` / `WS_EX_NOACTIVATE`(`#[cfg(windows)]` 窗口句柄微调)兜底。本期先做到非兜底版,留待运行时定夺。

## 5. 导播编辑(IPC + 持久化)

- IPC 命令:
  - `get_presets() -> Vec<PresetMessage>`;
  - `set_presets(presets: Vec<PresetMessage>)` → 更新 `AppState.presets` + 持久化(`presets.rs` 写应用配置目录;路径经命令的 `AppHandle` 解析)。主持人下次 `GET /api/messages` 即取到新列表。
- **导播 GUI**:导播栏新增「快捷消息」卡(`DirectorMessages.vue`):列出预制;「编辑」进入编辑态(逐行改文本、增/删、保存调 `set_presets`);含**提示音开关**(写 `localStorage['sd.dir.toastChime']`)。收到的消息走现有**日志**面板(已 `log::event`)。
- 启动加载:`lib.rs` setup 用 `AppHandle` 解析配置目录,`presets.rs::load` 读入种子/已存预制写进 `AppState`。

## 6. 主持人 UI(`presenter.*`)

分段切换由两路扩为三路 **主控 / 屏幕 / 消息(MSG)**;MSG 页是预制消息**双列大按钮**网格(进入 MSG 页/挂载时 `GET /api/messages` 拉取并渲染);点击 → `POST /api/message {id}` → 成功弹「已发送 ✓」、失败「发送失败」。沿用现有黑白工业风与门控样式;消息发送**不要求 ARM**(通信)。

## 7. 路由 / 外壳

- `router.ts`:新增 `{ path:"/toast", component: ToastView, meta:{ bare:true } }`(hash history,toast 窗加载 `index.html#/toast`)。
- `App.vue`:`route.meta.bare` 时只渲染 `<router-view/>`(无顶栏、无网格背景),否则现有外壳。

## 8. 影响文件

| 文件 | 改动 |
|---|---|
| `src-tauri/src/app_state.rs` | `Inner.presets` + `AppState.message_tx`;`PresetMessage`/`HostMessage` 类型;`new` 签名加 `tx` |
| `src-tauri/src/presets.rs` | **新增**:预制列表 load/save(配置目录)+ 默认种子 |
| `src-tauri/src/routes.rs` | `GET /api/messages`、`POST /api/message`(仅 token) |
| `src-tauri/src/commands.rs` | `get_presets` / `set_presets`(持久化) |
| `src-tauri/src/lib.rs` | 建通道、加载预制、注册命令、配置 toast 窗、spawn 接收任务(显示 toast + 任务栏闪) |
| `src-tauri/tauri.conf.json` | 新增 `toast` 窗口 |
| `src/views/ToastView.vue` | **新增**:软弹窗视图(事件→显示→倒计时→自关) |
| `src/components/DirectorMessages.vue` | **新增**:预制编辑卡 + 提示音开关 |
| `src/views/DirectorView.vue` | 栏内挂载 `DirectorMessages` |
| `src/router.ts` / `src/App.vue` | `/toast` 路由 + bare 外壳 |
| `src/utils/api.ts` | `PresetMessage` 类型 + `getPresets`/`setPresets` |
| `src/presenter/presenter.{html,css,js}` | 第三页 MSG + 拉取/发送 |

## 9. 测试

- **Rust 纯逻辑(Linux):** `presets.rs` 默认种子 + id 派生 + "id 属于列表" 校验抽成纯函数并单测;既有测试保持绿。
- **交叉编译:** `cargo xwin build` 0 错误(验证 tauri 窗口 API / 通道 / 任务栏闪 FFI 编译)。
- **前端构建:** `pnpm build`(vue-tsc + vite,含新组件/路由)。
- **运行时(用户 Windows):** 主持人 MSG 页点预制 → 导播顶部居中软弹窗出现(主程序最小化也出现)、6 秒自关、**确认不抢焦点**(关键验证项)、任务栏闪;导播编辑预制 → 主持人重新拉取生效;提示音开关有效。

## 10. 不做(YAGNI / 已知局限)
- 导播→主持人**不实装**(仅 `from` 钩子)。
- 不引入第三方通知/弹窗库(原生 Tauri 窗 + WebAudio)。
- toast 的"绝不抢焦点"兜底(`WS_EX_NOACTIVATE`)留待运行时确有问题再做。
- 不改三层锁、COM、主持人端既有控制逻辑。
