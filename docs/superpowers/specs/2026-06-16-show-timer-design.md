# 演出计时器(Show Timer)设计

**日期:** 2026-06-16
**状态:** 待审阅
**范围:** 导播设/控的倒计时(含超时),同步给主持人顶部状态条。不动锁/COM/消息。

---

## 1. 目标

导播设定一个时长并启动倒计时;主持人手机**顶部状态条**看到剩余时间(`M:SS`),**归零后转红、继续向上计超时**(`+M:SS`)。两端秒数平滑跳动且不受时钟偏差影响。

用户拍板:**倒计时 + 到点转红超时**;**主持人放顶部状态条小读数**。

## 2. 状态模型(`app_state.rs`)

```rust
pub struct TimerState {
    pub duration_ms: u64,        // 设定时长
    pub running: bool,           // 是否在跑
    pub started_at_ms: u64,      // 本次 running 起点(服务端时钟)
    pub elapsed_before_ms: u64,  // 之前累计(暂停时累加)
}
```
`Inner.timer: Option<TimerState>`(默认 None,不持久化)。

**纯函数(Linux 可单测)**:
```rust
pub fn timer_remaining_ms(duration_ms: u64, running: bool, started_at_ms: u64,
                          elapsed_before_ms: u64, now_ms: u64) -> i64 {
    let live = elapsed_before_ms + if running { now_ms.saturating_sub(started_at_ms) } else { 0 };
    duration_ms as i64 - live as i64   // 可为负 = 超时
}
```

**展示视图** `TimerView { remaining_ms: i64, running: bool, duration_ms: u64 }`,由 `AppState` 在当前服务端时刻算出(`timer_remaining_ms(...)`),`Option`。

## 3. 导播控制(IPC 命令)

- `timer_start(duration_ms: u64)` → `timer = Some(TimerState{ duration_ms, running:true, started_at_ms: now, elapsed_before_ms:0 })`(从满开始)。
- `timer_toggle()` → 有计时则翻转 running:暂停时 `elapsed_before_ms += now - started_at_ms; running=false`;继续时 `started_at_ms=now; running=true`。无计时则无操作。
- `timer_reset()` → `timer = None`。

## 4. 送达 & 客户端走秒

- `TimerView` 折进**两个** status:`get_status`(导播 IPC,`DirectorStatus.timer`)与 `/api/status`(主持人 HTTP,`timer` 字段)。剩余由服务端算好,客户端**不需要绝对时钟**。
- **两端客户端本地插值**:收到 `remaining_ms`+`running` 时记 `base={remaining, at: 本地now}`;显示 = `running ? base.remaining - (本地now - base.at) : base.remaining`;本地每 ~250ms 跳一次,每次 status 轮询(900/1500ms)重新对齐 base。→ 平滑、无导播 PC 与手机的时钟偏差。

## 5. UI

- **导播**:新 `DirectorTimer.vue` 卡(控制栏内)——快捷 **5/10/15/20 分钟** + 自定义(分钟数输入)、**Start / Pause(继续)/ Reset**、大读数(超时 `+M:SS` 转红)。读 `appState` 的 `timer` 并本地走秒;操作调对应 IPC 后刷新。
- **主持人**:顶部状态条加一格 `计时 <b>M:SS</b>`;超时转红;任意页可见;在 status 轮询里读 `s.timer` 并本地走秒。

## 6. 影响文件

| 文件 | 改动 |
|---|---|
| `src-tauri/src/app_state.rs` | `TimerState`/`TimerView`、`Inner.timer`、`timer_remaining_ms`(+单测)、`timer_view()` |
| `src-tauri/src/commands.rs` | `timer_start`/`timer_toggle`/`timer_reset` |
| `src-tauri/src/lib.rs` | 注册三个计时命令 |
| `src-tauri/src/routes.rs` | `/api/status` 加 `timer` |
| `src/utils/api.ts` | `TimerView` 类型 + `DirectorStatus.timer` + `timerStart/timerToggle/timerReset` |
| `src/stores/appState.ts` | `timer` ref(从 `get_status` 取) |
| `src/components/DirectorTimer.vue` | **新增**:计时卡 |
| `src/views/DirectorView.vue` | 控制栏挂载 `DirectorTimer` |
| `src/presenter/presenter.{html,css,js}` | 状态条计时读数 + 本地走秒 |

## 7. 测试

- **Rust 纯逻辑:** `timer_remaining_ms` 单测(运行中/暂停/超时/未启动);既有 23 保持绿。
- **交叉编译:** `cargo xwin build` 0 错误。
- **前端构建:** `pnpm build`。
- **运行时(用户 + 手机):** 导播 Start 5 分钟 → 两端同步倒数;Pause/继续/Reset 生效;归零后两端转红、向上计 `+M:SS`;关掉应用不持久化(重启清空)。

## 8. 不做(YAGNI)
- 不做多个计时器、不做计时声音提醒(到点转红即可;声音可后续)。
- 计时不持久化。
- 不动锁、COM、消息、捕获。
