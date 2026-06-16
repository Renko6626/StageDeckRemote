# 演出计时器 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 导播设/控倒计时(含超时),`get_status`+`/api/status` 同步,导播卡 + 主持人状态条本地平滑走秒。

**Architecture:** `Inner.timer: Option<TimerState>` + 纯函数 `timer_remaining_ms`/`timer_view_of`;两个 status 内联算出 `TimerView{remaining_ms,running,duration_ms}`;两端客户端拿 `remaining_ms` 后本地插值、每次轮询重对齐。详见 `docs/superpowers/specs/2026-06-16-show-timer-design.md`。

**Tech Stack:** Rust(Tauri IPC、Axum)、Vue 3、原生 JS。

**验证命令:**
- 原生测试: `P=/data/sunyunbo/miniconda3/envs/tauri-dev; PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml`
- 交叉编译: `cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
- 前端构建: `cd /data/sunyunbo/www/StageDeckRemote && pnpm build`

---

### Task 1: 后端计时(状态 + 纯函数 + 命令 + status)

**Files:** `src-tauri/src/app_state.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/routes.rs`

- [ ] **Step 1: `app_state.rs` 类型/字段/纯函数/测试**

在 `DirectorMessage` 定义之后加:

```rust
/// 演出计时器状态(临时,不持久化)。
#[derive(Clone, Debug)]
pub struct TimerState {
    pub duration_ms: u64,
    pub running: bool,
    pub started_at_ms: u64,
    pub elapsed_before_ms: u64,
}

/// 计时展示视图(remaining 可为负 = 超时)。
#[derive(Clone, Debug, Serialize)]
pub struct TimerView {
    pub remaining_ms: i64,
    pub running: bool,
    pub duration_ms: u64,
}

/// 服务端当前毫秒时间。
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 计时剩余毫秒(可为负=超时)。纯函数,便于单测。
pub fn timer_remaining_ms(
    duration_ms: u64,
    running: bool,
    started_at_ms: u64,
    elapsed_before_ms: u64,
    now_ms: u64,
) -> i64 {
    let live = elapsed_before_ms + if running { now_ms.saturating_sub(started_at_ms) } else { 0 };
    duration_ms as i64 - live as i64
}

/// 由计时状态 + 当前时刻算出展示视图。纯函数。
pub fn timer_view_of(t: &TimerState, now_ms: u64) -> TimerView {
    TimerView {
        remaining_ms: timer_remaining_ms(
            t.duration_ms,
            t.running,
            t.started_at_ms,
            t.elapsed_before_ms,
            now_ms,
        ),
        running: t.running,
        duration_ms: t.duration_ms,
    }
}
```

在 `Inner` 里 `pub director_message: Option<DirectorMessage>,` 之后加:

```rust
    /// 演出计时器(临时;两端轮询 status 取走显示)。
    pub timer: Option<TimerState>,
```

在 `AppState::new` 的 `Inner { ... }` 里 `director_message: None,` 之后加:

```rust
                timer: None,
```

在 `app_state.rs` 的 `#[cfg(test)] mod tests` 里加一个测试:

```rust
    #[test]
    fn timer_remaining_running_paused_overrun() {
        // 运行中:5s 时长,已过 2s(started=1000,now=3000)→ 剩 3s
        assert_eq!(timer_remaining_ms(5000, true, 1000, 0, 3000), 3000);
        // 暂停:累计 2s,不随 now 变 → 剩 3s
        assert_eq!(timer_remaining_ms(5000, false, 0, 2000, 999_999), 3000);
        // 超时:运行中已过 7s → 剩 -2s
        assert_eq!(timer_remaining_ms(5000, true, 0, 0, 7000), -2000);
        // 未启动(running=false,elapsed=0)→ 满
        assert_eq!(timer_remaining_ms(5000, false, 0, 0, 123), 5000);
    }
```

- [ ] **Step 2: `commands.rs` 三个计时命令 + get_status 带 timer**

在 `commands.rs` 末尾加:

