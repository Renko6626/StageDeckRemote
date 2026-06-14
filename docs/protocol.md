# 通信协议

> 占位文档。当前 API 的权威定义见 [`project.md`](project.md) §9，实现见
> `src-tauri/src/routes.rs`（主持人 HTTP API）与 `src-tauri/src/commands.rs`（导播 IPC）。

主持人端 HTTP API 概览：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/presenter?token=` | 主持人控制页面 |
| GET | `/api/status?token=` | 运行状态 |
| GET | `/api/snapshot.jpg?token=` | 当前画面（MVP 轮询） |
| POST | `/api/click` | 归一化坐标点击 |
| POST | `/api/next` `/api/prev` | 翻页 |
| POST | `/api/lock` `/api/unlock` | 主持人本地锁定/解锁 |
| POST | `/api/black` `/api/restore` | OBS 黑屏/恢复（P2，占位） |

待补充：WebSocket 升级方案、错误码完整列表、坐标映射示意图。
