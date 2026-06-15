# 快捷预制消息 + 导播软弹窗 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 主持人点预制消息发给导播,导播即使最小化也以置顶软弹窗(不抢焦点、6 秒自关)看到;预制由导播编辑、同步主持人。

**Architecture:** Rust 侧:`AppState` 持预制列表 + `mpsc` 发送端;HTTP 收消息后入通道;`lib.rs` 接收任务持 `AppHandle`,显示独立 `toast` 窗(`index.html#/toast`)并任务栏闪。前端:主持人加第三页 MSG;导播加编辑卡;`ToastView` 渲染软弹窗。详见 `docs/superpowers/specs/2026-06-16-quick-messages-design.md`。

**Tech Stack:** Rust(Tauri 2 窗口/事件 API、tokio mpsc)、Axum、Vue 3(hash router)、原生 JS 主持人页。

**验证命令:**
- 原生测试(纯逻辑 + 含 tauri 的 lib,tauri-dev 环境):
  ```bash
  P=/data/sunyunbo/miniconda3/envs/tauri-dev; \
  PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH \
    cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml
  ```
- 交叉编译:
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 \
    PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
    cargo xwin build --target x86_64-pc-windows-msvc
  ```
- 前端构建:
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote && pnpm build
  ```

## 文件结构

| 文件 | 改动 |
|---|---|
| `src-tauri/src/app_state.rs` | `PresetMessage`/`HostMessage` 类型;`Inner.presets`;`AppState.message_tx`;`new(tx)`;更新测试 |
| `src-tauri/src/presets.rs` | **新增**:`preset_id`/`from_texts`/`default_presets`/`load`/`save`(+ 单测) |
| `src-tauri/src/lib.rs` | `mod presets`;建 mpsc 通道;setup 载入预制 + spawn 接收任务(显示 toast + 任务栏闪);注册命令 |
| `src-tauri/tauri.conf.json` | 主窗加 `label:"main"`;新增 `toast` 窗 |
| `src-tauri/src/routes.rs` | `GET /api/messages`、`POST /api/message`(仅 token) |
| `src-tauri/src/commands.rs` | `get_presets` / `set_presets`(持久化) |
| `src/views/ToastView.vue` | **新增**:软弹窗视图 |
| `src/router.ts` / `src/App.vue` | `/toast` 路由 + bare 外壳 |
| `src/components/DirectorMessages.vue` | **新增**:预制编辑卡 + 提示音开关 |
| `src/views/DirectorView.vue` | 栏内挂载 `DirectorMessages` |
| `src/utils/api.ts` | `PresetMessage` 类型 + `getPresets`/`setPresets` |
| `src/presenter/presenter.{html,css,js}` | 第三页 MSG + 拉取/发送 |

---

### Task 1: 后端基座(类型 + 持久化 + 通道 + toast 窗)

**Files:**
- Modify: `src-tauri/src/app_state.rs`
- Create: `src-tauri/src/presets.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: `presets.rs`(新增,先含失败测试再实现)**

新建 `src-tauri/src/presets.rs`,完整内容:

```rust
//! 预制快捷消息的默认种子与持久化(spec §2/§5)。

use std::path::Path;

use crate::app_state::PresetMessage;

/// 从文本派生稳定 id(FNV-1a 32 位 → 8 位十六进制)。纯函数,便于单测。
pub fn preset_id(text: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for b in text.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

/// 把一组文本构造为预制消息(去首尾空白、跳过空串;id 由文本派生)。
pub fn from_texts(texts: &[String]) -> Vec<PresetMessage> {
    texts
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| PresetMessage { id: preset_id(t), text: t.to_string() })
        .collect()
}

/// 首次默认种子。
pub fn default_presets() -> Vec<PresetMessage> {
    from_texts(&[
        "请加快节奏".into(),
        "请放慢".into(),
        "时间到".into(),
        "下一位准备".into(),
        "麦克风没声音".into(),
        "收到 / OK".into(),
    ])
}

