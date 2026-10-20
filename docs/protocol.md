# 主持人端 HTTP API 协议

> 主持人浏览器 ↔ 导播端 Axum 服务之间的**对外协议**权威参考。
> **基准 = 当前 `src-tauri/src/routes.rs` 实现**；与 [`project.md` §9](project.md) 的设计相比，
> 本文补全了状态码、token 传递、字段类型与错误码全集，并以实现为准。
> 导播端的本机 Tauri IPC（特权、不对外）见 [`architecture.md` §3/§7](architecture.md) 与 `commands.rs`，不在此文档范围。
>
> v0.2.0 起新增 Relay 部署形态，端点列表见 §6（LAN 别名）与 §7（Relay 路由族）；
> 业务语义与 LAN 直连完全一致——见 [`architecture.md` §11](architecture.md)。

---

## 1. 总览

### 1.1 部署形态与 Base URL

两种部署同时存在，**业务语义一致**，只是路由前缀不同：

| 部署 | Base URL | 路由前缀 | 说明 |
| --- | --- | --- | --- |
| **LAN 直连**（默认） | `http://<导播 IP>:8787` | `/api/*` 与 `/presenter/api/*`（别名，§6） | 导播 Axum 直接监听 |
| **Relay**（v0.2.0+） | `http://<relay-host>/r/<sid>` | `/api/*` 与 `/presenter/api/*`（§7） | CLAB 中继 + WS 隧道转发到导播 |

主持人页 HTML 用 `<base href>` 把所有相对路径锚定到对应前缀；**presenter.js 不感知部署模式**——
所有调用都形如 `fetch("api/status?token=...")`。

### 1.2 共通规则

- **鉴权**：每个请求都要带 `token`（应用启动时随机生成，可在 Settings 重置）。
  - **GET** 请求：token 放 **query**，如 `?token=xxxx`。
  - **POST** 请求：token 放 **JSON body**。
  - Relay 入口从 **query** 读取 token，因此 Relay POST 请求需同时在 query 和 JSON body 中携带 token。
  - **0.2.7 Relay 补充修复：** 初始 Relay 附件的手机 POST 仅传 JSON token，会被中继以 401 拒绝。当前源码的统一 `api()` 已在 Relay POST 中补上 query token，并保留 JSON token；GET 和 LAN POST 不变。部署需使用 `post-token-fix` 附件、重启中继并刷新手机页面，Windows 0.2.7 安装包无需重装。
- **请求体**：POST 一律 `Content-Type: application/json`，64 KiB 上限（LAN 的 `DefaultBodyLimit` 与 Relay 隧道两端一致）。
- **响应体**：除截图(二进制 JPEG)、MJPEG 流、页面/静态资源外，均为 JSON。
- **单控制端**：点击/翻页/黑屏/恢复/宏/锁/解锁额外要求请求体 `client_id` 等于
  `active_client_id`（先到为主、控制端 >10s 无心跳后可被接管；v0.2.4 起由 4s 放宽，一次 Wi‑Fi 抖动不丢控制权）。
- **幂等 `op_id`**（v0.2.4+）：六个动作端点（`click`/`next`/`prev`/`black`/`restore`/`action`）的请求体可带
  `"op_id": "<client_id>-<seq>"`。服务端保留最近 64 条、60s 内的结果：同 `op_id` 已执行完 → 原样回放上次响应；
  仍在执行 → `409 op_in_flight`。不带 `op_id` 行为不变。**门控失败（401/403）不进缓存**，解锁后同 id 可再执行。
  手机端在网络错误 / 超时 / 502 / 503 时用同一 `op_id` 重发（最多 3 次），任何其它状态码都视为"已送达"停止重发。收到响应只表示请求已送达，不等于执行成功。
- **COM 忙碌处理（v0.2.7）**：状态探测与 COM 操作共用 `ppt_gate`。主动 COM 操作最多等待通道释放 500ms；状态探测拿不到通道时直接返回 `busy:true`。手机发送操作期间暂停新探测，状态心跳继续。宏请求等待后仍未获得通道时返回 `409 ppt_busy`，本次宏未执行。

### 1.3 信任与三层锁

