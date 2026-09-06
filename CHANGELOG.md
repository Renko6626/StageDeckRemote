# Changelog

格式宽松版的 [Keep a Changelog](https://keepachangelog.com/);
版本号遵循 [SemVer](https://semver.org/lang/zh-CN/) 但 0.x 期间允许小幅度破坏。

## [0.2.3] - 2026-09-06

### Fixed

- 主持人状态心跳的等待时间延长至 35 秒，与 Relay 的 30 秒隧道超时一致，避免手机在 Relay 或短暂网络抖动时于第 5 秒主动断开。

## [0.2.2] - 2026-09-06

### Fixed

- 宏执行仅在明确的参数数量错误时回退无参调用，避免运行时异常后重复执行。
- 宏执行与 PPT 查询改为后台调用并限制并发；超时保持忙碌保护，手机心跳与导播日志独立刷新。
- 区分 PowerPoint 忙碌和网络断开，显示宏失败、超时及检查 VBA 弹窗的提示。

### Changed

- 补充宏开始、形状/宏名、回退、耗时、超时、迟到结果和 COM/VBA 异常详情日志。
- 新增宏回退、超时保护、心跳隔离、忙碌提示和日志刷新回归测试。

## [0.2.1] — 2026-06-27 · 导播 GUI 双模式

- **重构**:导播 GUI 改 Setup/Live 双模式布局。顶栏加 `[Setup] [Live]` 切换;Setup 模式 rail 装 setup-class widget(选窗 / QR / 测试翻页 / 小型紧急锁);Live 模式 rail 装 live-class widget(大字计时 / 快捷消息 / 巨型紧急锁)。切换是纯前端 UI,不动主持人 ARM/lock/token,localStorage 也不持久化模式(冷启动总在 Setup)。
- **拆分**:`LocalControlPanel` 三合一 blob 拆开 —— 远控 toggle → `StatusBar`,紧急锁 → `EmergencyLock`(带 `size: "small" | "large"` prop),本地翻页 → `LocalNav`(改名)。`ControlStatus` 也并入 `StatusBar`。每个组件单一职责,可独立测。
- **测试基础设施**:引入 `vitest` + `@vue/test-utils` + `jsdom` + `@pinia/testing`。新增 39 个 Vue 组件单测覆盖 EmergencyLock(6)/ StatusBar(15)/ LocalNav(4)/ DirectorView(12)+ smoke(2)。Tauri IPC 在测试环境下全部模块级 mock。
- 见 [`docs/superpowers/specs/2026-06-27-director-gui-setup-live-split-design.md`](docs/superpowers/specs/2026-06-27-director-gui-setup-live-split-design.md)。

## [0.2.0] — 2026-06-25 · Relay 架构

- **新增**:Relay 模式作为 LAN 直连之外的备选传输路径(校园网内中继)。
  - 混合架构:控制走 HTTP 隧道(`/r/<sid>/api/*`),推流走二进制 WS。
  - 引入 Cargo workspace + 共享 `protocol` crate + Linux `relay` 二进制。
  - 导播 GUI 加 Relay 配置 + 状态指示;QR 面板加 LAN/Relay 切换。
  - 见 [`docs/adr/0007-relay-hybrid-tunnel.md`](docs/adr/0007-relay-hybrid-tunnel.md)。
- **维护**:9 个 task 全程经过 spec→plan→subagent→review 流程;最终 44 测试通过 + 1 ignored。

## [0.1.18] — 2026-06-18 · 应用图标

- StageDeck 应用 logo 全套(导播 NSIS 安装包 / Windows 任务栏 / 主持人浏览器标签 favicon),替换 Tauri 占位。
- 设计方向 A3:四角准星 + 双雪佛龙(推进)+ 红点(LIVE)。

## [0.1.17] — 2026-06-16 · 主持人大字计时

- 主持人页计时改为接管 `SLIDE` 大字栏(超时转红);状态栏小 chip 移除。

## [0.1.16] — 2026-06-16 · 截图/推流优化

- **核心**:截图链路改为 "变化驱动 + 单点编码 + 推送"。
  - WGC 帧 → 采样 hash 检测变化 → 变化才编码 q82 → `frame_broker`(`watch::Sender`)扇出。
  - 主持人 `GET /api/stream.mjpg`(multipart),导播 Tauri Channel 推 `<canvas>`。
  - 静止时 ≈ 零 CPU。
- 见 [`docs/adr/0006-capture-stream-broker.md`](docs/adr/0006-capture-stream-broker.md)。

## [0.1.15] — 2026-06-16 · audit 卫生轮

- 清死代码 / 过期 `#[allow(dead_code)]`(Windows 交叉编译 0 warning)。
- 消息通道改有界 + `try_send`(防 token-gated 刷消息致内存增长)。
- 导播 webview CSP 从 `null` 改为允许 `data:` / inline styles / Tauri IPC 的策略。
- 主持人 250ms 计时 tick 改为按需启停。
- `DirectorMessages` 编辑列表换稳定 `:key`。
- `QrCodePanel` 加 `try/catch` + 空 URL 清画布。
- README + architecture.md 按现状订正。

## [0.1.14] — 2026-06-16 · audit Critical/High/Medium 修复

- `AppState::lock()` 中毒自恢复 helper,统一全部锁点(消除 `.expect` panic 与 `if let Ok` 静默 no-op 的不一致)。
- COM/截图 handler 包 `spawn_blocking`(不阻塞 tokio worker)。
- token 改常量时间比较(`constant_time_eq`);加 64 KiB body 上限。
- `/api/status` 不再返回 `active_client_id`(避免被动同 token 持有者抢控)。
- 主持人 401 自愈;动作 403 给反馈;锁按钮判据按 ARM/LOCK 分别匹配后端。
- 导播轮询加 in-flight 守卫。
- 详见 [`docs/superpowers/specs/2026-06-16-code-audit.md`](docs/superpowers/specs/2026-06-16-code-audit.md)。

## [0.1.13] — 2026-06-16 · 设置面板

- 导播 GUI 加齿轮 → 设置模态:重置访问码 / 服务端口(重启生效)/ 提示音开关。

## [0.1.0 – 0.1.12]

早期 MVP 期。原始产品文档见 [`docs/project.md`](docs/project.md)。