/// 从磁盘读取;不存在或解析失败/为空则返回默认种子。
pub fn load(path: &Path) -> Vec<PresetMessage> {
    let parsed: Option<Vec<PresetMessage>> = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    match parsed {
        Some(v) if !v.is_empty() => v,
        _ => default_presets(),
    }
}

/// 写回磁盘(自动建父目录)。
pub fn save(path: &Path, presets: &[PresetMessage]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(presets).unwrap_or_else(|_| "[]".into());
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_stable_unique_hex() {
        assert_eq!(preset_id("时间到"), preset_id("时间到"));
        assert_ne!(preset_id("a"), preset_id("b"));
        assert_eq!(preset_id("x").len(), 8);
    }

    #[test]
    fn from_texts_trims_and_skips_empty() {
        let v = from_texts(&["  hi  ".into(), "".into(), "  ".into(), "yo".into()]);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].text, "hi");
        assert_eq!(v[1].text, "yo");
    }

    #[test]
    fn default_nonempty() {
        assert!(!default_presets().is_empty());
    }
}
```

- [ ] **Step 2: `app_state.rs` 加类型/字段/签名,并更新测试**

在 `app_state.rs` 顶部 `use` 区(`use crate::log::LogEntry;` 后)加:

```rust
use tokio::sync::mpsc::UnboundedSender;
```

在 `TargetWindow` 定义之后加两个类型:

```rust
/// 预制快捷消息(导播拥有、可编辑、同步给主持人)。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresetMessage {
    pub id: String,
    pub text: String,
}

/// 一条已收到的消息。本期 from 恒为 "presenter";字段保留以备导播→主持人。
#[derive(Clone, Debug, Serialize)]
pub struct HostMessage {
    pub from: String,
    pub text: String,
    pub at_ms: u64,
}
```

在 `Inner` 结构体里(`pub logs: Vec<LogEntry>,` 之后)加:

```rust
    /// 导播可编辑的预制快捷消息(同步给主持人)。
    pub presets: Vec<PresetMessage>,
```

在 `AppState` 结构体里(`pub capture: ...` 之后)加:

```rust
    /// 收到主持人消息时投递给 Tauri 接收任务(显示软弹窗)。
    pub message_tx: UnboundedSender<HostMessage>,
```

把 `AppState::new` 改为:

```rust
    pub fn new(token: String, server_port: u16, message_tx: UnboundedSender<HostMessage>) -> Self {
        AppState {
            inner: Mutex::new(Inner {
                selected_window: None,
                // 默认:远程关闭(日常总闸)、强制锁释放(仅紧急用)、未解锁。
                // 导播开「远程控制」一步即放行,主持人再 ARM 即可全功能。
                remote_enabled: false,
                control_locked: false,
                presenter_unlocked: false,
                active_client_id: None,
                token,
                server_port,
                logs: Vec::new(),
                presets: crate::presets::default_presets(),
            }),
            capture: crate::capture::CaptureManager::new(),
            message_tx,
        }
    }
