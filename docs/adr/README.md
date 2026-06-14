# 架构决策记录（ADR）

本目录用轻量 ADR（Architecture Decision Record）固化项目的关键技术决策，便于审计与回溯。
每条记录包含：背景（Context）、决策（Decision）、影响（Consequences）、备选方案（Alternatives）。

新增决策时复制现有格式，编号递增；不删除旧记录，被取代时把状态改为 `Superseded by ADR-XXXX`。

| 编号 | 标题 | 状态 |
| --- | --- | --- |
| [0001](0001-use-tauri.md) | 导播端采用 Tauri v2 | Accepted |
| [0002](0002-ipc-http-trust-boundary.md) | 导播走 IPC、主持人走 HTTP 的信任边界 | Accepted |
| [0003](0003-presenter-vanilla-js.md) | 主持人页用原生 JS、由 Axum 直接 serve | Accepted |
| [0004](0004-cfg-windows-native.md) | 原生能力 `#[cfg(windows)]` + 跨平台占位 | Accepted |
| [0005](0005-mvp-snapshot-polling.md) | MVP 截图用 HTTP 轮询 + 占位图先打通链路 | Accepted |
