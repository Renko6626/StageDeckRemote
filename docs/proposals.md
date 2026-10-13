# 待办与提案(Backlog)

> 滚动维护的工程/产品待办清单。来源:2026-06-26 的整体代码审阅。
> 完成一项就在前面打勾或挪到 `## 已完成` 区段;新加项往对应 Tier 末尾追加。
> **不是路线图**——优先级随真实用户/演出反馈调整。

---

## Tier 1 · 清债(small,~1-2 小时合计)

已知尾巴。没什么决策成本,该清就清。

- [x] ~~**relay session 宽限期 reaper**~~ — 见 "已完成"。spec §13 兑现:`collect_expired` 摘 sid + 10s tick reaper。
- [x] ~~**`relay_client::control_loop` 状态时序**~~ — Connected 推迟到 `ServerHello{ok:true}` 抵达后。见 "已完成"。
- [x] ~~**`relay_client::stream_loop` 校 ServerHello**~~ — 严格解析 + 早期失败路径全走 backoff。见 "已完成"。
- [x] ~~**删 `web_server::run` 死代码**~~ — 见 "已完成"。
- [x] ~~**删 `src-tauri/Cargo.lock` 僵尸文件**~~ — `git rm` + `.gitignore` 防复活。见 "已完成"。
- [x] ~~`use crate::session::Session` unused import~~ — 已在 admin page PR(commit `8212e19`)顺手清掉。

---

## Tier 2 · 工程基础设施 / 可观测性(medium,各 1-2 周末)

按"演出前晚上能不能睡着"排:

- [x] ~~**GitHub Actions CI**~~ —— `.github/workflows/ci.yml` 落了:push/PR 到 master 触发,Ubuntu runner 跑 `pnpm typecheck` + `pnpm build` + `pnpm test`(vitest) + `cargo test --workspace`。配套 `release.yml`:tag `v*.*.*` 推送触发 3 个 job(Windows NSIS / Linux relay / 收尾出 draft GitHub Release 挂俩产物)。CHANGELOG 抽取脚本 `scripts/extract-changelog.sh` 把对应 version section 当 release body。
- [x] ~~**自动化 release 脚本**~~ 见下面"已完成"(commit 在本日完成)。
- [x] ~~**relay 结构化日志(tracing)**~~ 见下面"已完成"。
- [ ] **relay `/admin/metrics` 端点**(中,~100 行)
  同 `admin_token` 鉴权。两种格式:
    - JSON(curl 可用):`{tunnel_total, tunnel_failed, tunnel_timeout, stream_frames, stream_bytes, ws_reconnects, sessions_registered, sessions_evicted}`
    - Prometheus(`# HELP / # TYPE`):配 Grafana 看时间序列
  Tier 1 的 reaper 做完后再做这条,**两条配合 = 真能事故复盘**。
- [ ] **导播侧结构化日志**(中,~50 行)
  director 端的 `log::event`(内存环 → GUI 日志面板)保留;另外接 tracing,文件输出 `%LOCALAPPDATA%\StageDeck\director.log`(rotating)。
  目前 panic / `eprintln!` 类崩盘日志找不到。
- [ ] **CHANGELOG.md**(Nit,持续)
  release 脚本提示了但没有强制。每次 bump 时手工写 1-3 句版本变更。
- [ ] **README 加 release badge / 截图**(Nit)
  README 里可以放最新 GitHub Release 链接 + 导播 / 主持人页 / status page 截图各一张。

---

## Tier 3 · 架构演进(可选,挑 1-2 个做)

每条都解锁一类新场景,不做也不掉档。按性价比/复杂度排:

- [ ] **relay session 持久化**(中,半天)
  落 `/var/lib/stagedeck-relay/state.json`:secret、token、registered_at、last_seen。relay 启动读回。
  收益:systemd 升级 / OS 重启不再吹掉进行中的演出。
- [ ] **`std::sync::Mutex` → `parking_lot::Mutex`**(小,1 小时)
  audit 优化空间 §5。机械改动,删 `AppState::lock()` poison-recovering helper,代码更清爽且快一点。
- [ ] **WebRTC 推流路径**(大,1 周+)
  只有"哪天真要播 PPT 内嵌视频/直播流"时才做。broker 抽象已为它留好接口(多一个 consumer)。
  **触发条件**:演出内容形态从纯静态幻灯片变为动态。
- [ ] **多 relay / failover**(中-大)
  简单版:导播 Settings 配多个 base_url,手动切换。完整版:relay 间 gossip 自动接管。
  **触发条件**:一台 CLAB 不够稳。
- [ ] **跨平台导播(macOS / Linux)**(大)
  把 COM/WGC 改成"PPT 控制 trait + Windows driver";macOS 用 AppleScript + ScreenCaptureKit。
  **触发条件**:用户群明确出现非 Windows 用户。

---

## Tier 4 · 用户可见功能(等基础设施扎实后再做)

按"用户感知 × 复杂度"排:

- [ ] **通用 PPT 动作识别与 VBA 兼容性**（2026-10-13 记录，暂缓）
  详见 [样本结论、当前算法与验收待办](vba/compatibility-backlog.md)。重点是可见按钮与真实点击层关联、文字/组合动作、宏存在性与签名诊断、文稿/页面/形状身份校验、动态重建及可信错误反馈。不得按特定 PPT 文件、页码或形状前缀硬编码。用户决定先导出当前版本；涉及接口和模块路线的改动后续确认。
- [ ] **预演健康检查**(小,推荐先做)
  一个按钮 → 自动 ping:token / 锁 / 网络 / PPT 窗口 / relay 状态,出 OK/警告列表。**演出当天最想要的就是这个**。
- [ ] **演讲者备注**(中)
  从 COM 读 `Slide.NotesPage.Shapes(2).TextFrame.TextRange.Text`,主持人页加 NOTES tab。
- [ ] **观察席模式**(小-中)
  `is_controller` 扩成 controller + observers。observer 能看画面/slide_state,任何 action 拒。
  audit `2026-06-16-code-audit.md` 也提过。
- [ ] **多段演出 / 计时模板**(小)
  Settings 预设 "30min 主旨" / "45min 讨论" 几组,一键切换 timer。
- [ ] **导播移动伴侣**(中)
  额外的扫码 URL,导播自己扫,只看状态 + 强制锁/解锁,不能选窗。
- [ ] **PPT 投票 / 答题数据汇总**(中-大)
  泛化 `/api/message` 模式:动作类要 controller,投票/反馈类只要 token。

---

## Tier 5 · 代码健康(文件越大越逼近做这层)

不紧急,但 file size 信号已响:

- [ ] **拆 `src-tauri/src/commands.rs`** (~340 行)
  按功能拆 `commands/{capture,timer,presets,settings,relay,window}.rs`,`mod commands; pub use`。
- [ ] **拆 `src/stores/appState.ts`**
  独立 `useTimerState` / `useRelayState` / `usePresetsState`。
- [ ] **`relay_client.rs` 内部状态机化**
  目前 `loop { connect; if err continue; ... }` 风格;`enum ConnectionState { Idle, Connecting, Connected, Reconnecting(backoff) }` 更清楚。
- [ ] **`docs/project.md` 头部加陈旧提示**
  1270 行原始产品文档。加一行"v0.2.0 起内容陈旧;最新现状以 architecture.md + ADR 为准"。
- [ ] **presenter 独立 build 与否的判断**
  目前 vanilla JS 嵌进 director 和 relay 两个 binary;改 1 个字要重编 2 个。
  如果以后想让前端独立迭代,需要更大的重构;不想就别动。

---

## 红线 · 别做(避免后悔)

- ❌ **让 relay 走公网**(没补 TLS / 限速 / DDoS 保护之前)。spec §13 写了"v1 校园内",这条红线在补足之前别动。
- ❌ **把 PPT 控制做成"通用桌面遥控"**。诱惑会来——别走。信任边界(ADR-0002)是这个项目的护城河,这一退就是远程桌面的所有问题。
- ❌ **为"多用户共用一台 relay"做投资**直到真出现这种需求。多租户 = 隔离 + 配额 + 审计,工作量翻 5 倍。
- ❌ **轻易动 `frame_broker` 的 `watch` 抽象**。换 `broadcast` 会让慢消费者拖累所有人或被 lag-drop。"永远只看最新"对视频流是正确的。

---

## 建议的执行顺序(如果只看一段就看这段)

**Sprint 1(本周末)** — 清债 + 安全网
- [x] proposals.md 写出来(就是本文件)
- [x] release 脚本 + CHANGELOG 起头
- [x] relay 结构化日志(tracing)
- [x] Tier 1 那 5 条尾巴一次性清掉
- [x] GitHub Actions CI(`.github/workflows/ci.yml` + `release.yml` + extract-changelog)

**Sprint 2** — 可观测性
- [ ] relay `/admin/metrics`(JSON + Prometheus)
- [ ] 导播端 tracing 落盘
- [ ] 部署 Prometheus + Grafana(可选但强推)

**Sprint 3** — 演出体验
- [ ] 预演健康检查
- [ ] 演讲者备注

**Sprint 4 起** — 看真实演出反馈推着走,不照清单照单全收。

---

## 已完成

按日期倒序追加;链接到 commit 或 PR。

- `2026-06-27` · GitHub Actions CI + Release (`.github/workflows/ci.yml` + `release.yml` + `scripts/extract-changelog.sh`) — Tier 2
- `2026-06-27` · 导播 GUI Setup/Live 双模式重构 + vitest + 39 单测 (v0.2.1) — Tier 4 / 用户体验
- `2026-06-26` · Tier 1 五条尾巴清掉:relay session 宽限期 reaper(`SessionTable::collect_expired` + 10s tick)/ `relay_client` control+stream WS 严格 ServerHello 校验 + 状态时序修正 / 删 `web_server::run` / 删 `src-tauri/Cargo.lock` 僵尸文件 — Tier 1
- `2026-06-26` · 自动化 release 脚本(`scripts/release.sh`)+ CHANGELOG.md 起头 — Tier 2
- `2026-06-26` · relay 结构化日志(tracing-subscriber + 关键事件点)— Tier 2
- `2026-06-26` · proposals.md 起草(就是本文件)— 元
