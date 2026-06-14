# ADR-0001：导播端采用 Tauri v2

- 状态：Accepted（2026-06-14）
- 相关：[architecture.md §3](../architecture.md)、[project.md §5](../project.md)

## 背景

导播端需要一个能在 Windows 上分发、像"正经应用"一样运行的桌面程序，并且要做
原生能力：枚举/截取 PowerPoint 窗口、模拟鼠标键盘。同时主持人端是浏览器网页，
因此应用本身还要内嵌一个局域网 Web 服务。技术上需要同时满足：好分发、原生 Win32、
能跑 Web 服务、前端用熟悉的 Web 技术。

## 决策

导播端用 **Tauri v2**（Rust 核心 + WebView 前端），前端 Vue 3 + TypeScript，
本地服务 Axum + Tokio，原生能力用 `windows-rs`。

## 影响

**正面**
- Rust 侧天然适配 Win32（窗口枚举、截图、`SendInput`）与高性能本地服务。
- 产物体积小、可打包为正常 Windows 应用，后续可加自动更新/安装包。
- 前端复用 Web 技术栈；主持人端零安装。

**负面 / 代价**
- 在 Linux 上构建完整应用需 `webkit2gtk`/`librsvg` 系统库（生产构建建议在 Windows）。
- 比纯脚本多一层构建链（Cargo + 前端打包）。

## 备选方案

- **Electron**：体积大（100MB+），且 Win32 仍需另寻原生方案 —— 否决。
- **纯 Python（PyInstaller）**：分发/兼容/杀软误报问题多（[project.md §5.3](../project.md)）—— 仅用于原型。
- **纯原生 GUI（egui/Slint）**：导播端不用 Web，但主持人端仍是 Web，等于两套 UI 技术栈 —— 否决。
- **Wails(Go) / .NET**：可行，但要换语言，`windows-rs + axum` 已足够 —— 否决。