```rust
/// 导播 GUI:启动计时(从满开始倒计时,ms 毫秒)。
#[tauri::command]
pub fn timer_start(state: State<Arc<AppState>>, ms: u64) {
    if let Ok(mut i) = state.inner.lock() {
        i.timer = Some(crate::app_state::TimerState {
            duration_ms: ms,
            running: true,
            started_at_ms: crate::app_state::now_ms(),
            elapsed_before_ms: 0,
        });
    }
    log::event(&state, "计时:开始");
}

/// 导播 GUI:暂停 / 继续。
#[tauri::command]
pub fn timer_toggle(state: State<Arc<AppState>>) {
    if let Ok(mut i) = state.inner.lock() {
        if let Some(t) = i.timer.as_mut() {
            let now = crate::app_state::now_ms();
            if t.running {
                t.elapsed_before_ms += now.saturating_sub(t.started_at_ms);
                t.running = false;
            } else {
                t.started_at_ms = now;
                t.running = true;
            }
        }
    }
}

/// 导播 GUI:重置(清空)计时。
#[tauri::command]
pub fn timer_reset(state: State<Arc<AppState>>) {
    if let Ok(mut i) = state.inner.lock() {
        i.timer = None;
    }
    log::event(&state, "计时:重置");
}
```

把 `DirectorStatus` 结构体里 `pub access_url: Option<String>,` 之后加:

```rust
    pub timer: Option<crate::app_state::TimerView>,
```

在 `get_status` 构造 `DirectorStatus { ... }` 里 `access_url,` 之后加(在持锁的 `i` 作用域内内联算,避免再锁):

```rust
        timer: i
            .timer
            .as_ref()
            .map(|t| crate::app_state::timer_view_of(t, crate::app_state::now_ms())),
```

> 注:`get_status` 当前在 `let i = state.inner.lock()...` 作用域里构造 `DirectorStatus`;上面这行用 `i.timer` 内联算,**不要**调用会再次加锁的方法(防死锁)。`now_ms()` 与 `timer_view_of` 都是纯函数、不加锁。

- [ ] **Step 3: `lib.rs` 注册三个命令**

在 `tauri::generate_handler![ ... ]` 里 `commands::get_snapshot,` 之后加:

```rust
            commands::timer_start,
            commands::timer_toggle,
            commands::timer_reset,
```

- [ ] **Step 4: `routes.rs` 的 `/api/status` 加 timer**

把 status 处理器 json! 里:

```rust
            "director_message": i.director_message.clone(),
            "server_time": now_secs(),
```

替换为:

```rust
            "director_message": i.director_message.clone(),
            "timer": i.timer.as_ref().map(|t| crate::app_state::timer_view_of(t, crate::app_state::now_ms())),
            "server_time": now_secs(),
```

- [ ] **Step 5: 交叉编译 + 原生测试**

Run 交叉编译(0 error)+ 原生测试(**24 passed**,新增 1 个 timer 测试)。

- [ ] **Step 6: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/app_state.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/routes.rs
git commit -m "feat(timer): backend countdown state, pure timer math, IPC commands, status views"
```

---

### Task 2: 导播计时卡(api + store + 组件 + 挂载)

**Files:** `src/utils/api.ts`、`src/stores/appState.ts`、`src/components/DirectorTimer.vue`(新)、`src/views/DirectorView.vue`

- [ ] **Step 1: `api.ts` 类型 + 字段 + 方法**

在 `PresetMessage` 接口之后加:

```ts
export interface TimerView {
  remaining_ms: number;
  running: boolean;
  duration_ms: number;
}
```

在 `DirectorStatus` 接口里 `access_url: string | null;` 之后加:

```ts
  timer: TimerView | null;
```

在 `export const api = { ... }` 里 `getSnapshot: ...` 之后加:

```ts
  timerStart: (ms: number) => invoke<void>("timer_start", { ms }),
  timerToggle: () => invoke<void>("timer_toggle"),
  timerReset: () => invoke<void>("timer_reset"),
```

- [ ] **Step 2: `appState.ts` 加 timer ref + 操作**

把 import 行:

```ts
import { api, type DirectorStatus, type LogEntry, type TargetWindow } from "../utils/api";
```

改为:

```ts
import { api, type DirectorStatus, type LogEntry, type TargetWindow, type TimerView } from "../utils/api";
```

在 `const logs = ref<LogEntry[]>([]);` 之后加:

```ts
  const timer = ref<TimerView | null>(null);
```

在 `applyStatus` 里 `activeClientId.value = s.active_client_id;` 之后加:

```ts
    timer.value = s.timer;
