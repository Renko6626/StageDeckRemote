# StageDeck Remote

局域网 PPT 远程控制系统。导播电脑运行桌面应用，主持人用手机/平板浏览器扫码访问，
在受限、安全、可随时被导播接管的前提下，看 PPT 画面、点击 PPT 按钮、翻页、黑屏/恢复。

完整需求与设计见 [`docs/project.md`](docs/project.md)。

## 架构

```
导播 GUI ──Tauri IPC──▶ Rust 核心 ◀──HTTP/WS── 主持人浏览器
（本机、特权）            │  AppState   │        （局域网、受限）
                         └─ Axum :8787 ┘
```

- **导播端**：Tauri v2 桌面应用。GUI（Vue 3 + TS）通过 **Tauri IPC** 调用本机特权操作
  （枚举/选择窗口、强制锁、远程开关、本地翻页）。这些能力**不暴露在网络上**。
- **主持人端**：纯浏览器页面，由内嵌的 **Axum** 服务（`0.0.0.0:8787`）提供。
  只暴露主持人需要的受限接口（看画面、点击、翻页、锁定），全部需要 token。
- **安全边界**：局域网上只能看到"主持人能做的"那部分 API；导播特权走 IPC，天然隔离。
  三层锁：`remote_enabled`（导播总开关，默认关）→ `control_locked`（导播紧急锁，默认释放）→
  `presenter_unlocked`（主持人 ARM）。`click_allowed = remote_enabled && !control_locked && presenter_unlocked`
  门控**所有**主持人动作（翻页/黑屏/恢复/宏/点击）；`button_allowed = remote_enabled && !control_locked`
  只作为主持人 ARM（`/api/unlock`）的前置条件。

## 技术栈

| 层 | 选型 |
| --- | --- |
| 桌面应用 | Tauri v2 |
| 导播 GUI | Vue 3 + TypeScript + Vite + Pinia + Vue Router |
| 后端核心 | Rust |
| 本地服务 | Axum + Tokio |
| 原生能力 | windows-rs（截图/输入/窗口枚举 + PowerPoint COM 控制，仅 Windows） |
| 主持人端 | 浏览器（原生 HTML/CSS/JS，无需安装） |

## 目录结构

```
src/                 导播端 GUI（Vue）
  components/         面板组件（窗口选择/二维码/状态/本地控制/日志）
  stores/            Pinia store（appState / networkState）
  views/             DirectorView 主面板
  presenter/         主持人端页面（HTML/CSS/JS，由 Axum 直接 serve）
  utils/             api.ts（IPC 封装）/ qrcode.ts
src-tauri/src/       Rust 核心（见 docs/project.md §11 模块职责）
  commands.rs        导播端 Tauri IPC 命令（特权）
  web_server.rs      Axum 服务启动
  routes.rs          主持人端 HTTP API（受限）
  window_manager / capture / capture_session / input / ppt_com   原生能力（#[cfg(windows)]）
docs/                项目文档
```

## 开发

前置：Node 18+、pnpm、Rust 1.77+。**目标平台为 Windows**（原生截图/输入依赖 Win32）。

```bash
pnpm install
pnpm tauri dev      # 启动桌面应用（需在 Windows 上运行原生能力）
pnpm typecheck      # 仅类型检查前端
```

> 注意：导播 GUI 依赖 Tauri IPC，必须用 `pnpm tauri dev` 启动。直接 `pnpm dev`
> 在纯浏览器里打开时 `invoke` 会失败，面板会显示"未连接"，属正常现象。

### 平台说明

- 原生能力（`capture` / `capture_session` / `input` / `window_manager` / `ppt_com`）通过
  `#[cfg(windows)]` 实现真实分支（WGC/GDI BitBlt 截图、SendInput 输入、EnumWindows 枚举、
  PowerPoint COM 控制），非 Windows 平台为 `#[cfg(not(windows))]` 占位实现（截图返回占位图、
  其余返回"仅 Windows"），以便在 Linux/macOS 上开发与编译**服务层**。
- 在 Linux 上构建完整 Tauri 应用还需系统库 `webkit2gtk` / `librsvg`
  （见 <https://tauri.app/start/prerequisites/>）。生产构建建议在 Windows 上进行。

## 当前状态

核心闭环已打通：结构、IPC/HTTP 通路、API 路由、三层锁、截图链路均就绪。Win32 原生能力
均已实现为 `#[cfg(windows)]` 真实分支（非 Windows 为占位 stub）：WGC/GDI BitBlt 截图
（`capture.rs` + `capture_session.rs`）、SendInput 输入（`input.rs`）、EnumWindows 窗口枚举
（`window_manager.rs`）、PowerPoint COM 控制（`ppt_com.rs`，翻页/黑屏/宏/取放映态）；
二维码已用离线 `qrcode` 库本地生成（`src/utils/qrcode.ts` + `QrCodePanel.vue`）。源码中已无
`TODO(windows)` 占位标记。路线图见 [`docs/project.md`](docs/project.md) §14–§15。
