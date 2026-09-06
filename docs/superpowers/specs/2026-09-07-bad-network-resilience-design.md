# 糟糕网络下的控制可靠性

目标：手机 Wi‑Fi 丢包、导播 PC 与 Relay 之间链路抖动这两类情况下，主持人按下去的动作要么确定执行一次，要么明确告诉主持人"结果未知"；任何一段链路半开都能在 25s 内被发现并自愈。Relay 的 `config.toml` 不加新配置项。

## 已确认的代码问题（v0.2.3）

1. Relay 设计稿写了 Relay→导播 Ping 心跳，`crates/relay/src/director_ws.rs` 从未发送；导播端也不主动发；两端都没有 TCP keepalive。控制 WS 半开时导播卡在读，Relay 认为通道存活，手机每个请求等满 30s 才得到 504，恢复只能靠操作系统 TCP 超时。
2. 翻页坞请求失败被 `catch(function () {})` 静默吞掉，没有 pending 守卫，也没有幂等 id。坏网下"按了没反应再按一次"会双翻页。
3. 0.2.3 把 status 心跳超时拉到 35s，坏网时手机 35s 内一直显示 LINK 且按钮可点；`statusPending` 单飞使轮询在这 35s 内停摆。服务端 4s 无心跳即丢控制权，与 35s 不匹配。
4. `connect_async` 没有超时；导播断开后 Relay 上等待中的隧道请求不会被清理。
5. 导播端推流循环只写不读：收不到 Close 帧，也无法回应协议层 Ping。
6. Relay 断线重连全程不写导播日志，导播只看到 `connecting`。

## 一、心跳与半开检测

常量放 `protocol` crate，不进 Relay 配置：

| 常量 | 值 |
| --- | --- |
| `HEARTBEAT_INTERVAL` | 10s |
| `LIVENESS_TIMEOUT` | 25s |
| `CONNECT_TIMEOUT` | 8s |

Relay（`director_ws.rs`）：

- 控制 WS 写者每 10s 发 JSON `Ping{nonce}`。读者用 25s 超时包住 `stream.next()`，任何入站消息都算存活；超时或出错即关闭 socket、退出循环。
- 断开时清空该 session 的 `pending` 表，所有等待中的隧道请求立即得到 `502 director_offline`。
- 推流 WS 每 10s 发 WS 协议层 `Ping` 帧，读者同样 25s 超时。协议层 Pong 由 tungstenite 自动回复。
- 判死逻辑抽成纯函数 `liveness_expired(last_inbound, now) -> bool`。

导播（`relay_client.rs`）：

- 两条连接的 `connect_async` 都包 8s 超时。
- 控制循环读侧 25s 超时，超时视为半开，主动断开并走现有退避重连。
- 推流循环改为 `select!` 同时等待"新帧"和"入站消息"，入站侧 25s 超时。

不做：TCP keepalive（应用层心跳已覆盖）。退避（0.5→5s + 抖动）与 60s session 宽限期不变。

兼容性：JSON Ping 变体已在协议里，老导播遇到新 Relay 正常；老 Relay 遇到新导播会每 25s 误判一次并重连，只在混用版本时发生，接受。

## 二、幂等与重试

服务端：

- 六个动作接口（`next/prev/black/restore/action/click`）请求体加可选 `op_id: String`。未携带则按老行为执行。
- 新模块 `op_cache.rs`，挂在 `Inner`：最多 64 条、60s 过期，每条为 `InFlight` 或 `Done { status, body }`。纯函数：`reserve(op_id, now) -> Hit(resp) | InFlight | Reserved`、`complete(op_id, resp, now)`、`evict(now)`。
- 接入点在 token、控制权、三层锁校验之后：门控失败不进缓存。命中 `Done` 原样回放；命中 `InFlight` 回 `409 {"ok":false,"reason":"op_in_flight"}`。
- 命中时写导播日志"重复请求 op=… 已去重"。
- Relay 不改；重试与首发同时在隧道里时由导播缓存挡住。

手机端（`presenter.js` 新增 `sendOp(path, body)`，替换所有动作按钮的裸 `api()`）：

