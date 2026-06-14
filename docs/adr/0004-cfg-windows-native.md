# ADR-0004：原生能力 `#[cfg(windows)]` + 跨平台占位

- 状态：Accepted（2026-06-14）
- 相关：[architecture.md §8](../architecture.md)、[project.md §11.6–§11.8](../project.md)

## 背景

截图、模拟输入、窗口枚举都是 Win32 专有能力（`windows-rs`），只能在 Windows 编译运行。
但开发常在非 Windows 环境进行（本项目开发机为 Linux，且缺 `webkit2gtk`、无 sudo），
需要能在本地编译/检查大部分后端逻辑，而不被原生能力卡住。

## 决策

原生模块（`window_manager` / `capture` / `input`）按平台分支：

- `#[cfg(windows)]`：真实 Win32 实现（当前为 `TODO(windows)` 占位）。
- `#[cfg(not(windows))]`：占位实现 —— 截图返回占位图、输入返回"不支持"、枚举返回空集。

并把 Axum 服务层（`web_server` / `routes`）写成只依赖 `Arc<AppState>`、不引用 `tauri`，
从而可在隔离 crate 中 `cargo check`（绕开 `webkit2gtk` 这一 Tauri 的 Linux 系统依赖）。

## 影响

**正面**
- 服务层、状态、鉴权、坐标映射等可在 Linux/macOS 编译与 CI 检查，开发反馈快。
- 平台差异集中在三个模块，边界清晰；`截图占位图`让"手机看到画面"链路在任何平台可联调。

**负面 / 代价**
- 占位分支与真实分支需保持签名一致；新增原生函数要同时补两侧。
- 完整 Tauri 应用仍只能在 Windows（或装了 webkit2gtk 的 Linux）构建运行。

## 备选方案

- **只写 Windows 代码、不做占位**：代码更干净，但本机完全无法编译，开发反馈差 —— 否决。
- **用跨平台截图/输入库**：增加依赖且能力/可控性不如直接 Win32，舞台场景需要精确控制 —— 暂否。
