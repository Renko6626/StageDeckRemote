# Changelog

格式宽松版的 [Keep a Changelog](https://keepachangelog.com/);
版本号遵循 [SemVer](https://semver.org/lang/zh-CN/) 但 0.x 期间允许小幅度破坏。

## [0.3.1] - 2026-10-24

### Fixed

- 导播和手机同时读取 PowerPoint 状态时，最多等待 COM 通道释放 1 秒，减少正常扫描重叠导致的忙碌误报。

### Changed

- 导播整页扫描间隔调整为 2.5 秒，手机约每 2.7 秒扫描一次；心跳、权限刷新和画面推送频率不变，手机操作完成后立即刷新。

### Upgrade notes

- 使用 Relay 时同时更新本版随附的 Linux Relay 并刷新手机页面，使中继提供新的手机脚本。Relay 协议及服务版本仍为 0.1.2。
- 尚未进行 Windows 安装、真实 PowerPoint 和手机扫码实机验收。

## [0.3.0] - 2026-10-22

### Added

- 导播日志自动保存到本机 JSONL 文件，单文件约 5 MiB，保留当前文件和 3 份历史；日志面板增加详细诊断开关、目录入口及文件写入错误提示。
- 详细模式记录 COM 成员查找与调用的开始、完成、耗时和 HRESULT，并记录请求关联、输入发送及捕获异常，便于定位放映期间的卡顿和忙碌。
- 手机端尽力回传请求超时、网络错误、HTTP 错误及脚本异常，同类错误抑制重复上报；上报失败不影响控制，也不自动重试。Relay 0.1.2 增加对应路由。
- 过滤日志中的访问码、Relay 密钥和凭据查询参数，不记录请求体、画面或文稿正文。使用方法见 [诊断说明](docs/diagnostics.md)。

### Fixed

- PPT 通道忙碌时保留屏幕点击和基础翻页，宏、黑屏和恢复单独受限。手机基础翻页直接激活选中窗口并模拟方向键，不再等待 COM 通道。
- 忙碌提示说明仍可使用的功能，不再统一禁用全部控制；继续使用现有状态轮询。

### Upgrade notes

- 更新 Windows 导播客户端；使用 Relay 时同时替换并重启随附的 Linux Relay 0.1.2，手机刷新页面。
- 尚未进行 Windows 安装、真实 PowerPoint 动态页面和手机扫码实机验收。

## [0.2.9] - 2026-10-20

### Changed

- Setup 侧栏收敛为演示窗口、主持人连接及远程控制；完整状态放入详情，本地翻页与紧急锁移至预览下方。保留 Live 工具、分栏拖拽、真实二维码、浏览器打开及原有控制逻辑。
- Relay 配置移至主持人连接卡片的独立面板，显示运行状态与待生效配置，保存、停用仍需重启生效。
- 服务器地址支持 HTTP、HTTPS、WS、WSS 及无前缀输入，统一为 WS 并保留显式端口；未增加 TLS 支持。
- 顶栏通用设置入口增加文字、扩大点击区域并提高对比度，保留提示音、服务端口与访问码重置。

### Fixed

- 主窗口关闭后显式退出整个导播应用，避免隐藏消息窗口使进程及 Relay 自动重连任务残留。

## [0.2.8] - 2026-10-20

### Fixed

- 修复启用 Relay 后重启客户端时的启动崩溃：同步初始化阶段通过 Tauri 运行时启动控制及推流任务。
- 增加同步启动回归测试，验证两个 Relay 通道均能完成连接。

### Known limitations

- 尚未进行 Windows 安装及 PowerPoint 实机验收。

## [0.2.7] - 2026-10-18

### Relay 补充修复（2026-10-19）

- 手机端统一请求函数为 Relay POST 补齐 URL token，保留 JSON token，修复中继返回 `401 invalid_token` 的问题。LAN 请求不变。
- v0.2.7 Release 补充 `stagedeck-relay-linux-x86_64-0.2.7-post-token-fix` 及独立校验文件；旧附件保留。替换并重启 Relay、刷新手机页面即可生效，Windows 安装包无需重装。

### Fixed

- 主动操作最多等待 PowerPoint 通道释放 500ms，避免短暂状态探测导致操作立即返回 `409 ppt_busy`；通道持续占用时仍拒绝执行。
- 手机发送操作期间暂停新的 PowerPoint 状态探测，忙碌时明确提示本次操作未执行。

### Known limitations

- 本版未修改 PPT 文件及其中的 VBA 模拟点击逻辑；动画目标退场后再次点击导致误翻页的问题仍需单独处理。
- 尚未进行 Windows 安装及 PowerPoint 实机验收。

## [0.2.6] - 2026-10-13

### Fixed

- Relay 保留无人观看时的最新帧，导播重连补发静止画面；连接接管后旧控制通道不能覆盖访问码，旧推流不能覆盖新画面或错误清除在线状态。
- 主持人页面拒绝非法会话 ID，修复 URL 内容直接插入 HTML 的问题。
- Relay 设置保存与运行配置分离，重启前二维码继续指向当前连接；地址校验和磁盘写入失败会反馈到设置界面。
- admin 状态页补充中文连接文字、手机布局及使用说明。
- 提供题目模板局部修复脚本：限定第 5 页 back 的宏模块，导出配套音效；原始 PPTM 保持不变，第 4 页 back 行为仍待确认。

### Known limitations

- 通用 PPT 动作识别、透明点击层显示、宏签名与上下文校验暂缓，见 [后续待办](docs/vba/compatibility-backlog.md)。本版未包含这些优化，仍需 Windows PowerPoint 真机验收。

## [0.2.5] - 2026-09-07 · 糟糕网络下的控制可靠性

### Added

- 动作接口（翻页 / 黑屏 / 恢复 / 宏 / 点击）支持幂等 `op_id`：服务端 60s 内同 id 只执行一次，已完成的原样回放，仍在执行的回 `409 op_in_flight`。命中写导播日志"已去重"。
- 手机端 `sendOp`：网络错误 / 超时 / 502 / 503 时用同一 `op_id` 自动重发（前两次 1.5s 快试，第三次等完整结果）；发送期间所有动作按钮禁用并显示"…"；预算耗尽提示"未送达或结果未知，核对页码后再按"。
- 手机连接状态三档：`LINK` / `弱 · Ns`（黄点，按钮可用）/ `DOWN`（LAN 8s、Relay 15s）。
- Relay ↔ 导播存活探测：relay 每 10s 发 Ping（控制 WS 用 JSON、推流 WS 用协议层帧），任一方向 25s 无入站即判半开断开；导播握手 8s 超时。常量固定在 `protocol` crate，relay 配置不加项。
- 导播 StatusBar 主持人心跳三态（在线 / 弱 · Ns 前 / 离线）；Relay 连接、断开原因、退避写导播日志面板。

### Changed

- 手机 status 心跳超时从 35s 改回 5s，由"弱"档承担抖动缓冲。
- 服务端控制端离线阈值 4s → 10s。
- Relay 在导播断开或被新连接接管时立即清空等待中的隧道请求（`502 director_offline`），不再让手机等满 30s。
- 导播推流 WS 循环改为同时读写，能收到 Close 帧并回应协议层 Ping。

见 [`docs/superpowers/specs/2026-09-07-bad-network-resilience-design.md`](docs/superpowers/specs/2026-09-07-bad-network-resilience-design.md)。

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