```

把测试里两处 `let st = AppState::new("t".into(), 0);` 各自替换为(测试构造一个丢弃接收端的通道):

```rust
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let st = AppState::new("t".into(), 0, tx);
```

- [ ] **Step 3: 跑测试确认通过**

Run(原生测试命令)。Expected:`presets` 的 3 个新测试 + 既有(含已改的 app_state)全绿(共 23 passed)。

- [ ] **Step 4: `lib.rs` 接通道 + toast 接收任务**

把 `lib.rs` 顶部模块声明区加入(与其它 `mod` 并列):

```rust
mod presets;
```

在 `use app_state::AppState;` 之后加:

```rust
use tauri::{Emitter, Manager};
```

把 `run()` 里从 `let token = ...` 到 `.setup(...)` 这段替换为:

```rust
    let token = auth::generate_token();
    let (message_tx, mut message_rx) =
        tokio::sync::mpsc::unbounded_channel::<app_state::HostMessage>();
    let state = Arc::new(AppState::new(token, DEFAULT_PORT, message_tx));
    let server_state = state.clone();
    let preset_state = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            // 后台启动局域网 Web 服务(主持人端入口)。
            let st = server_state.clone();
            tauri::async_runtime::spawn(async move {
                web_server::run(st).await;
            });

            // 载入持久化的预制消息(覆盖默认种子)。
            if let Ok(dir) = app.path().app_config_dir() {
                let loaded = presets::load(&dir.join("presets.json"));
                if let Ok(mut i) = preset_state.inner.lock() {
                    i.presets = loaded;
                }
            }

            // 接收主持人消息 → 顶部居中软弹窗 + 任务栏闪(均不抢焦点)。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(msg) = message_rx.recv().await {
                    if let Some(toast) = handle.get_webview_window("toast") {
                        let wsize = toast
                            .outer_size()
                            .unwrap_or(tauri::PhysicalSize { width: 380, height: 120 });
                        if let Ok(Some(mon)) = toast.primary_monitor() {
                            let msize = mon.size();
                            let x = ((msize.width as i32) - (wsize.width as i32)) / 2;
                            let _ = toast.set_position(tauri::PhysicalPosition { x, y: 40 });
                        }
                        let _ = toast.show();
                        let _ = handle.emit_to("toast", "host-message", msg.clone());
                    }
                    if let Some(main) = handle.get_webview_window("main") {
                        let _ = main
                            .request_user_attention(Some(tauri::UserAttentionType::Informational));
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::list_windows,
            commands::auto_detect_ppt,
            commands::select_window,
            commands::set_remote_enabled,
            commands::force_lock,
            commands::local_next,
            commands::local_prev,
            commands::local_black,
            commands::local_restore,
            commands::get_logs,
            commands::vba_list_macros,
            commands::vba_run_macro,
            commands::get_slide_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

> 注:`get_presets`/`set_presets` 在 Task 3 再加入 `generate_handler!`。本任务保持原命令列表。

- [ ] **Step 5: `tauri.conf.json` 加 toast 窗 + 主窗 label**

把 `app.windows` 数组(第 13-21 行)替换为:

```json
    "windows": [
      {
        "label": "main",
        "title": "StageDeck Remote · 导播端",
        "width": 1100,
        "height": 760,
        "minWidth": 900,
        "minHeight": 640
      },
      {
        "label": "toast",
        "url": "index.html#/toast",
        "width": 380,
        "height": 120,
        "visible": false,
        "decorations": false,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "focus": false,
        "resizable": false,
        "shadow": false
      }
    ],
```

- [ ] **Step 6: 交叉编译 + 原生测试**

Run 交叉编译命令(Expected:0 error)。Run 原生测试命令(Expected:23 passed)。

- [ ] **Step 7: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/app_state.rs src-tauri/src/presets.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "feat(messages): backend foundation — presets, mpsc channel, toast window + receiver"
```

---

### Task 2: HTTP 端点(同步 + 发送)

**Files:**
- Modify: `src-tauri/src/routes.rs`

- [ ] **Step 1: 注册两条路由**

在 `router()` 的链式 `.route(...)` 里(`.route("/api/action", post(action))` 之后)加:

```rust
        .route("/api/messages", get(messages))
        .route("/api/message", post(message))
```

- [ ] **Step 2: 加 `now_ms` 辅助**

在 `now_secs()` 函数之后加:

```rust
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
```

- [ ] **Step 3: 加两个处理器(文件末尾、`#[cfg(test)]` 之前或末尾追加)**

```rust
// ---- 快捷消息(主持人 → 导播,仅 token,不受三层锁限制) -------------------

async fn messages(State(state): State<Arc<AppState>>, Query(q): Query<TokenQuery>) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    let presets = state
        .inner
        .lock()
        .map(|i| i.presets.clone())
        .unwrap_or_default();
    Json(presets).into_response()
}

#[derive(Deserialize)]
struct MessageReq {
    token: Option<String>,
    #[allow(dead_code)]
    client_id: Option<String>,
    id: String,
}

async fn message(State(state): State<Arc<AppState>>, Json(req): Json<MessageReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    // 只允许发送预制列表里的消息:校验 id,取服务端当前文本。
    let text = state.inner.lock().ok().and_then(|i| {
        i.presets.iter().find(|p| p.id == req.id).map(|p| p.text.clone())
    });
    let Some(text) = text else {
        return json_err(StatusCode::BAD_REQUEST, "unknown_message");
    };
    let msg = crate::app_state::HostMessage {
        from: "presenter".to_string(),
        text: text.clone(),
        at_ms: now_ms(),
    };
    let _ = state.message_tx.send(msg);
    crate::log::event(&state, format!("收到主持人消息：{text}"));
    ok_json()
}
```

- [ ] **Step 4: 交叉编译 + 原生测试**

Run 交叉编译(0 error)+ 原生测试(23 passed)。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/routes.rs
git commit -m "feat(routes): GET /api/messages + POST /api/message (token-only, preset-validated)"
```

---

### Task 3: IPC 命令(读取/编辑预制)

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: `commands.rs` 加 Manager import**

把 `use tauri::State;` 改为:

```rust
use tauri::{Manager, State};
```

- [ ] **Step 2: 加两个命令(文件末尾)**

```rust
/// 导播 GUI:读取预制快捷消息列表。
#[tauri::command]
pub fn get_presets(state: State<Arc<AppState>>) -> Vec<crate::app_state::PresetMessage> {
    state.inner.lock().map(|i| i.presets.clone()).unwrap_or_default()
}

/// 导播 GUI:用一组文本替换预制消息(id 服务端派生),并持久化。
#[tauri::command]
pub fn set_presets(app: tauri::AppHandle, state: State<Arc<AppState>>, texts: Vec<String>) {
    let presets = crate::presets::from_texts(&texts);
    if let Ok(mut i) = state.inner.lock() {
        i.presets = presets.clone();
    }
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = crate::presets::save(&dir.join("presets.json"), &presets);
    }
    log::event(&state, "已更新快捷消息预制");
}
```

- [ ] **Step 3: `lib.rs` 注册命令**

在 `tauri::generate_handler![ ... ]` 列表里 `commands::get_slide_state,` 之后加:

```rust
            commands::get_presets,
            commands::set_presets,