```

在 `autoDetectPpt` 函数之后加三个操作:

```ts
  async function timerStart(ms: number) {
    await api.timerStart(ms);
    await refresh();
  }
  async function timerToggle() {
    await api.timerToggle();
    await refresh();
  }
  async function timerReset() {
    await api.timerReset();
    await refresh();
  }
```

把 `return { ... }` 里加上(`autoDetectPpt,` 之后):

```ts
    timer,
    timerStart,
    timerToggle,
    timerReset,
```

- [ ] **Step 3: 新建 `src/components/DirectorTimer.vue`**

```vue
<script setup lang="ts">
// 演出计时器(导播):快捷/自定义时长 + 启动/暂停/重置 + 本地平滑走秒。
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";

const app = useAppState();
const { timer } = storeToRefs(app);

const QUICK = [5, 10, 15, 20];
const customMin = ref(10);

// 本地走秒:用收到的 remaining_ms + 本地时间差插值,每次 status 更新重对齐。
const base = ref<{ remaining: number; running: boolean; at: number } | null>(null);
const liveRemaining = ref(0);
let tick: number | undefined;

watch(
  timer,
  (t) => {
    base.value = t ? { remaining: t.remaining_ms, running: t.running, at: Date.now() } : null;
    recompute();
  },
  { immediate: true }
);

function recompute() {
  if (!base.value) {
    liveRemaining.value = 0;
    return;
  }
  const b = base.value;
  liveRemaining.value = b.running ? b.remaining - (Date.now() - b.at) : b.remaining;
}

function fmt(ms: number): string {
  const neg = ms < 0;
  let s = Math.floor(Math.abs(ms) / 1000);
  const m = Math.floor(s / 60);
  s = s % 60;
  return (neg ? "+" : "") + m + ":" + (s < 10 ? "0" : "") + s;
}

const active = computed(() => timer.value != null);
const running = computed(() => timer.value?.running ?? false);
const over = computed(() => active.value && liveRemaining.value < 0);
const display = computed(() => (active.value ? fmt(liveRemaining.value) : "—:—"));

function start(min: number) {
  void app.timerStart(Math.max(1, min) * 60000);
}
function toggle() {
  void app.timerToggle();
}
function reset() {
  void app.timerReset();
}

onMounted(() => {
  tick = window.setInterval(recompute, 250);
  recompute();
});
onUnmounted(() => {
  if (tick) window.clearInterval(tick);
});
</script>

<template>
  <section class="card">
    <h2>演出计时器</h2>
    <div class="readout" :class="{ over, idle: !active }">{{ display }}</div>
    <div class="quick">
      <button v-for="q in QUICK" :key="q" @click="start(q)">{{ q }}m</button>
      <span class="custom">
        <input type="number" v-model.number="customMin" min="1" max="180" />
        <button @click="start(customMin)">设定</button>
      </span>
    </div>
    <div class="ctrl">
      <button :disabled="!active" @click="toggle">{{ running ? "暂停" : "继续" }}</button>
      <button :disabled="!active" @click="reset">重置</button>
    </div>
  </section>
</template>

<style scoped>
.readout {
  font-family: var(--mono);
  font-size: 38px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--ink);
  text-align: center;
  padding: 4px 0 10px;
}
.readout.over {
  color: var(--danger);
}
.readout.idle {
  color: var(--ink-faint);
}
.quick {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 8px;
  align-items: center;
}
.quick > button {
  font-family: var(--mono);
  font-size: 11px;
  padding: 7px 9px;
}
.custom {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  margin-left: auto;
}
.custom input {
  width: 48px;
  background: var(--panel-2);
  border: 1px solid var(--line-hi);
  color: var(--ink);
  font-family: var(--mono);
  font-size: 12px;
  padding: 6px;
}
.ctrl {
  display: flex;
  gap: 8px;
}
.ctrl button {
  flex: 1;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 9px;
}
.ctrl button:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
</style>
```

- [ ] **Step 4: `DirectorView.vue` 挂载**

在 import 区(`import DirectorMessages ...` 那一带)加:

```ts
import DirectorTimer from "../components/DirectorTimer.vue";
```

在模板 `<aside class="rail">` 里 `<LocalControlPanel />` 之后加一行:

```vue
      <DirectorTimer />
