# 主持人端 HTTP API 协议

> 主持人浏览器 ↔ 导播端 Axum 服务之间的**对外协议**权威参考。
> **基准 = 当前 `src-tauri/src/routes.rs` 实现**；与 [`project.md` §9](project.md) 的设计相比，
> 本文补全了状态码、token 传递、字段类型与错误码全集，并以实现为准。
> 导播端的本机 Tauri IPC（特权、不对外）见 [`architecture.md` §3/§7](architecture.md) 与 `commands.rs`，不在此文档范围。
>
> 标注 **【计划中】** 的部分尚未实现。

---

## 1. 总览

- **Base URL**：`http://<导播电脑 IP>:8787`（端口默认 8787）。
- **绑定**：服务监听 `0.0.0.0:8787`（局域网可达）。
- **鉴权**：每个请求都要带 `token`（应用启动时随机生成）。
  - **GET** 请求：token 放 **query**，如 `?token=xxxx`。
  - **POST** 请求：token 放 **JSON body**。
- **请求体**：POST 一律 `Content-Type: application/json`。
- **响应体**：除截图(二进制 JPEG)与页面/静态资源外，均为 JSON。

### 信任与三层锁

| 操作类别 | 放行条件 |
| --- | --- |
| 看状态 / 看画面 / 锁定 | token 正确即可 |
| 翻页（next/prev）/ 解锁（unlock）| `button_allowed = remote_enabled && !control_locked` |
| 点击预览图（click）| `click_allowed = remote_enabled && !control_locked && presenter_unlocked` |

锁的来源与默认值见 [`architecture.md` §5](architecture.md)。

---

## 2. 端点

### 2.1 `GET /presenter`

主持人控制页面（HTML）。

- 请求：`?token=xxxx`
- 成功：`200 text/html`
- 失败：`401`（纯文本 `invalid token`）

### 2.2 `GET /presenter/presenter.css` · `GET /presenter/presenter.js`

主持人页静态资源（编译期内嵌）。

- 请求：无参数
- 成功：`200`（`text/css` / `text/javascript`）
- 说明：静态资源**不校验 token**（仅页面骨架与脚本，无敏感数据）。

### 2.3 `GET /api/status`

运行状态。

- 请求：`?token=xxxx`
- 成功：`200 application/json`

```json
{
  "ok": true,
  "ppt_window_found": true,
  "remote_enabled": true,
  "control_locked": false,
  "presenter_unlocked": true,
  "active_client_id": "host-tablet",
  "server_time": 1710000000
}
```

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `ppt_window_found` | bool | 是否已选中目标窗口 |
| `remote_enabled` | bool | 导播总开关（锁层 1） |
| `control_locked` | bool | 导播强制锁（锁层 2） |
| `presenter_unlocked` | bool | 主持人本地解锁（锁层 3） |
| `active_client_id` | string \| null | 当前控制端 |
| `server_time` | number | 服务端 Unix 秒 |

- 失败：`401 {"ok":false,"reason":"invalid_token"}`

### 2.4 `GET /api/snapshot.jpg`

当前画面截图。

- 请求：`?token=xxxx`
- 成功：`200 image/jpeg`，`Cache-Control: no-store`（服务端固定 1280×720、质量 70）
- 失败：`401 {"ok":false,"reason":"invalid_token"}`；`500`（纯文本 `capture error: ...`）
- 说明：MVP 为 HTTP 轮询；未选窗口时返回占位图。**【计划中】** WebSocket / MJPEG 推流。

### 2.5 `POST /api/click`

在预览图上点击 → 归一化坐标 → 真实屏幕点击。

- 请求体：

```json
{ "token": "xxxx", "client_id": "host-tablet", "x": 0.723, "y": 0.614 }
```

| 字段 | 类型 | 必需 | 说明 |
| --- | --- | --- | --- |
| `token` | string | 是 | 鉴权 |
| `client_id` | string | 否 | 客户端标识（当前仅记录） |
| `x` / `y` | number | 是 | 归一化坐标，范围 `[0,1]` |

- 成功：`200`

```json
{ "ok": true, "screen_x": 1388, "screen_y": 663 }
```

- 映射：`screen = (left + round(x*width), top + round(y*height))`（见 [`architecture.md` §6](architecture.md)）。
- 失败（见 §3）：`invalid_token` / `coordinate_out_of_range` / `control_locked` / `ppt_window_not_found` / `input_failed`

### 2.6 `POST /api/next` · `POST /api/prev`

翻页（下一页 / 上一页）。

- 请求体：`{ "token": "xxxx", "client_id": "host-tablet" }`（`client_id` 可选）
- 成功：`200 {"ok":true}`
- 放行条件：`button_allowed`
- 失败：`invalid_token`(401) / `control_locked`(403) / `input_failed`(500)

### 2.7 `POST /api/lock` · `POST /api/unlock`

主持人端本地锁定 / 释放控制。

- 请求体：`{ "token": "xxxx", "client_id": "host-tablet" }`
- `lock`：成功 `200 {"ok":true}`，置 `presenter_unlocked = false`（仅需 token）。
- `unlock`：置 `presenter_unlocked = true`，**要求 `button_allowed`**。
  - 失败：`invalid_token`(401) / `remote_disabled`(403)
- 说明：导播端强制锁 / 关闭远程优先级更高，会立即清掉解锁状态。

### 2.8 `POST /api/black` · `POST /api/restore`

OBS 黑屏 / 恢复。

- 请求体：`{ "token": "xxxx", "client_id": "host-tablet" }`
- 当前：**【计划中】** 返回 `501 {"ok":false,"reason":"obs_not_implemented"}`
- 目标：接入 OBS WebSocket 切场景（[`project.md` §6.3](project.md)）。

---

## 3. 错误码

失败响应统一为 `{"ok": false, "reason": "<code>"}`（§2.1/§2.4 的页面/二进制端点除外）。

| HTTP | `reason` | 触发 |
| --- | --- | --- |
| 401 | `invalid_token` | token 缺失或不匹配 |
| 400 | `coordinate_out_of_range` | `x` 或 `y` 不在 `[0,1]` |
| 403 | `control_locked` | 三层锁未放行（click / 翻页） |
| 403 | `remote_disabled` | unlock 时远程未开启或被强制锁 |
| 409 | `ppt_window_not_found` | 未选中目标窗口 |
| 500 | `input_failed` | 模拟输入失败（非 Windows 平台占位即返回此项） |
| 501 | `obs_not_implemented` | OBS 功能尚未实现 |

---

## 4. 坐标约定

主持人端**永远发送归一化坐标** `{x,y} ∈ [0,1]`，不发设备像素（设备尺寸 / 缩放各异）。
服务端校验范围后按目标窗口矩形映射为屏幕坐标。公式与风险见 [`architecture.md` §6](architecture.md)
与 [`project.md` §7](project.md)。

---

## 5. 计划中的演进

| 项 | 现状 | 目标 |
| --- | --- | --- |
| 画面传输 | HTTP 轮询 `snapshot.jpg` | WebSocket / MJPEG 推流 |
| OBS 黑屏 / 恢复 | `501 obs_not_implemented` | OBS WebSocket 切场景 |
| 单控制端独占 | `active_client_id` 仅记录 | 抢占 / 踢出，仅一个 active client 可控 |

---

## 关联

- 设计与错误原因背景：[`project.md` §9](project.md)
- 实现：`src-tauri/src/routes.rs`
- 架构与状态模型：[`architecture.md`](architecture.md)
