# 代码审计报告 + 修复计划

**日期:** 2026-06-16
**范围:** 全仓(~5,700 行,Rust 后端 + Vue/presenter 前端)5 维并行审计:安全/信任边界、Rust 并发与状态、原生 Win32/COM、前端、跨文件一致性。
**结论:** 架构干净、信任边界真实有效、锁模型自洽且有单测;无结构性问题。下列为可执行发现。

> 本文档既是审计记录,也是本轮修复(Critical + High + Medium)的依据。Low/Nits、文档重写、以及若干"优化空间"列在 §4 暂不在本轮修复范围。

---

## 1. Critical / High

| # | 位置 | 问题 | 修法 | 本轮 |
|---|---|---|---|---|
| 1 | `commands.rs:34`、`routes.rs:131/198/268` vs 其余 `if let Ok` | **Mutex 中毒处理不统一。** `get_status`(导播 1.5s 轮询)+ 3 个 HTTP handler 用 `.expect("state lock poisoned")`(中毒即 panic),其余全用 `if let Ok`(中毒即静默 no-op)。一次持锁 panic → 导播轮询开始抛错,**同时 `force_lock`/`set_remote_enabled`/`regenerate_token` 等急停开关静默失效** —— 对急停类 App 是最糟失效方向。 | 加 `AppState::lock()` 帮助方法,`self.inner.lock().unwrap_or_else(\|p\| p.into_inner())` 恢复中毒守卫;把所有锁点改用它(全部变成不可失败 + 中毒自恢复)。 | ✅ #1 |
| 2 | `presenter.js:178-184` | **重置 token 后主持人端 401 永久死锁。** `handle401()` 杀轮询且不重试、不提示重扫码;一次偶发 401 即锁死会话。 | 401 后保留慢速重试循环(数秒/次)自愈,并显式提示"访问码已更改,请重新扫码"。 | ✅ #2 |
| 3 | `routes.rs:238/324/372` → `ppt_com::*`、`/api/snapshot.jpg` → `capture` | **同步 STA COM/GDI 跑在 tokio async worker 上**,无 `spawn_blocking`、每次调用都 `CoInitializeEx(STA)`。风险:apartment 混乱致偶发 `RPC_E_WRONG_THREAD`;PowerPoint 慢调用/弹窗阻塞执行器。真机"控制失败/卡顿"最可能根因,交叉编译/Linux 测试照不到。 | 本轮:给每个 COM/capture 调用包 `tokio::task::spawn_blocking`(对象不跨 await,安全且可编译验证)。完整"专用 STA 线程 + 通道"列入 §4 优化。 | ✅ #3(spawn_blocking) |
| 4 | `presenter.js:238/247/264` | **主持人操作 403 静默无反馈。** action POST 是空 `.catch()`;导播在两次轮询间锁屏 → 点"下一页"无反应、无提示。 | 检查响应状态,403 时立即 `poll()` 刷新并闪"已被锁定/已失去控制权"。 | ✅ #4 |

## 2. Medium

