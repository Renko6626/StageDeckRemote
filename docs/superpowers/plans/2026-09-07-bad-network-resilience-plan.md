# 糟糕网络可靠性 — 实施计划

Spec：`docs/superpowers/specs/2026-09-07-bad-network-resilience-design.md`。每个任务一个 commit，先测后码。

1. **protocol 常量 + op_cache 纯模块**
   `crates/protocol`：`HEARTBEAT_INTERVAL / LIVENESS_TIMEOUT / CONNECT_TIMEOUT` + `liveness_expired()`。
   `src-tauri/src/op_cache.rs`：`OpCache { reserve, complete, evict }`，单测覆盖 Hit / InFlight / 过期 / 上限。
2. **routes 接入 op_id + presence 10s**
   六个动作接口请求体加 `op_id`；门控之后 reserve，执行后 complete；命中写日志。
   `PRESENCE_TIMEOUT_MS` → 10000；`get_status` 加 `presenter_seen_ms_ago`。routes 单测：重复 op 只执行一次。
3. **Relay 心跳 + 判死 + 清 pending**
   控制 WS：10s JSON Ping，25s 读超时；断开时 drain pending → 502。推流 WS：10s 协议层 Ping，25s 读超时。
   e2e：Ping 到达；沉默 25s 后关连接且隧道请求立即 502。
4. **导播 relay_client**
   8s connect 超时；控制读侧 25s 超时；推流 `select!` 读写；生命周期写 `log::event`。
   单测：握手超时、读侧超时（`time::pause`）。
5. **presenter.js**
   `sendOp()` 重试 + 统一 pending；`lastOkAt` 三档 + 500ms tick；status 超时 5s。改写 presenter.spec.ts。
6. **StatusBar 三态** + spec。
7. **验证**：`cargo test --workspace`、`pnpm build`、`pnpm test`、xwin 交叉编译。
8. **收尾**：CHANGELOG、版本 0.2.3 → 0.2.4、NSIS 安装包 + relay 二进制 → `out/`，合回 master。