```

- [ ] **Step 4: 交叉编译 + 原生测试**

Run 交叉编译(0 error)+ 原生测试(23 passed)。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): get_presets/set_presets (persisted to app config dir)"
```

---

### Task 4: 软弹窗前端(ToastView + 路由 + bare 外壳)

**Files:**
- Create: `src/views/ToastView.vue`
- Modify: `src/router.ts`
- Modify: `src/App.vue`

- [ ] **Step 1: 新建 `src/views/ToastView.vue`**

```vue
<script setup lang="ts">
// 置顶软弹窗:监听 host-message 事件,显示约 6 秒后自关;不抢焦点。
import { ref, onMounted, onUnmounted } from "vue";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface HostMessage {
  from: string;
  text: string;
  at_ms: number;
}

const DURATION = 6000;
const text = ref("");
const time = ref("");
const extra = ref(0); // 连发叠加计数
const progress = ref(100);

let unlisten: UnlistenFn | null = null;
let hideTimer: number | undefined;
let tick: number | undefined;
let deadline = 0;

function fmtTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => n.toString().padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}`;
}

function chime() {
  if (localStorage.getItem("sd.dir.toastChime") === "off") return;
  try {
    const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    const ac = new Ctx();
    const o = ac.createOscillator();
    const g = ac.createGain();
    o.frequency.value = 880;
    o.connect(g);
    g.connect(ac.destination);
    g.gain.setValueAtTime(0.0001, ac.currentTime);
    g.gain.exponentialRampToValueAtTime(0.15, ac.currentTime + 0.02);
    g.gain.exponentialRampToValueAtTime(0.0001, ac.currentTime + 0.25);
    o.start();
    o.stop(ac.currentTime + 0.26);
  } catch {
    /* 音频不可用则静默 */
  }
}

