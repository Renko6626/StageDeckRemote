# Relay 架构 设计 (方案 ③ — 混合:HTTP 隧道控制 + 二进制 WS 推流)

**日期:** 2026-06-25
**状态:** 已批准(待写实现计划)
**范围:** 在现有 LAN 直连之外,新增一条经过校内中继服务器(CLAB Linux 主机)的传输路径,解决场馆 WiFi 的"对等阻断/客户端隔离"等不可靠场景。**LAN 模式继续保留**。

---

## 1. 背景与目标

**问题:** 当前架构是"导播开 Axum listener,手机扫码直连"。场馆 WiFi 常见的 client isolation / 对等阻断让两端虽都连上 SSID 但互相看不到对方。

**已确认边界:**
- 中继服务器**仅校园网内可见**(CLAB 提供校内 IP 的 Linux VM)→ 导播和手机都必须能进校园网。无公网暴露需求。
- LAN 直连保留,新模式**并存**;走哪条路由二维码 URL 决定,手机零模式感知。
- 主持人端仍是"任意手机扫码免装"。
- 内容仍以静态幻灯片为主;沿用现有 broker 的"变化才推一帧"语义。

**核心设计取舍:** 业务逻辑零改动 + 推流通道走最适合二进制扇出的链路 + 手机端协议完全不变。

## 2. 非目标 (YAGNI)

- 不上 TLS(校内 HTTP,v2 再说)
- 不做多租户账户体系、不开公网
- 不引入 WebRTC / P2P NAT 穿透(打破"手机零客户端"约束)
- 不动三层锁、token 模型、单控制端语义、计时器、消息预制
- relay 端不做持久化(重启等于 sessions 全部重连)

## 3. 拓扑

```
[导播 Tauri / Windows]           [Relay / CLAB Linux]              [手机浏览器]
 - LAN Axum listener (留)         - Rust binary (axum)               (无任何 App)
 - 新: relay_client (WS)    ──持久 WS 控制隧道──▶                  ──HTTP──▶
 - broker watch::Sender (已有)    - session 路由表                    - GET /r/<sid>/presenter
                            ──持久 WS 推流通道──▶                    - GET /r/<sid>/api/stream.mjpg
                                  - 复刻一份 frame watch 扇出         - POST /r/<sid>/api/...
```

**关键概念:** Relay 是"透明 HTTP 终端 + 隧道路由器"。手机以为自己在跟一个普通 HTTP 服务对话,完全不知道有 relay 存在;Relay 本身**不持业务状态、不验业务锁**,只做会话路由与帧扇出。

## 4. 会话与 QR

### 4.1 会话注册
- 导播 GUI Settings 开启"Relay 模式"并填:relay base URL、`session_id`(导播自填的短码,如 `PROM-2026`,正则 `[A-Z0-9-]{4,16}`)、`director_secret`(预共享密钥)。
- 启动 / 开启 relay 模式时,`relay_client` 打 `WS /director/control`,首帧 `ClientHello{ session_id, token, director_secret, version }`。
- Relay 端 `(session_id → director_secret)` 表:
  - 首次出现的 session_id:接收、绑定 secret。
  - 已存在:仅当 secret 一致才接收(reconnect 用),否则 reject(挡撞名)。
- 然后导播再打 `WS /director/stream`(同样的 `ClientHello`),用于推帧。
- 注册成功 → relay 在内存为该 session 创建:control 路由项 + 帧 `watch::Sender<Arc<Vec<u8>>>`(扇出给手机的 MJPEG handler)+ token 缓存。

### 4.2 QR
- `DirectorStatus` 多返回 `lan_access_url` (今天的) + `relay_access_url`(Relay 模式下,形如 `http://stagedeck.clab.pku.edu.cn/r/<sid>/presenter?token=<tok>`)。
- `QrCodePanel` 加切换:LAN / Relay。导播按场地挑一个生成。**手机不知情**。

## 5. 控制平面 — HTTP 隧道

**关键洞见:** `routes.rs::router()` 已是 `axum::Router`,本质 `tower::Service<Request> → Response`。我们**不绑端口、不 listen**,直接喂请求拿响应:

数据流:
```
手机 ─HTTP─▶ Relay /r/<sid>/api/next
              │ 1. token_ok(边缘快速校验)
              │ 2. 查 sid → 拿到 director 的 control WS
              │ 3. TunnelReq { req_id, method:"POST", path:"/api/next",
              │                 query, headers, body } 经 WS 推过去
              ▼
            Director relay_client
              │ 4. 反序列化为 http::Request
              │ 5. router.call(req).await → Response  ←── 业务零改动
              │ 6. TunnelResp { req_id, status, headers, body } 回送
              ▼
            Relay 翻成 HTTP Response 给手机
```

**保留 `token_ok` 在 Director 侧再校一次**(纵深防御)。锁/控制权/cid/计时/消息预制**全部在 router 内部,relay 完全不感知**。

### 5.1 控制 WS 消息(JSON 文本帧)
```jsonc
// Director → Relay (首帧)
{ "type": "ClientHello", "session_id": "PROM-2026", "token": "...", "secret": "...", "version": 1 }
// Relay → Director (回执)
{ "type": "ServerHello", "ok": true, "reason": null }
// Relay → Director (转发手机请求)
{ "type": "TunnelReq", "req_id": 42, "method": "POST", "path": "/api/next",
  "query": "", "headers": [["content-type","application/json"]], "body_b64": "..." }
// Director → Relay (回响应)
{ "type": "TunnelResp", "req_id": 42, "status": 200, "headers": [...], "body_b64": "..." }
// Director → Relay (token 轮换)
{ "type": "TokenUpdate", "token": "..." }
// Relay → Director (心跳;客户端 30s 不响应就断)
{ "type": "Ping", "nonce": 123 } / { "type": "Pong", "nonce": 123 }
```
Body 用 base64 表达任意字节(简化 schema 调试 / 兼容文本与 JSON 体)。

### 5.2 请求/响应大小上限
- TunnelReq body:复用现有 `DefaultBodyLimit::max(64 KiB)` 同等数量级;relay 边缘拒绝超大请求,不浪费隧道。
- TunnelResp body:控制类响应都是小 JSON,无须特殊上限;若超过则截断报错。

## 6. 推流平面 — 二进制 WS + relay 端复刻 MJPEG

**最巧的点:手机端协议零改动**,还是 `<img src=".../api/stream.mjpg?token=...">`。

数据流:
```
Director 编码线程 broker.publish(jpeg)
   ▼
Director relay_client 推流任务: rx.changed() → WS Binary frame(纯 JPEG 字节,无包装)
   ▼
Relay 收到二进制帧 → 更新该 session 的 "frame watch::Sender" → 扇出
   ▼
手机 GET /r/<sid>/api/stream.mjpg
   - relay handler 几乎复制 routes.rs::stream_mjpeg,
   - 但订阅源从 state.capture.subscribe() 换成 "该 session 的 watch_rx"
   ▼
multipart/x-mixed-replace 流给手机
```

- WS 推流帧是**纯 JPEG 字节**,无 envelope —— WS 本身已分帧,简化最大化。第一帧之外不发心跳(相邻几秒没帧很正常)。
- 推流 WS 上 director 唯一发送的非二进制是首帧 `ClientHello`(JSON);收到 `ServerHello` 之后就只发二进制。
- Relay 侧用 `tokio::sync::watch<Arc<Vec<u8>>>`,语义跟你刚做的 broker 一致;手机断 = WatchStream drop = 自动 unsubscribe。

## 7. 鉴权

- Token 仍由 Director 生成。`ClientHello` 报给 Relay,Relay 缓存 `session → token`。
- **Relay 边缘校 token**(快速 reject)+ **Director router 内 `token_ok` 再校**(纵深)。
- MJPEG handler 由 Relay 直接服务 → token 在 relay 验。
- `regenerate_token` 触发 `TokenUpdate` 推到 relay → 缓存更新;旧 token 立刻 401。
- Director 与 Relay 的 WS 鉴权:`director_secret` 预共享。
- 无公网暴露 → v1 不强求 TLS。Relay config 留 TLS 开关,v2 用 `rustls`/Caddy 反代加。

## 8. 双模并存(LAN + Relay)

