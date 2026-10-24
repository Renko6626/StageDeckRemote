# StageDeck Remote

局域网 PPT 远程控制系统。导播电脑运行桌面应用，主持人用手机/平板浏览器扫码访问，
在受限、安全、可随时被导播接管的前提下，看 PPT 画面、点击 PPT 按钮、翻页、黑屏/恢复。

完整需求与设计见 [`docs/project.md`](docs/project.md)。

## 下载与快速使用

当前版本为 **v0.3.1**，安装包、Linux Relay 和校验文件见 [GitHub Release](https://github.com/Renko6626/StageDeckRemote/releases/tag/v0.3.1)。

局域网直连不需要启动 Relay：

1. 在 Windows 导播电脑安装 StageDeck Remote，打开 PowerPoint 文稿并开始放映。
2. 手机与导播电脑接入能互相访问的同一局域网。选择要捕获的 PPT 窗口，打开「远程控制」。
3. 主持人连接卡片选择 **局域网**，手机扫码，在手机端点「释放控制」后即可操作。
4. 使用宏按钮时，文稿自身的宏也需要能够在 PowerPoint 中运行。先确认电脑上能正常使用这些按钮。

扫码地址通常形如 `http://192.168.50.10:8787/presenter?token=...`。页面打不开时，检查导播电脑防火墙和 Wi-Fi 客户端隔离；详细步骤见 [网络手册](docs/network.md)。

## Relay 简单教程

Relay 是单独运行在 **Linux x86-64 服务器**上的中继程序。导播电脑和手机都必须能访问这台服务器的端口；使用校内服务器时，两端也需接入校园网或相应 VPN。

**已有 Relay 部署：** v0.3.1 调整了内嵌手机页面的扫描频率，使用中继时需同时更新本版 Relay 附件、重启中继并刷新手机页面；Relay 程序版本仍为 0.1.2。鉴权规则见 [协议说明](docs/protocol.md#12-共通规则)。

### 1. 下载并启动中继

在 Linux 服务器终端执行：

```bash
mkdir -p stagedeck-relay
cd stagedeck-relay
curl -fL https://github.com/Renko6626/StageDeckRemote/releases/download/v0.3.1/stagedeck-relay-linux-x86_64-0.1.2 -o relay
chmod +x relay
./relay
```

未另行配置时默认监听 `0.0.0.0:8080`。终端需要保持运行，按 `Ctrl+C` 停止。服务器防火墙或安全组需允许导播和手机访问 TCP 8080。

另开一个终端检查服务：

```bash
curl -i 'http://127.0.0.1:8080/r/SANITY/api/status?token=anything'
```

没有导播注册 `SANITY` 会话时，预期返回 **502** 和 `session_unavailable`，说明中继已启动。根路径 `/` 没有首页，返回 404 属正常情况。

### 2. 让导播端连接中继

假设服务器地址为 `10.0.0.20`，在导播端 **Setup → 主持人连接 → 配置连接 → Relay 中继** 中填写：

| 字段 | 示例与说明 |
| --- | --- |
| 服务器地址 | `http://10.0.0.20:8080`；自动统一为 `ws://`，也可输入不带前缀的主机与端口；替换为两端能访问的服务器地址，不能填写 `0.0.0.0` 或服务器的 `127.0.0.1` |
| 会话编号 | `PROM-2026`；4–16 位大写字母、数字或连字符，每台导播使用不同编号，重复编号与密钥会相互接管 |
| 连接密钥 | 自定的非空密钥，用于防止其他导播占用同一 Session ID |

点「保存」后**重启导播应用**，等待主持人连接区域显示“Relay 已连接”。保存设置不会立即切换当前连接；关闭 Relay 也需要重启导播才能生效。

二维码面板选择 **Relay** 后，手机扫码进入形如 `http://10.0.0.20:8080/r/PROM-2026/presenter?token=...` 的地址。地址中的 token 由软件生成，与上面的 secret 不同。地址输入中的 HTTP、HTTPS、WS、WSS 前缀均转换为 `ws://`；实际连接仍使用普通 WebSocket，未增加 TLS 支持。

### 3. 自定义端口和长期运行

需要改端口时，在 `relay` 所在目录新建 `relay.toml`：

```toml
listen = "0.0.0.0:8080"
session_grace_secs = 60
tunnel_timeout_secs = 30
# 可选：启用只读状态页，先替换为独立管理密钥
# admin_token = "replace-with-your-own-admin-token"
```

停止原进程后，指定配置启动：

```bash
STAGEDECK_RELAY_CONFIG="$PWD/relay.toml" ./relay
```

不指定环境变量时，程序读取 `/etc/stagedeck-relay/config.toml`；配置缺失或无法解析时会使用默认值，请核对启动日志中的 `listen` 和 `config_path`。

配置 `admin_token` 后，可访问 `http://服务器地址:8080/admin/status?token=管理密钥` 查看连接状态；未配置时此页返回 404。要让中继在终端关闭后继续运行、开机自动启动，按 [网络手册 §7.2](docs/network.md#72-一次性准备-c在中继服务器上部署-relay) 配置 systemd。更新中继时需替换二进制并重启进程，单独更新 Windows 安装包不会更新中继内嵌的手机页面。

## 架构

**默认 LAN 直连**：

```
导播 GUI ──Tauri IPC──▶ Rust 核心 ◀──HTTP/WS── 主持人浏览器
（本机、特权）            │  AppState   │        （局域网、受限）
                         └─ Axum :8787 ┘
```

**v0.2.0 起可选 Relay 模式**（场馆 WiFi 不可控时走校内中继）：

```
导播 PC ──WS 控制隧道──▶ Relay (校内 Linux)  ◀──HTTP/MJPEG── 主持人浏览器
        ──WS 推流通道──▶  (会话路由 + 扇出)
```

- **导播端**：Tauri v2 桌面应用。GUI（Vue 3 + TS）通过 **Tauri IPC** 调用本机特权操作
  （枚举/选择窗口、强制锁、远程开关、本地翻页）。这些能力**不暴露在网络上**。
- **主持人端**：纯浏览器页面。LAN 模式下由导播 PC 的 **Axum** 服务（`0.0.0.0:8787`）提供；
  Relay 模式下由 CLAB 中继直接吐出（同一份 `include_str!` 编译进各自二进制）。手机端**协议零感知**——
  扫到哪个二维码就走哪条路径，所有调用都是相对路径。
- **安全边界**：局域网上只能看到"主持人能做的"那部分 API；导播特权走 IPC，天然隔离。
  三层锁：`remote_enabled`（导播总开关，默认关）→ `control_locked`（导播紧急锁，默认释放）→
  `presenter_unlocked`（主持人 ARM）。`click_allowed = remote_enabled && !control_locked && presenter_unlocked`
  门控**所有**主持人动作（翻页/黑屏/恢复/宏/点击）；`button_allowed = remote_enabled && !control_locked`
  只作为主持人 ARM（`/api/unlock`）的前置条件。
- **画面传输**（v0.1.16 起）：WGC 后台捕获 + 单点编码线程 + `frame_broker` watch 扇出，
  主持人走 `GET /api/stream.mjpg`（multipart），导播预览走 Tauri Channel 推 → canvas。
  静止时 ≈ 零 CPU。设计见 [ADR-0006](docs/adr/0006-capture-stream-broker.md)。
- **Relay 架构**（v0.2.0 起）：`crates/relay/` 是独立 Linux 二进制；控制平面走 HTTP 隧道
  （relay 推 `TunnelReq` 给导播 WS，导播把请求喂同一个 `axum::Router`），推流平面走二进制 WS
  （broker 复刻一份 watch 给手机）。手机端协议字面跟 LAN 直连一致。设计见
  [ADR-0007](docs/adr/0007-relay-hybrid-tunnel.md)。

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
Cargo.toml           Cargo workspace 根
src/                 导播端 GUI（Vue）
  components/         面板组件（窗口选择/二维码/状态/本地控制/日志/Settings/Toast）
  stores/            Pinia store（appState / networkState）
  views/             DirectorView 主面板 / ToastView
  presenter/         主持人端页面（HTML/CSS/JS，由 Axum/Relay 直接 serve）
  utils/             api.ts（IPC 封装）/ qrcode.ts
src-tauri/src/       Rust 核心（见 docs/project.md §11 模块职责）
  commands.rs        导播端 Tauri IPC 命令（特权）
  web_server.rs      Axum 服务启动
  routes.rs          主持人端 HTTP API（受限）
  frame_broker.rs    watch 扇出编码帧 + 采样 hash 变化检测
  relay_client.rs    Relay 模式下的 WS 控制隧道 + WS 推流（v0.2.0）
  window_manager / capture / capture_session / input / ppt_com   原生能力（#[cfg(windows)]）
crates/
  protocol/          共享 wire schema（ControlMessage + mjpeg_part）
  relay/             Relay binary（Linux，CLAB 部署；lib+bin split）
    src/             main.rs / lib.rs / session.rs / director_ws.rs / phone_routes.rs / config.rs
    tests/           end_to_end.rs（Linux loopback 集成测试，不依赖 Windows/Tauri）
docs/                项目文档
```

## 开发

Windows 本地开发、增量构建、手机页面刷新和故障快照见 [Windows 快速迭代指南](docs/windows-development.md)。本次 GIF 页预览冻结的复现、对照数据和代码入口见 [排查交接记录](docs/debugging/2026-10-24-gif-preview.md)。

前置：Node.js LTS、pnpm 9、Rust stable 和 Tauri 平台依赖。**目标平台为 Windows**（原生截图/输入依赖 Win32）。

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

## 当前状态（v0.3.1）

核心闭环已打通并经多轮演进。Win32 原生能力均已实现为 `#[cfg(windows)]` 真实分支（非 Windows 为占位
stub）：WGC/GDI BitBlt 截图（`capture.rs` + `capture_session.rs`）、SendInput 输入（`input.rs`）、
EnumWindows 窗口枚举（`window_manager.rs`）、PowerPoint COM 控制（`ppt_com.rs`，翻页/黑屏/宏/取放映态）；
二维码用离线 `qrcode` 库本地生成（`src/utils/qrcode.ts` + `QrCodePanel.vue`）。源码中无 `TODO(windows)`
占位标记。

关键演进：

- **v0.3.1**：状态探测最多等待 COM 通道 1 秒，降低导播和手机整页扫描频率，保留操作后立即刷新。GIF 页引发的慢扫描与预览冻结仍在 Windows 实机排查，尚未修复。
- **v0.3.0**：增加本地 JSONL 诊断及详细 COM 调用日志；COM 忙碌时保留基础翻页和屏幕坐标点击。图像与 COM 状态是两条独立链路。
- **v0.2.7 Relay 补充附件**：修复手机 POST 请求缺少 URL token 导致的中继 401；需更新 Relay 并刷新手机页面。
- **v0.2.7**：主动 COM 操作最多等待通道释放 500ms，减少与短暂状态探测冲突产生的拒绝；手机发送操作期间暂停新探测。宏请求仍忙时提示「本次操作未执行」。
- **v0.2.6**：修复 Relay 重连、静止画面保留和连接接管，完善设置保存与只读 admin 状态页。
- **v0.2.5**：增加操作去重重试、连接状态分级和 Relay 存活探测。
- **v0.1.16** — 截图/推流改"变化驱动 + 单点编码 + 推送"。主持人 MJPEG 长连、导播 Tauri Channel 推 canvas、
  `frame_broker` 复用一份扇出，静止时 ≈ 零 CPU。详见 [ADR-0006](docs/adr/0006-capture-stream-broker.md)。
- **v0.2.0** — **Relay 架构**。引入 `crates/protocol` 共享 schema、`crates/relay` Linux 二进制、
  导播 `relay_client` WS 控制隧道 + WS 推流、双模并存（QR 切换 LAN/Relay）。集成测试可在 Linux loopback
  端到端跑通。详见 [ADR-0007](docs/adr/0007-relay-hybrid-tunnel.md) 与 [`docs/architecture.md` §11](docs/architecture.md)。

更新记录见 [CHANGELOG.md](CHANGELOG.md)。本版已完成 Windows 安装包与 Linux Relay 构建；GIF 页慢扫描和预览冻结已有 Windows 实机反馈，仍待定位修复，不能将构建成功视为该问题已通过验收。

文稿内部的 VBA 模拟点击、失效宏绑定和外置音效仍需由文稿作者核对，0.2.7 未修改 PPT 文件。已检查样本及未决事项见 [VBA 兼容性记录](docs/vba/compatibility-backlog.md)。路线图见 [`docs/project.md`](docs/project.md) §14–§15。