function show(msg: HostMessage) {
  if (text.value) extra.value += 1; // 已有一条在显示 → 叠加计数
  text.value = msg.text;
  time.value = fmtTime(msg.at_ms);
  deadline = Date.now() + DURATION;
  progress.value = 100;
  if (hideTimer) window.clearTimeout(hideTimer);
  hideTimer = window.setTimeout(dismiss, DURATION);
  chime();
}

function dismiss() {
  if (hideTimer) window.clearTimeout(hideTimer);
  extra.value = 0;
  text.value = "";
  void getCurrentWindow().hide();
}

async function openMain() {
  try {
    const w = await Window.getByLabel("main");
    if (w) {
      await w.unminimize();
      await w.setFocus();
    }
  } catch {
    /* ignore */
  }
  dismiss();
}

onMounted(async () => {
  unlisten = await listen<HostMessage>("host-message", (e) => show(e.payload));
  tick = window.setInterval(() => {
    if (!text.value) return;
    progress.value = Math.max(0, ((deadline - Date.now()) / DURATION) * 100);
  }, 100);
});
onUnmounted(() => {
  if (unlisten) unlisten();
  if (hideTimer) window.clearTimeout(hideTimer);
  if (tick) window.clearInterval(tick);
});
</script>

<template>
  <div class="toast" @click="openMain">
    <div class="accent"></div>
    <div class="head">
      <span class="who">● 主持人 · {{ time }}<span v-if="extra"> · +{{ extra }}</span></span>
      <button class="x" @click.stop="dismiss">✕</button>
    </div>
    <div class="msg">{{ text }}</div>
    <div class="prog"><i :style="{ width: progress + '%' }"></i></div>
  </div>
</template>

<style scoped>
.toast {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  border: 1px solid var(--line-hi);
  cursor: pointer;
  overflow: hidden;
  font-family: var(--sans);
}
.accent {
  height: 3px;
  background: var(--danger);
  flex: 0 0 auto;
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 7px 12px 2px;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--ink-dim);
}
.x {
  appearance: none;
  background: transparent;
  border: none;
  color: var(--ink-faint);
  cursor: pointer;
  font-size: 12px;
  padding: 2px 4px;
}
.msg {
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  padding: 2px 14px 8px;
  font-size: 20px;
  font-weight: 700;
  line-height: 1.2;
  color: var(--ink);
}
.prog {
  flex: 0 0 auto;
  height: 2px;
  background: var(--line);
}
.prog i {
  display: block;
  height: 100%;
  background: var(--ink);
}
</style>
```

- [ ] **Step 2: `router.ts` 加 `/toast` 路由**

把 `router.ts` 改为(加 import 与路由项):

```ts
import { createRouter, createWebHashHistory } from "vue-router";

import DirectorView from "./views/DirectorView.vue";
import ToastView from "./views/ToastView.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "director", component: DirectorView },
    { path: "/toast", name: "toast", component: ToastView, meta: { bare: true } },
  ],
});
```

> 若 `router.ts` 现有结构略有差异,只需:① import `ToastView`;② 在 `routes` 数组里追加 `{ path:"/toast", name:"toast", component: ToastView, meta:{ bare:true } }`。其余保持。

- [ ] **Step 3: `App.vue` bare 外壳**

把 `App.vue` 的 `<template>` 改为(bare 路由只渲染 router-view,无顶栏/背景):

```vue
<template>
  <router-view v-if="$route.meta.bare" />
  <div v-else class="app">
    <header class="topbar">
      <strong class="brand">STAGEDECK<span class="brand-sub">&nbsp;·&nbsp;DIRECTOR</span></strong>
    </header>
    <router-view />
  </div>
</template>
```

(`<script setup>` 与 `<style>` 不变;`$route` 在模板中由 vue-router 全局提供。)

- [ ] **Step 4: 前端构建**

Run `pnpm build`。Expected:`vue-tsc` 通过(`@tauri-apps/api/window`/`event` 已是依赖)、`vite build` 成功。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/views/ToastView.vue src/router.ts src/App.vue
git commit -m "feat(toast): ToastView soft popup + /toast bare route"
```