| 操作类别 | 放行条件 |
| --- | --- |
| 看状态 / 看画面（snapshot/stream）/ 看 slide_state / 看 messages | token 正确即可 |
| ARM（unlock）| `button_allowed = remote_enabled && !control_locked` + `is_controller` |
| LOCK（lock）| `is_controller`（只需控制端身份；任何时候都能主动锁） |
| 主持人动作（next / prev / black / restore / action / click）| `click_allowed = remote_enabled && !control_locked && presenter_unlocked` + `is_controller` |
| 发消息（message）| token 正确即可（不要求 controller，方便观察席给导播说话） |

锁的来源、默认值与"导播总是强势"的语义见 [`architecture.md` §5](architecture.md)。

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

- 请求：`?token=xxxx&cid=host-tablet`；`cid` 用于登记心跳和认领控制权，不传时 `is_controller` 为 false。
- 成功：`200 application/json`

```json
{
  "ok": true,
  "ppt_window_found": true,
  "remote_enabled": true,
  "control_locked": false,
  "presenter_unlocked": true,
  "is_controller": true,
  "director_message": null,
  "timer": null,
  "server_time": 1710000000
}
```

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `ppt_window_found` | bool | 是否已选中目标窗口 |
| `remote_enabled` | bool | 导播总开关（锁层 1） |
| `control_locked` | bool | 导播强制锁（锁层 2） |
| `presenter_unlocked` | bool | 主持人本地解锁（锁层 3） |
| `is_controller` | bool | 本次请求的 `cid` 是否为当前控制端，不暴露其他设备的 ID |
| `director_message` | object \| null | 最近一条导播消息，包含 `seq`、`text` 和 `at_ms` |
| `timer` | object \| null | 当前计时状态；未设置时为 null |
| `server_time` | number | 服务端 Unix 秒 |

- 失败：`401 {"ok":false,"reason":"invalid_token"}`

### 2.4 `GET /api/snapshot.jpg`

当前画面一次性截图。**已不是默认推送路径**——保留为兜底/调试用，正式画面走 §2.5 的 MJPEG 流。

- 请求：`?token=xxxx`
- 成功：`200 image/jpeg`，`Cache-Control: no-store`（服务端固定 1280×720、质量 70）
- 失败：`401 {"ok":false,"reason":"invalid_token"}`；`500`（纯文本 `capture error: ...`）
- 说明：未选窗口时返回占位图。GDI 编码在 `tokio::task::spawn_blocking` 中执行，不阻塞 async worker。

### 2.5 `GET /api/stream.mjpg`

**主流推送路径**：变化驱动的 `multipart/x-mixed-replace` MJPEG 长连。

- 请求：`?token=xxxx`
- 成功：`200 multipart/x-mixed-replace; boundary=frame`，`Cache-Control: no-store`
- 失败：`401 {"ok":false,"reason":"invalid_token"}`
- 段格式：
  ```
  --frame\r\n
  Content-Type: image/jpeg\r\n
  Content-Length: <N>\r\n
  \r\n
  <N 字节 JPEG>\r\n
  ```
- 语义：订阅 frame_broker（`tokio::sync::watch`），连上立即推一帧当前最新（若有），之后每次帧变化推一帧；
  空帧（seq 0 占位）跳过。空闲时（无变化）零流量。手机断 → `WatchStream` drop → 订阅自动释放。
- 实现：[`architecture.md` §4.1](architecture.md) + [ADR-0006](adr/0006-capture-stream-broker.md)。

### 2.6 `GET /api/slide_state`

当前 PPT 放映窗口的页码 / 总页数 / 状态 / 该页绑定的宏 shape。

- 请求：`?token=xxxx`
- 成功：`200 application/json`

```json
{
  "available": true,
  "busy": false,
  "in_show": true,
  "current": 3,
  "total": 12,
  "actions": [{ "id": "btnA", "label": "A 选项", "macro_name": "Choose" }, { "id": "btnB", "label": "B 选项", "macro_name": "Choose" }]
}
```