```

- [ ] **Step 5: 前端构建**

Run `pnpm build`。Expected:成功。

- [ ] **Step 6: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/utils/api.ts src/stores/appState.ts src/components/DirectorTimer.vue src/views/DirectorView.vue
git commit -m "feat(director): show-timer card (durations + start/pause/reset, local tick)"
```

---

### Task 3: 主持人状态条计时读数

**Files:** `src/presenter/presenter.html`、`src/presenter/presenter.css`、`src/presenter/presenter.js`

- [ ] **Step 1: `presenter.html` 状态条加计时格**

把状态条里:

```html
          <span class="stat">PG <b id="slide-pos">--/--</b></span>
        </span>
```

替换为:

```html
          <span class="stat">PG <b id="slide-pos">--/--</b></span>
          <span class="stat" id="timer-stat" hidden>计时 <b id="timer-val">--</b></span>
        </span>
```

- [ ] **Step 2: `presenter.css` 加超时红(文件末尾追加)**

```css
/* ─── show timer (status bar) ─── */
#timer-stat.over b {
  color: var(--danger);
}
```

- [ ] **Step 3: `presenter.js` —— el 引用**

在 `el` 对象里 `dirBannerX: ...,` 之后加:

```js
    timerStat: document.getElementById("timer-stat"),
    timerVal: document.getElementById("timer-val"),
```

- [ ] **Step 4: `presenter.js` —— 轮询里处理 timer**

把 poll 里:

```js
        render();
        handleDirectorMessage(s.director_message);
      })
```

替换为:

```js
        render();
        handleDirectorMessage(s.director_message);
        handleTimer(s.timer);
      })
```

- [ ] **Step 5: `presenter.js` —— 计时逻辑 + 本地走秒(在 `// ── boot ──` 之前加)**

```js
  // ── show timer (status bar, local tick) ──
  var timerBase = null; // {remaining, running, at}
  function handleTimer(t) {
    if (!t) { timerBase = null; el.timerStat.hidden = true; return; }
    timerBase = { remaining: t.remaining_ms, running: t.running, at: Date.now() };
    el.timerStat.hidden = false;
  }
  function fmtTimer(ms) {
    var neg = ms < 0;
    var s = Math.floor(Math.abs(ms) / 1000);
    var m = Math.floor(s / 60);
    s = s % 60;
    return (neg ? "+" : "") + m + ":" + (s < 10 ? "0" : "") + s;
  }
  function tickTimer() {
    if (!timerBase) return;
    var live = timerBase.running ? timerBase.remaining - (Date.now() - timerBase.at) : timerBase.remaining;
    el.timerVal.textContent = fmtTimer(live);
    el.timerStat.classList.toggle("over", live < 0);
  }
  setInterval(tickTimer, 250);
```

- [ ] **Step 6: 交叉编译(内嵌路径不变)+ 前端构建**

Run 交叉编译(0 error)+ `pnpm build`(成功)。

- [ ] **Step 7: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.html src/presenter/presenter.css src/presenter/presenter.js
git commit -m "feat(presenter): show-timer readout in status bar (status-fed, local tick, red overrun)"
```

---

## Self-Review

- **Spec 覆盖:** §2 状态/纯函数 → Task 1;§3 命令 → Task 1;§4 status 折入 + 客户端走秒 → Task 1(后端)+ Task 2/3(前端插值);§5 导播卡 → Task 2、主持人读数 → Task 3。
- **类型/契约一致:** `TimerView{remaining_ms,running,duration_ms}`(Rust serialize)↔ `get_status`/`/api/status` 的 `timer` ↔ `api.ts TimerView`/`appState.timer`/presenter `t.remaining_ms`。`timer_start(ms)` 单字参数 `ms`(避免 camelCase/snake_case 歧义)↔ `api.timerStart(ms)` 传 `{ ms }` ↔ 命令签名 `ms: u64`。
- **死锁:** `get_status`/`/api/status` 在持锁作用域内**内联** `timer_view_of(i.timer..)`,不调用会再加锁的方法;`now_ms`/`timer_remaining_ms`/`timer_view_of` 均纯函数。
- **Placeholder:** 无;新组件给整文件,其余确切插入/替换。
- **范围:** 不持久化;不动锁/COM/消息/捕获。