- `AppState` 新增 `relay_config: Option<RelayConfig>` 与 `relay_status: RelayStatus`(Disconnected/Connecting/Connected/Error)。
- LAN Axum listener **永远跑**。Relay 客户端是**附加**任务,可独立开关。
- Settings 模态加:Relay 开关 + base URL 输入框 + session_id 短码 + secret + 状态指示灯。
- `DirectorStatus` 多返回 `lan_access_url`, `relay_access_url`(后者仅 Relay 已连成功时非空)。
- `QrCodePanel` 加 LAN/Relay 切换 toggle;默认 LAN。
- **核心收益:同一份 `router` 被两条 transport 复用** —— LAN 侧 `web_server::run` 当 listener、Relay 侧 `relay_client` 当 dispatcher,都喂同一个 router 实例(`Arc<Router>`)。业务零 awareness。

## 9. 生命周期 / 失败模式

### 9.1 Director ↔ Relay 链路
- 重连:指数退避 0.5s → 5s 上限 + jitter;一次成功消息后退避归零。
- 控制 WS 与 流 WS 独立维护(任一断不影响另一;断的一边自己重连)。
- 重连后立刻 re-`ClientHello`;Relay 用 secret 校验同一 session 接管。

### 9.2 Relay 端 session 寿命
- 控制 WS 断 → 该 session 进入"待恢复"宽限期 60s;期间手机请求 → `502 Bad Gateway`。
- 流 WS 断 → 不影响控制;手机 MJPEG 流维持连接但无新帧,前端有 `error→1s 重连` 已覆盖。
- 宽限期超过 → 清 session(包括 token 缓存、frame watch);此后 QR 失效,需导播重启 relay 模式。

### 9.3 手机端
- 控制类 502/503:复用现有 `handle401` 风格 → 退避慢轮询、显式 banner 提示"中继断开,正在重连"。
- MJPEG 流断:已有的 `error→1s startStream` 自愈逻辑直接覆盖。
- Token 轮换(在 Relay 已传达后)→ 旧 token 走老的 401 自愈路径,免新做。

### 9.4 Relay 进程崩溃/重启
- 所有 sessions 即时失效。Director 重连 → re-register → 重建 session(空 broker、token 重报)。
- v1 接受:期间手机看到流断 + 502,自愈到下一次连上。

## 10. 文件/模块结构(影响面)

### 10.1 Cargo workspace 引入
为共享 WS 消息类型,引入 workspace:
```
Cargo.toml                  (新,workspace 根)
src-tauri/                  (已有,作为 workspace member)
crates/
  protocol/                 (新,小 crate;TunnelReq/Resp/ClientHello… serde types)
  relay/                    (新,relay binary 的 cargo crate)
```

### 10.2 Director 侧(`src-tauri/src/`)
| 文件 | 改动 |
|---|---|
| `relay_client.rs`(新) | WS 控制 + WS 流 两个 reconnect 循环;`router.call(req)` 分发器;`broker.subscribe()` 推流任务 |
| `app_state.rs` | 加 `relay_config`/`relay_status` 字段 |
| `commands.rs` | 加 `set_relay_config`/`relay_status` 命令;`DirectorStatus` 增 `lan_access_url`/`relay_access_url` |
| `config.rs` | 持久化 `relay_config` (base_url, session_id, secret, enabled) |
| `lib.rs` | 启动时按 config 决定是否 spawn relay_client;router 改为 `Arc<Router>` 给两边共享 |
| `routes.rs` | 无业务改动(router 工厂保留);仅确保 router 可作为独立 `Service` 复用 |

### 10.3 Presenter 侧 — base URL 处理
- 现状:presenter.html/js 用**绝对路径** (`/presenter/presenter.css`, `/api/...`)。Relay 下要挂在 `/r/<sid>/` 子路径下 → 必须改成相对或用 `<base>` 标签。
- **选定:`<base href="...">` 由服务端注入**(LAN 侧 `<base href="/">`,Relay 侧 `<base href="/r/<sid>/">`)。HTML 模板里留占位 `__BASE__`,routes.rs 与 relay 各自替换。
- presenter.js 内所有 `"/api/..."` → `"api/..."`(相对) → 跟 `<base>` 拼接生效。
- 后果:**LAN 模式行为完全不变**(`<base href="/">`+ `"api/..."` 等价于今天的 `"/api/..."`)。