- 字段：`available` = PowerPoint 可达；`busy` = COM 通道被占用或本次读取超时；`in_show` = 在放映态；`actions` = 当前页 `Shape.ActionSettings(ppMouseClick).Action == 8`（运行宏）的 shape 列表；`id` = shape name，传给 §2.10 的 `action`；`macro_name` 供显示和日志使用。
- 失败：`401 invalid_token`。PowerPoint 不可达时仍返回 200，`available:false`；忙碌时为 `available:false,busy:true`，不等于网络断开。读取超过 2 秒返回忙碌状态，后台 COM 调用仍持有通道直到实际结束。
- 手机继续轮询此状态。`busy:true` 时禁用宏、黑屏和恢复，保留屏幕点击与基础翻页；基础翻页激活选中窗口后模拟方向键，不经过 COM 通道。所有操作仍须通过原有权限与锁定检查。

### 2.7 `POST /api/click`

在预览图上点击 → 归一化坐标 → 真实屏幕点击。

- 请求体：

```json
{ "token": "xxxx", "client_id": "host-tablet", "x": 0.723, "y": 0.614 }
```

| 字段 | 类型 | 必需 | 说明 |
| --- | --- | --- | --- |
| `token` | string | 是 | 鉴权 |
| `client_id` | string | 是 | 客户端标识（用于单控制端门控） |
| `x` / `y` | number | 是 | 归一化坐标，范围 `[0,1]` |

- 成功：`200`

```json
{ "ok": true, "screen_x": 1388, "screen_y": 663 }
```

- 映射：`screen = (left + round(x*width), top + round(y*height))`（见 [`architecture.md` §6](architecture.md)）。
- 放行条件：`click_allowed` + `is_controller`。
- 失败（见 §3）：`invalid_token` / `not_controller` / `coordinate_out_of_range` / `control_locked` / `ppt_window_not_found` / `input_failed`。

### 2.8 `POST /api/next` · `POST /api/prev`

翻页（下一页 / 上一页）。

- 请求体：`{ "token": "xxxx", "client_id": "host-tablet" }`
- 成功：`200 {"ok":true}`
- 放行条件：`click_allowed` + `is_controller`。
- 实现：激活选中窗口后调用 `input::key_next/prev`（SendInput），不等待 COM 通道。
- 失败：`invalid_token`(401) / `not_controller`(403) / `control_locked`(403) / `input_failed`(500)。

### 2.9 `POST /api/black` · `POST /api/restore`

PowerPoint 黑屏 / 恢复（B 键效果）。

- 请求体：`{ "token": "xxxx", "client_id": "host-tablet" }`
- 成功：`200 {"ok":true}`
- 放行条件：`click_allowed` + `is_controller`。
- 实现：`ppt_com::set_black(true/false)`（**走独立 COM 通道，与窗口选择/截图无关**，原 OBS 方案已废弃）。
- 失败：`invalid_token`(401) / `not_controller`(403) / `control_locked`(403) / `503 black_failed` / `503 restore_failed`。

### 2.10 `POST /api/action`

触发当前页一个宏 shape（由 §2.6 `slide_state` 返回的 `id` 指定）。

- 请求体：`{ "token": "xxxx", "client_id": "host-tablet", "shape": "btnA" }`
- 成功：`200 {"ok":true}`。实际宏名记录在导播日志中；调用正常返回不能证明宏内部的动画或玩法操作成功。
- 放行条件：`click_allowed` + `is_controller`。
- 实现：服务端**根据 shape name 重新读绑定的宏名**（不信任客户端传入），调 `Application.Run(macro, shape)`；
  仅在 `DISP_E_BADPARAMCOUNT`（参数数量被拒绝）时回退无参调用，运行时异常不自动重跑宏。
- 失败：`invalid_token`(401) / `not_controller`(403) / `control_locked`(403) / `409 ppt_busy` / `409 op_in_flight` / `500 action_failed` / `504 action_timeout`。
- `ppt_busy` 表示本次宏未执行；稍后重新按按钮会生成新的 `op_id`。`op_in_flight` 表示同一次请求仍在执行，不应再发起新操作。宏执行超过 8 秒返回 `action_timeout`，结果未知，后台调用与通道占用会保留到实际结束。

### 2.11 `POST /api/lock` · `POST /api/unlock`

