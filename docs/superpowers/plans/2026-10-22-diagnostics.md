# Diagnostics Implementation Plan

**Goal:** 在不改变控制行为的前提下，提供可分享的本地诊断记录。
**Spec:** ../specs/2026-10-22-diagnostics-design.md
**Architecture:** Rust Diagnostics 管理有界 JSONL 文件；log.rs 复用现有事件出口。Vue 通过 IPC 管理开关和打开目录，手机经现有 HTTP/Relay 通道单次报告异常。
**Tech Stack:** 现有 Rust std/serde/Tauri/Axum、Vue、原生 JS，无新增依赖。

- [x] 文件存储与后端接入：diagnostics.rs、log.rs、app_state.rs、lib.rs、commands.rs、routes.rs。补充关键文件轮换/脱敏和上报验证测试。
- [x] 导播入口：LogPanel.vue、utils/api.ts；显示模式、目录和错误，复用 opener。
- [x] 手机异常：presenter.js/spec、Relay 路由，复用认证、无重试，不影响操作。
- [x] COM 诊断：ppt_com.rs；逐成员调用开始/完成/耗时/错误，避免返回值与正文。
- [x] 集中检查、代码审阅和操作说明；不构建安装包，不覆盖用户现有改动。

验证：`pnpm test`（76 通过）、`pnpm build`、`tauri-env linux cargo test -p stagedeck-remote --lib`（60 通过）、`tauri-env linux cargo test -p relay phone_routes::tests`（5 通过）、从 src-tauri 运行 `tauri-env win cargo xwin build --target x86_64-pc-windows-msvc`，均成功。Windows 编译保留原有 RelayHandle 未使用字段警告。未在 Windows 运行实际 PowerPoint 或打开日志目录。

审阅：未发现阻断项；普通模式逐次探测失败刷屏的问题已收敛到详细模式，正常模式保留变化记录。