### 10.4 Relay 侧(`crates/relay/src/`)
| 文件 | 责任 |
|---|---|
| `main.rs` | tokio runtime + config 加载 + `axum::serve` |
| `session.rs` | `SessionTable`: `sid → DirectorHandle`(control_tx, frame_watch_sender, token, secret) |
| `director_ws.rs` | `/director/control` + `/director/stream` 升级 handler;`ClientHello` 校 secret;收 `TunnelResp` 路由回 oneshot |
| `phone_routes.rs` | `/r/:sid/presenter`(注入 `<base>`)、`/r/:sid/presenter/presenter.css/js`、`/r/:sid/api/*`(隧道)、`/r/:sid/api/stream.mjpg`(本地扇出) |
| `tunnel.rs` | 手机 HTTP req → `TunnelReq`(分配 `req_id`、oneshot 返回);超时 30s |
| `config.rs` | toml 配置(listen addr、允许的 session secret 白名单或 open mode) |

### 10.5 Shared (`crates/protocol/src/`)
- `enum ControlMessage { ClientHello, ServerHello, TunnelReq, TunnelResp, TokenUpdate, Ping, Pong }`
- `RelayConfig` 公共字段定义。
- serde + tagged JSON。

## 11. 部署(CLAB)

- `cargo build --release -p relay` → 静态二进制(musl 可选);scp 到 CLAB。
- `/etc/systemd/system/stagedeck-relay.service` 跑 binary,journald 日志,`After=network-online.target`,`Restart=on-failure`。
- 配置 `/etc/stagedeck-relay/config.toml`:`listen = "0.0.0.0:80"`(或 8080)、`max_sessions`、可选 `auth.secret_required = true` 强制所有 session 携 secret。
- 反代 / TLS(v2):前置 Caddy 拿校内证书(若 PKU 有内网 CA);v1 直接裸 HTTP。
- 日志:每分钟一次 session 数 / 流帧速率 summary 到 stdout。

## 12. 测试策略

### 12.1 Linux 可单测
- `protocol`:消息 (de)serialize roundtrip。
- Director `relay_client`:从 fake control_tx 喂 `TunnelReq` → 喂 router → 拿响应字节;mock broker 喂帧 → 验证 stream_tx 收到对应字节。
- Relay `session.rs`:注册/查找/secret 校验/过期。
- Relay `tunnel.rs`:`req_id` 分发器(oneshot 完成、超时清理)。
- Relay `phone_routes.rs::stream_mjpeg`:复刻 frame_broker 的 publish→subscribe-latest 测试。

### 12.2 集成(Linux 本机,全栈无 Windows)
- 启动 relay → 用一个 mock director(直接拿 `src-tauri` 的 router + 一个 dummy `CaptureManager` 喂帧)→ 用 `curl`/`reqwest` 充当手机,跑控制矩阵(token/锁/cid/401/403/502) + 拉一段 MJPEG 流验证段头与负载。

### 12.3 端到端(真机)
- CLAB 部署 + Windows 跑导播(配 relay) + 手机走校园 WiFi 扫码。

## 13. 风险 / 已知限制

- **校园网认证 / captive portal**:手机首次扫码若未连过校园 WiFi,会被门户拦截。**协议层不解决,文档建议主持人提前连好校园 WiFi**。
- **Relay 重启对正在进行的演出是硬中断**(无快照);可接受,后续可加 snapshot/replay。
- **CLAB 本身是单点**;v1 不做多 relay 冗余。
- **HTTP 隧道延迟比 LAN 直连多一跳**(预期 +5~20ms,可接受;静态幻灯片不敏感)。
- **JSON+base64 控制通道有 ~33% 体积放大**;请求体本身小,可忽略。后续可换 bincode/MessagePack。

## 14. 关键设计取舍小结

| 取舍点 | 选定 | 替代 / 拒绝原因 |
|---|---|---|
| Relay 角色 | 混合(隧道控制 + 二进制 WS 推流) | 纯 WS pubsub:业务代码翻面;纯隧道:推流低效 |
| Session ID | 导播自填短码 | Relay 分配:不利于人记忆与签到协调 |
| Token 模型 | Relay 缓存 + 边缘 + 纵深 | 零信任(全转发由导播 401):浪费往返、流 handler 没法本地校 |
| 序列化 | JSON+base64(控制)+ 纯二进制(流) | bincode:不易调试;留 v2 |
| 双 transport | router 复用 `Arc<Router>` | 拆两套 handler:违背"业务零改动" |
| Cargo 结构 | workspace + `protocol` 共享 crate | 复制 schema:漂移风险 |
| TLS | v1 不上 | 校内裸 HTTP 可接受;v2 Caddy 反代 |