主持人端本地锁定 / 释放控制。

- 请求体：`{ "token": "xxxx", "client_id": "host-tablet" }`
- `lock`：置 `presenter_unlocked = false`。放行条件：`is_controller`（任何时候控制端都能主动锁）。
- `unlock`：置 `presenter_unlocked = true`。放行条件：`button_allowed` + `is_controller`。
- 失败（unlock）：`invalid_token`(401) / `not_controller`(403) / `remote_disabled`(403)。
- 说明：导播端强制锁 / 关闭远程优先级更高，会立即清掉 `presenter_unlocked` 并撤销主持人 ARM 状态。

### 2.12 `GET /api/messages`

读取共享预制消息池（由导播在 Settings 内编辑、持久化）。

- 请求：`?token=xxxx`
- 成功：`200 application/json`

```json
{ "items": [{ "id": "msg-1", "text": "请准备下一段" }, { "id": "msg-2", "text": "时间到" }] }
```

- 失败：`401 invalid_token`。

### 2.13 `POST /api/message`

主持人按预制 id 发一条消息给导播（导播侧弹"软"toast 通知）。

- 请求体：`{ "token": "xxxx", "client_id": "host-tablet", "id": "msg-1" }`
- 成功：`200 {"ok":true}`
- 放行条件：仅 token（**不要求** is_controller，方便观察席/副导播说话；text 由 id 在服务端从池里查得，
  客户端不能传任意 text）。
- 失败：`invalid_token`(401) / `400 unknown_message`。

---

## 3. 错误码

失败响应统一为 `{"ok": false, "reason": "<code>"}`（§2.1/§2.4/§2.5 的页面/二进制/流式端点除外）。

| HTTP | `reason` | 触发 |
| --- | --- | --- |
| 401 | `invalid_token` | token 缺失或不匹配 |
| 400 | `coordinate_out_of_range` | `x` 或 `y` 不在 `[0,1]` |
| 400 | `unknown_message` | `/api/message` 指定的 `id` 不在预制池里 |
| 403 | `not_controller` | 单控制端门控未通过（`client_id` ≠ `active_client_id`） |
| 403 | `control_locked` | 三层锁未放行（动作类） |
| 403 | `remote_disabled` | unlock 时远程未开启或被强制锁 |
| 409 | `ppt_window_not_found` | 未选中目标窗口 |
| 409 | `op_in_flight` | 同 `op_id` 的请求仍在执行（重发到得太快）；已送达，勿再重发 |
| 409 | `ppt_busy` | `/api/action` 等待 COM 通道释放 500ms 后仍忙，本次宏未执行 |
| 500 | `input_failed` / `action_failed` | 翻页、坐标点击或宏执行失败，详见导播日志 |
| 503 | `black_failed` / `restore_failed` | 黑屏或恢复失败 |
| 504 | `action_timeout` | 宏执行超过 8 秒，结果未知；后台仍可能继续执行 |

### 3.1 Relay 模式额外错误码

仅在 Relay 部署下由中继返回，导播侧的业务错误透传不变：

| HTTP | `reason` | 触发 |
| --- | --- | --- |
| 401 | `invalid_token` | relay 边缘 token 校验未过 |
| 413 | `body_too_large` | 请求体超过 64 KiB |
| 502 | `session_unavailable` | 该 `session_id` 没有活跃的导播 WS |
| 502 | `director_offline` | 导播 WS 已断、消息推不出去；或等待期间导播被判半开 / 被新连接接管，pending 请求被立即失败（v0.2.4+） |
| 504 | `tunnel_timeout` | 隧道往返超过 30s（导播在线但迟迟不回,典型是 VBA 弹窗） |

**存活探测（v0.2.4+）**：relay 每 10s 向导播控制 WS 发 JSON `Ping{nonce}`、向推流 WS 发协议层 Ping；任一方向 25s 没有任何入站帧即判半开、主动断开。导播侧同样 25s 读超时 + 8s 握手超时后走退避重连（0.5→5s + 抖动）。这些常量固定在 `protocol` crate，不进 relay 配置。

---

## 4. 坐标约定