---

### Task 5: 导播编辑卡(DirectorMessages + 挂载 + 类型)

**Files:**
- Modify: `src/utils/api.ts`
- Create: `src/components/DirectorMessages.vue`
- Modify: `src/views/DirectorView.vue`

- [ ] **Step 1: `api.ts` 加类型与方法**

在 `SlideState` 接口之后加:

```ts
export interface PresetMessage {
  id: string;
  text: string;
}
```

在 `export const api = { ... }` 里 `getSlideState: ...` 之后加:

```ts
  getPresets: () => invoke<PresetMessage[]>("get_presets"),
  setPresets: (texts: string[]) => invoke<void>("set_presets", { texts }),
```

- [ ] **Step 2: 新建 `src/components/DirectorMessages.vue`**

```vue
<script setup lang="ts">
// 快捷消息预制编辑(导播)+ 提示音开关。
import { ref, onMounted } from "vue";

import { api, type PresetMessage } from "../utils/api";

const presets = ref<PresetMessage[]>([]);
const editing = ref(false);
const drafts = ref<string[]>([]);
const chimeOn = ref(localStorage.getItem("sd.dir.toastChime") !== "off");
const error = ref<string | null>(null);

async function load() {
  try {
    presets.value = await api.getPresets();
    error.value = null;
  } catch (e) {
    error.value = String(e);
  }
}
function startEdit() {
  drafts.value = presets.value.map((p) => p.text);
  editing.value = true;
}
function addRow() {
  drafts.value.push("");
}
function removeRow(i: number) {
  drafts.value.splice(i, 1);
}
async function save() {
  try {
    await api.setPresets(drafts.value);
    editing.value = false;
    await load();
  } catch (e) {
    error.value = String(e);
  }
}
function cancel() {
  editing.value = false;
}
function toggleChime() {
  chimeOn.value = !chimeOn.value;
  localStorage.setItem("sd.dir.toastChime", chimeOn.value ? "on" : "off");
}

onMounted(load);
</script>

<template>
  <section class="card">
    <h2>快捷消息</h2>

    <template v-if="!editing">
      <ul class="list">
        <li v-for="p in presets" :key="p.id">{{ p.text }}</li>
        <li v-if="!presets.length" class="empty">无预制消息</li>
      </ul>
      <div class="row">
        <button @click="startEdit">编辑</button>
        <button class="chime" :class="{ on: chimeOn }" @click="toggleChime">
          提示音：{{ chimeOn ? "开" : "关" }}
        </button>
      </div>
    </template>

    <template v-else>
      <div class="edit">
        <div class="erow" v-for="(_, i) in drafts" :key="i">
          <input v-model="drafts[i]" placeholder="消息文本" />
          <button class="del" @click="removeRow(i)">✕</button>
        </div>
        <button class="add" @click="addRow">+ 添加</button>
      </div>
      <div class="row">
        <button @click="save">保存</button>
        <button @click="cancel">取消</button>
      </div>
    </template>

    <p v-if="error" class="err">{{ error }}</p>
  </section>
</template>

<style scoped>
.list {
  list-style: none;
  margin: 0 0 10px;
  padding: 0;
  display: grid;
  gap: 4px;
}
.list li {
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.04em;
  color: var(--ink-dim);
  padding: 4px 0;
  border-bottom: 1px solid var(--line);
}
.list li.empty {
  color: var(--ink-faint);
  text-transform: uppercase;
  letter-spacing: 0.12em;
}
.row {
  display: flex;
  gap: 8px;
}
.row button {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px 10px;
}
.chime.on {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}
.edit {
  display: grid;
  gap: 6px;
  margin-bottom: 10px;
}
.erow {
  display: flex;
  gap: 6px;
}
.erow input {
  flex: 1 1 auto;
  background: var(--panel-2);
  border: 1px solid var(--line-hi);
  color: var(--ink);
  font-family: var(--sans);
  font-size: 13px;
  padding: 7px 9px;
}
.del {
  flex: 0 0 auto;
  padding: 0 10px;
}
.add {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 7px 10px;
}
.err {
  margin: 8px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
```

