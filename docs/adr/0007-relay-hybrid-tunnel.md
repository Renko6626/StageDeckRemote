# ADR-0007：Relay 架构——混合 HTTP 隧道 + 二进制 WS 推流 + 双模并存

- 状态：Accepted（2026-06-25）
- 相关：[architecture.md §11](../architecture.md)、[protocol.md §6/§7](../protocol.md)、
  [network.md §7](../network.md)、spec
  [`superpowers/specs/2026-06-25-relay-architecture-design.md`](../superpowers/specs/2026-06-25-relay-architecture-design.md)

## 背景

LAN 直连(导播 Axum + 主持人扫码同 WiFi)在校园活动场地经常不靠谱:
**客户端隔离(AP isolation)** 让两台都在 SSID 里的设备互相看不到、**强制门户**拦扫码、
**场馆 WiFi 出口**抖。我们需要一条**当 LAN 不可信时也能用**的备选路径,
而手头资源是 PKU CLAB 上的一台校园网内 Linux VM。

约束:
- **主持人端必须保持"任意手机浏览器扫码、不装 App"**——任何要装客户端的方案(Tailscale 等)出局。
- **LAN 模式必须保留**:能用就用,延迟更低、不依赖第三方。
- 静态幻灯片画像下,**已有的 frame_broker(ADR-0006)** 是该被复用的扇出原语。

## 决策

把 relay 设计成**透明 HTTP 终端 + 隧道路由器**,**手机不知道有 relay**(协议字面跟直连一致):

```
[导播 Tauri / Windows]            [Relay / 校内 Linux]              [手机浏览器]
 - LAN Axum :8787 (留)             - Rust binary (axum 0.7)            扫码即用
 - relay_client (新)        ──持久 WS 控制隧道──▶                    ──HTTP──▶
 - frame_broker (watch)     ──持久 WS 推流通道──▶                    ──MJPEG─▶
                                   - 会话表 + 隧道分发
                                   - watch 复刻一份扇出
```

### §1. 控制平面 = HTTP 隧道

手机 `POST /r/<sid>/api/next?token=...` → relay 边缘 token 校验(快速 reject)→ 在该 session 的 control WS 上推
`TunnelReq{req_id, method, path, query, headers, body_b64}` 给导播 → 导播 `relay_client` 反序列化成
`axum::http::Request`,**喂给现有 `routes::router()`**(`tower::ServiceExt::oneshot`),拿响应原路回送
`TunnelResp{req_id, status, headers, body_b64}` → relay 翻回 HTTP 给手机。

**关键收益:`routes.rs`/`commands.rs`/锁/cid/控制权这套业务代码零改动**。轮子(transport)换了,
车架(业务)没动。

### §2. 推流平面 = 二进制 WS

导播侧多一个 task 订阅 ADR-0006 的 broker `state.capture.subscribe()`,每来新帧
`rx.borrow().jpeg.clone()` 当 WS 二进制帧上送 relay。relay 端**复刻一份** `watch::Sender<Arc<Vec<u8>>>`
扇出给所有连本会话的手机,对手机吐 `/r/<sid>/api/stream.mjpg` —— **MJPEG 协议跟 §2.5 完全一致**,
手机端协议零改动。

### §3. 复用 ADR-0006 的 broker 抽象

导播 broker 已经是 `watch::Sender<Arc<EncodedFrame>>`,扇出语义本身就支持多消费者。
**Relay 多加一个消费者(WS 上行到 relay)是它的自然延伸,业务零感知**。
relay 端的"watch + filter_map + WatchStream + multipart"几乎是 `routes.rs::stream_mjpeg` 的逐字复刻。

### §4. 会话注册 + 鉴权

- 导播自填短码 `session_id`(正则 `[A-Z0-9-]{4,16}`),配 `secret`(预共享密钥)防撞名。
- `ClientHello { session_id, token, secret, version }` 经控制 WS 发出;`ServerHello { ok }` 回执。
- relay 缓存 `session → token`,**边缘做快速 401**(避免无谓的隧道往返);
  导播 router 内部 `token_ok` 仍纵深校验(零信任 vs 性能的权衡:边缘 + 纵深都有)。
- token 轮换 → director 经 `relay_notify` 推 `TokenUpdate { token }` → relay 更新缓存。
- 推流 WS 复用同一 `ClientHello`(secret 防错接到别人的 session)。

### §5. 双模并存