主持人端**永远发送归一化坐标** `{x,y} ∈ [0,1]`，不发设备像素（设备尺寸 / 缩放各异）。
服务端校验范围后按目标窗口矩形映射为屏幕坐标。公式与风险见 [`architecture.md` §6](architecture.md)
与 [`project.md` §7](project.md)。

---

## 5. 实现状态

| 项 | 现状 |
| --- | --- |
| 画面传输 | ✅ 已实现 `GET /api/stream.mjpg`（v0.1.16；`snapshot.jpg` 保留兜底） |
| 黑屏 / 恢复 | ✅ 已实现（PowerPoint COM `set_black`；原 OBS 方案已废弃） |
| 宏触发 | ✅ 已实现 `POST /api/action`（COM `Application.Run`，shape 由服务端反查宏名） |
| 单控制端独占 | ✅ 已实现 `is_controller` 门控控制动作（先到为主、>10s 无心跳可被接管；消息接口除外） |
| 快捷消息 | ✅ 已实现 `GET /api/messages` + `POST /api/message`（id-only，text 服务端反查） |
| Relay 模式 | 已实现；0.2.7 需使用 `post-token-fix` 中继附件，鉴权说明见 §1.2 |
| Relay session 宽限期回收 | ✅ 每 10 秒检查，控制连接已断且超过宽限期的会话会被回收（默认宽限期 60 秒） |

---

## 6. LAN 端 `/presenter/api/*` 别名

为了让 presenter.html 在 LAN 和 Relay 两种 `<base href>` 下用同一份相对路径，LAN 端**为每个 `/api/<endpoint>` 都额外注册了 `/presenter/api/<endpoint>` 别名**（路由等价、handler 同一份）。`<base href="/presenter/">` + 相对 `api/status` → 命中 `/presenter/api/status` → 等同 `/api/status`。

- **业务上完全等价**，只是前缀不同。
- 旧的裸 `/api/*` 路由保留，向后兼容已发出的二维码 / 书签 / 工具调用。

---

## 7. Relay 模式路由族

Relay 部署下（详见 [`architecture.md` §11](architecture.md)），手机面对的 base URL 是
`http://<relay-host>/r/<session_id>`，端点对应表：

| 类别 | Relay 端 URL | 关系 |
| --- | --- | --- |
| 主持人页 HTML | `GET /r/<sid>/presenter` | relay 本地服务（注入 `<base href="/r/<sid>/presenter/">`） |
| 静态资源 | `GET /r/<sid>/presenter/{presenter.css, presenter.js, favicon.svg}` | relay 本地服务（`include_str!` 编进 binary） |
| 推流 | `GET /r/<sid>/api/stream.mjpg` 与 `GET /r/<sid>/presenter/api/stream.mjpg` | relay 本地复刻 watch 扇出，二进制 WS 从导播取帧 |
| 其余 API（status / snapshot / click / next / prev / black / restore / action / lock / unlock / messages / message / slide_state） | `* /r/<sid>/api/<endpoint>` 与 `* /r/<sid>/presenter/api/<endpoint>` | **隧道**：relay 边缘 token 校 → `TunnelReq` 经 WS 推给导播 → 导播喂自己的 `axum::Router` → 响应原路回送 |

**§2 描述的语义在隧道两端完全保持**：导播侧 `routes.rs` 是同一份 handler。Relay 仅做：
(a) token 边缘 401；(b) 64 KiB body 上限 → 413；(c) session 不可达 502；(d) 30s 隧道超时 504。
业务错误（`control_locked` / `not_controller` / `ppt_window_not_found` …）由导播返回，relay 透传。

会话注册协议（导播 ↔ relay 的两条 WS 上的 `ControlMessage` schema）见
[`crates/protocol/src/lib.rs`](../crates/protocol/src/lib.rs)
与 [ADR-0007](adr/0007-relay-hybrid-tunnel.md)。

---

## 关联

- 设计与错误原因背景：[`project.md` §9](project.md)
- 实现：`src-tauri/src/routes.rs`（LAN 端）+ `crates/relay/src/phone_routes.rs`（Relay 端）
- 架构与状态模型：[`architecture.md`](architecture.md)