- [ ] **Step 3: `DirectorView.vue` 挂载编辑卡**

在 `DirectorView.vue` 的 import 区加:

```ts
import DirectorMessages from "../components/DirectorMessages.vue";
```

在模板 `<aside class="rail">` 里 `<LocalControlPanel />` 之后加一行:

```vue
      <DirectorMessages />
```

- [ ] **Step 4: 前端构建**

Run `pnpm build`。Expected:成功。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/utils/api.ts src/components/DirectorMessages.vue src/views/DirectorView.vue
git commit -m "feat(director): quick-message preset editor card + chime toggle"
```

---

### Task 6: 主持人端第三页 MSG

**Files:**
- Modify: `src/presenter/presenter.html`
- Modify: `src/presenter/presenter.css`
- Modify: `src/presenter/presenter.js`

- [ ] **Step 1: `presenter.html` 加第三标签 + 消息页**

把 modeswitch(两按钮)那段:

```html
      <nav class="modeswitch">
        <button class="mode active" data-mode="main">主控 · MAIN</button>
        <button class="mode" data-mode="sync">屏幕 · SYNC</button>
      </nav>
```

替换为(加第三个):

```html
      <nav class="modeswitch">
        <button class="mode active" data-mode="main">主控 · MAIN</button>
        <button class="mode" data-mode="sync">屏幕 · SYNC</button>
        <button class="mode" data-mode="msg">消息 · MSG</button>
      </nav>
```

在 `<!-- ─── SYNC: screenshot fallback ─── -->` 那个 `<main class="page page-sync" ...>...</main>` 之后、`<!-- ─── ARM safety strip (shared) ─── -->` 之前,插入消息页:

```html
      <!-- ─── MSG: quick messages ─── -->
      <main class="page page-msg" id="page-msg">
        <div class="section-label">快捷消息 · 点击即发</div>
        <div class="msg-grid" id="msg-grid"></div>
        <div class="empty" id="msg-empty" hidden>暂无预制消息</div>
        <div class="msg-sent" id="msg-sent" hidden>已发送 ✓</div>
      </main>
```

- [ ] **Step 2: `presenter.css` 三路切换 + 消息样式**

把 `.modeswitch` 的 `grid-template-columns: 1fr 1fr;` 改为:

```css
  grid-template-columns: 1fr 1fr 1fr;
```

在文件末尾追加:

```css
/* ─── MSG: quick messages ─── */
.page-msg {
  overflow-y: auto;
}
.msg-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  padding: 10px 16px 14px;
}
.msg-btn {
  appearance: none;
  min-height: 64px;
  background: var(--panel-2);
  border: 1px solid var(--line-hi);
  color: var(--ink);
  font-family: var(--sans);
  font-size: 16px;
  font-weight: 600;
  padding: 10px 12px;
  cursor: pointer;
  text-align: left;
  transition: background 0.06s linear, color 0.06s linear, transform 0.04s linear;
}
.msg-btn:active:not(:disabled) {
  background: var(--ink);
  color: #000;
  transform: translateY(1px);
}
.msg-btn:disabled {
  opacity: 0.32;
}
.msg-sent {
  position: fixed;
  left: 50%;
  bottom: 92px;
  transform: translateX(-50%);
  background: var(--ink);
  color: #000;
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.1em;
  padding: 8px 16px;
  z-index: 5;
}
```

- [ ] **Step 3: `presenter.js` 拉取 + 渲染 + 发送 + 三路切换**

在 `el` 对象里(`tbtns`/`dockBtns` 那行附近)加三项:

```js
    pageMsg: document.getElementById("page-msg"),
    msgGrid: document.getElementById("msg-grid"),
    msgEmpty: document.getElementById("msg-empty"),
    msgSent: document.getElementById("msg-sent"),