- `op_id = cid + "-" + 自增序号`。
- 总预算 4s，最多 3 次尝试。前两次每次 1.5s 超时；最后一次用完整超时（翻页/点击 5s，宏 12s）。
- 重试条件：网络错误、超时、`502`、`503`。不重试：2xx、`401`、`403`、`409`、`504`。
- 发送期间所有动作按钮禁用（`actionPending` 扩到翻页坞与 SYNC 点击），被按的按钮显示"…"。
- 提示：成功不弹；`409` 提示"已送达，PPT 处理中"；预算耗尽提示"未送达或结果未知，核对页码后再按"；`403` 沿用"已被锁定"。任何结局都立即 `poll()` 与 `pollSlide()`。

不做：离线队列；跨页面刷新的 op_id 持久化。

## 三、手机端连接状态机

状态源改为 `lastOkAt`（最近一次 status 成功时刻），`online/connecting` 布尔退休。`render()` 按距今时长分档：

| 档 | 判定 | 显示 | 按钮 |
| --- | --- | --- | --- |
| LINK | ≤ 3s | 绿点 + 延迟 ms | 可用 |
| 弱 | 3s 到阈值 | 黄点 + "弱 · Ns 前" | 可用 |
| DOWN | 超过阈值 | 红点 + 断线遮罩 | 禁用 |

阈值：LAN 8s，Relay 15s。模式由 `document.baseURI` 是否含 `/r/` 判断，只用于此阈值。首次成功前显示"LINK…"，首次失败即 DOWN。

- status 轮询超时从 35s 改回 5s，两种模式一致。轮询节奏 900ms 与 401 慢轮询 4s 不变。
- 新增 500ms 本地 tick 驱动档位切换，与计时器 tick 独立。
- 服务端 `PRESENCE_TIMEOUT_MS` 4s → 10s。
- DOWN 时本地视图锁定、按钮禁用，不向服务端发 lock；恢复后按下一次心跳的 `presenter_unlocked` 决定按钮是否直接回来。

## 四、导播 GUI 可见性

- `get_status` 加 `presenter_seen_ms_ago: Option<u64>`；`StatusBar` 三态：≤3s "在线"、3–10s "弱 · Ns 前"、>10s "离线"。
- `relay_client.rs` 通过 `log::event` 写：连上（含第几次重连）、断开原因（25s 无心跳 / 握手超时 / 被 Relay 拒绝及理由）、退避秒数。
- Relay 状态字符串、QR 面板、Settings 不改。

## 五、测试

Rust 单测：`op_cache` 全部分支；`routes` 带 `op_id` 的 `/api/next` 两次只执行一次并回放同一响应、门控 403 不进缓存、无 `op_id` 行为不变；`app_state` presence 10s 与 `presenter_seen_ms_ago`；`relay/session` 断开清空 `pending`；`relay/director_ws` 判死纯函数。

Relay 端到端（`crates/relay/tests/end_to_end.rs`，`tokio::time::pause()`）：连上后 10s 内收到 Ping；导播沉默后 25s 内 Relay 关连接，期间隧道请求立即 502。

导播 `relay_client`：握手超时 8s 内返回并退避；控制读侧沉默 25s 后断开重连。

手机端（`presenter.spec.ts`）：三档切换（LAN 8s / Relay 15s）；重试序列（前两次超时第三次成功、三次全失败、首发 503、首发 409）；发送期间按钮禁用；改写现有 35s 测试。

Vue：`StatusBar.spec.ts` 三态。

收尾必跑：`cargo test --workspace`、`pnpm build`、`pnpm test`、`xwin` 交叉编译 0 错误。

Windows + 手机实测（彩排项）：

1. 手机开飞行模式 2s 再关：状态栏经"弱"回到 LINK，不丢 ARM。
2. 翻页瞬间断 Wi‑Fi：页码只前进一页；手机提示"结果未知"或无提示。
3. 拔导播网线 30s：Relay 日志出现判死；手机 DOWN；插回后 10s 内两端恢复，导播日志有重连记录。
4. 双击翻页：导播日志出现"已去重"，页码只前进一页。
5. LAN 与 Relay 各跑一遍。

限制：应用无法区分"请求丢在去路"与"响应丢在回路"，后者靠去重回放；宏在服务端仍可能因 VBA 弹窗超时，那一类结果仍需导播看 PPT。