- LAN Axum listener **永远跑**;`relay_client` 是**附加任务**,看 `AppState.relay_config` 是否 Some。
- 二维码面板让导播按场地切 LAN 或 Relay;**手机看到的 URL 决定走哪条**,业务上无感。
- **同一份 `Arc<axum::Router>` 被两条 transport 复用**:LAN `web_server::run_with_router` 直接 listen,
  Relay `relay_client` 当 dispatcher。`routes::router` builder 一份。

### §6. Cargo workspace + 共享 schema

为让导播与 relay 共享 wire types,引入 workspace:
```
StageDeckRemote/
├── Cargo.toml                  workspace 根
├── src-tauri/                  导播 Tauri 应用 + relay_client.rs
└── crates/
    ├── protocol/               ControlMessage + mjpeg_part(共享)
    └── relay/                  relay binary (lib+bin split 让集成测试能进入)
```

集成测试 `crates/relay/tests/end_to_end.rs` 用 mock 导播 + reqwest 模拟手机,
**完全在 Linux loopback 上端到端跑通**,不依赖 Windows / Tauri。

## 影响

**正面**
- LAN 不可靠时有备选,不依赖场馆 WiFi 的对等连通性。
- 业务代码零改动,新增一条 transport 不污染锁/cid/控制权那套既有判定。
- Broker 抽象 ADR-0006 的水到渠成复用;扇出语义一致,导播侧多一个消费者也是 watch 订阅。
- 手机端协议零感知:同一份 presenter.html / .js,LAN 和 Relay 都跑——
  借 `<base href>` 注入 + LAN `/presenter/api/*` 别名 + Relay `/r/<sid>/presenter/api/*` 别名实现。
- 集成测试不需要 Windows / Tauri,Linux CI 可直接跑端到端冒烟。

**负面 / 代价**
- 多一跳延迟(实测控制类隧道往返 +5–20ms 量级);静态幻灯片不敏感,接受。
- relay 端无持久化:重启 = 所有 session 失效,导播自动重连即恢复;v1 不补 snapshot/replay。
- session 宽限期清理任务(spec §13)字段就位但 v1 未启用——**僵尸 session 在内存里累计**;
  单会话场景影响小,follow-up 任务追加。
- 多一份二进制 + 一套 systemd 维护成本(见 [network.md §7](../network.md))。
- ws→http 取 base 时的 scheme 字符串替换;导播没在 ws 那端用 wss/TLS,v1 校园内裸 HTTP,
  TLS 留 v2 跟 Caddy 反代一起做。
- WS 控制 + 流双连接;断线靠两端各自指数退避重连(0.5→5s + jitter)。
- relay_client 的 `RelayStatus` 在 ServerHello 验证完成**之前**就置 `Connected`,
  有半个 RTT 的"假绿"窗口(已知小瑕疵,follow-up 修)。

## 备选方案

- **WebRTC P2P + STUN/TURN**:NAT 穿透理论上更优,但要在 Rust 侧引整套 WebRTC 栈,
  且打破"手机零客户端"(STUN/TURN 是基础设施而不是协议简化)。否,留作未来与 ADR-0006 一并演进。
- **公网中继 + 公网域名**:解锁"手机走蜂窝"。但 v1 范围只校园内,且公网暴露意味着真要补 TLS、
  认证强度提升、限速、入侵检测——超出 MVP。校园内是合适的 sweet spot。
- **纯 WebSocket pubsub**(`relay-as-broker`,所有控制都走 WS pub/sub):干净但导播业务代码要翻面,
  原 axum::Router handler 全部得改用 WS 消息处理器。否,这违反"业务零改动"原则。
- **HTTP 长轮询替代 WS**:简单一点但实时性差、连接管理也不见得简单。否。

## 实施轨迹

发布于 v0.2.0(2026-06-25)。
- spec `2026-06-25-relay-architecture-design.md` → plan `2026-06-25-relay-architecture.md` →
  9 task 实施(workspace + protocol crate / relay 框架 / 隧道 / MJPEG 重扇出 / presenter 静态 +
  `<base href>` / 导播 relay_client / 导播 IPC + UI / Linux 集成测试)。
- 中途由最终评审抓到一个跨任务集成缺口(relay 缺 `/r/<sid>/presenter/api/*` 别名族),
  fix 落地 commit `f168171`。
- 全工作区 **44 测试通过 + 1 ignored**(MJPEG 端到端在 Windows 真机验证)。