```

> 注:若 presenter.js 的 `el` 末尾不是这几项也无妨,追加即可(保持对象语法正确)。

在模式切换处理器里(`el.pageSync.classList.toggle("active", mode === "sync");` 那一行之后)加:

```js
      el.pageMsg.classList.toggle("active", mode === "msg");
      if (mode === "msg") loadMessages();
```

在 `loadFrame` 相关函数附近(或文件靠后、`boot` 之前)加消息逻辑:

```js
  // ── quick messages (MSG) ──
  var sentTimer = null;
  function loadMessages() {
    api("/api/messages?token=" + encodeURIComponent(token))
      .then(handle401)
      .then(function (r) { return r ? r.json() : null; })
      .then(function (list) {
        if (!list) return;
        el.msgEmpty.hidden = list.length > 0;
        el.msgGrid.innerHTML = "";
        list.forEach(function (m) {
          var b = document.createElement("button");
          b.className = "msg-btn";
          b.setAttribute("data-id", m.id);
          b.textContent = m.text;
          el.msgGrid.appendChild(b);
        });
      })
      .catch(function () {});
  }
  function flashSent(text) {
    el.msgSent.textContent = text;
    el.msgSent.hidden = false;
    if (sentTimer) clearTimeout(sentTimer);
    sentTimer = setTimeout(function () { el.msgSent.hidden = true; }, 1400);
  }
  el.msgGrid.addEventListener("click", function (ev) {
    var b = ev.target.closest(".msg-btn");
    if (!b) return;
    api("/api/message", { id: b.getAttribute("data-id") })
      .then(function (r) { flashSent(r && r.ok ? "已发送 ✓" : "发送失败"); })
      .catch(function () { flashSent("发送失败"); });
  });
```

> 说明:消息发送**不要求 ARM/解锁**(通信),故按钮不接 `canAct`/`canTransport` 门控;在线即可点。`api()`/`handle401`/`token` 沿用现有定义。

- [ ] **Step 4: 交叉编译(内嵌路径不变)+ 前端构建**

Run 交叉编译命令(Expected:0 error)。Run `pnpm build`(Expected:成功)。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.html src/presenter/presenter.css src/presenter/presenter.js
git commit -m "feat(presenter): MSG tab — fetch presets, tap to send, sent toast"
```

---

## Self-Review

- **Spec 覆盖:** §2 模型 → Task 1;§3 HTTP → Task 2;§4 通道/toast 窗/接收 → Task 1 + Task 4(前端);§5 编辑/持久化 → Task 1(load)+ Task 3(命令/save)+ Task 5(GUI);§6 主持人 UI → Task 6;§7 路由/外壳 → Task 4。全覆盖。
- **类型/契约一致:** `PresetMessage{id,text}` 在 Rust(app_state)、`api.ts`、`presenter.js`(`m.id`/`m.text`)、HTTP `GET /api/messages` 一致;`POST /api/message {id}` 与 `MessageReq.id`、presenter `{id}` 一致;`HostMessage{from,text,at_ms}` 与 `emit_to("host-message")`、`ToastView` 的 `HostMessage` 接口一致;`set_presets(texts)` 与 `api.setPresets(texts)`、命令签名一致;`localStorage['sd.dir.toastChime']` 在 `DirectorMessages` 写、`ToastView` 读,键一致。
- **编译绿:** Task 1 改 `AppState::new` 签名的同时改了唯一调用方 `lib.rs` 与测试;`get_presets/set_presets` 在 Task 3 同时定义并注册;每个 Rust 任务以交叉编译 + `cargo test` 收尾,每个前端任务以 `pnpm build` 收尾。
- **Placeholder:** 无;新文件给完整内容,改文件给确切替换/插入块。
- **门控/安全:** `/api/messages`、`/api/message` 仅 token;`message` 校验 id ∈ 预制(主持人发不了任意文本)。三层锁、COM、既有控制逻辑不动。
- **运行时风险(spec §4):** toast "不抢焦点" 以 `focus:false`+`always_on_top`+`skip_taskbar`+仅 `show()` 实现;`WS_EX_NOACTIVATE` 兜底留待用户 Windows 实测后定夺。