| # | 位置 | 问题 | 修法 | 本轮 |
|---|---|---|---|---|
| 5 | `auth.rs:18` | Token 用 `==` 比较(短路)→ 同 WiFi 计时旁路;token 是唯一闸门且无限速 → 可无限探测。 | 常量时间比较(`constant_time_eq` crate),先比长度。 | ✅ #5 |
| 6 | `routes.rs` router 无 layer | **无 body 大小上限**(截图很吃 CPU 且仅 token-gated;无显式上限)。 | 加 axum 原生 `DefaultBodyLimit`(无新依赖)。限速对 LAN 偏重,列入 §4。 | ✅ #6(body limit) |
| 7 | `routes.rs:142` `status` | `/api/status` 把 `active_client_id` 明文返回给所有人 → 任何持 token 者可读到控制端 cid;配合可伪造 cid 可在 4s 心跳间隙抢控。 | 不向客户端返回 `active_client_id`(客户端只需 server 算好的 `is_controller`);确认 presenter.js 不读它。 | ✅ #7 |
| 8 | `app_state.rs:69` | 计时器用 `SystemTime`(挂钟,非单调);演出中 NTP/手动改表回拨会让运行中计时器跳变(saturating 防 panic,自愈但抖动)。 | 计时基准改单调 `Instant`,挂钟只留给日志时间戳。 | ✅ #8 |
| 9 | `DirectorView.vue:81-92` | 1.5s 轮询无 in-flight 守卫;Windows 上 refresh >1.5s 时 tick 堆叠,晚到旧结果覆盖新状态。 | 加 `inFlight` 守卫跳过,或改 setTimeout 链。 | ✅ #9 |
| 10 | `lib.rs:71/93` | 3 个 spawn 循环(消息接收、断线 watcher)panic 后静默死亡,本场后续不再有断线提醒/弹窗。 | 循环体包错误恢复(记日志后继续);watcher 锁点用 #1 的 `lock()` 自恢复。 | ✅ #10 |
| 11 | `config.rs:11` | `last_selected_window_title` 半接线:声明并序列化,从不读写。 | 本轮:删字段(YAGNI)。"记住上次窗口"若要做列入 §4。 | ✅ #11(删) |
| 12 | `Cargo.toml:30` | `tower-http`(cors 特性)被引入但全仓 0 引用(`router()` 无 `CorsLayer`)。 | 删依赖(#6 用 axum 原生,不需要它),同步 Cargo.lock。 | ✅ #12 |
| 13 | `presenter.js:103` | 锁按钮判据把"能 ARM"与"能 LOCK"混为一谈(`/api/lock` 只需 `is_controller`),锁屏后连"重新上锁"也按不动。 | `online && isController` 即允许 lock 动作。 | ✅ #13 |

## 3. Low / Nits(本轮不修,记录)
- `tauri.conf.json:37` `csp:null`:导播窗口会渲染主持人消息,建议加 `default-src 'self'` 兜底。
- `presenter.js:347` `setInterval(tickTimer,250)` 从不清理/不随断线停(便宜,但 teardown 不一致)。
- 误导/过期 `#[allow(dead_code)]`:`routes.rs` 里 3 个 `client_id` 字段其实有用(传给 `is_controller`);真正无用的是 `MessageReq.client_id`、`window_manager::window_exists`、`ppt_com::call_i32`。
- `DirectorMessages.vue:79` 编辑列表用 index 作 `:key`(可变列表响应式陷阱)。
- `QrCodePanel.vue:20` `draw()` rejection 未处理;空 URL 时旧 QR 残留。
- 消息通道无界,token-gated 刷消息可致内存增长。
- 动作鉴权与执行间的 sub-ms TOCTOU(无害)。

## 4. 文档过期 + 优化空间(本轮不修,记录)
**文档过期**(`README.md`、`docs/architecture.md` 停留在 MVP 期):
- README L75-77 `TODO(windows)` 占位项全已实现;L22 锁模型描述过时。
- architecture.md L157 `control_locked` 默认写成 `true`(实际 `false`);L137 `/api/next` 称只需 `button_allowed`(实际所有动作需 `click_allowed`,只有 `/api/unlock` 用 `button_allowed`);L207/251 仍列 `obs.rs`/OBS(已删);§10 占位表全已完成。
- 最新 `docs/superpowers/specs/2026-06-16-*` 与代码一致,为权威来源。

**优化空间:**
1. COM 专用 STA 线程 + 命令通道(治本,#3 的完整版)。
2. 轮询 → 推送(导播 4 路 interval 合并 / Tauri event;主持人 SSE/WebSocket)。
3. 截图流水线:缓存上帧、空闲降帧/退避、或流式推送。
4. presenter 端共享类型,挡后端字段漂移。
5. `parking_lot::Mutex`(根除整类中毒问题,比 #1 帮助方法更彻底)。
6. 完成"记住上次窗口"(补全 #11 删掉的字段)。
7. 把更多 COM 邻近逻辑抽成纯函数测试;加 HTTP 鉴权矩阵集成冒烟。

---

## 5. 修复执行追踪
- 验证(读-only,并行):确认 #1–#13 是真问题、给精确补丁配方、剔除误报。
- 实现(按文件分区避免冲突):Rust 一组(全部 `src-tauri`)、前端一组(`presenter.js`)。
- 每项:交叉编译 0 error + `cargo test` 绿;前端 `pnpm build` 绿(presenter.js 经 Rust 重编嵌入)。
- 版本 0.1.13 → 0.1.14,出安装包,合并 master。
