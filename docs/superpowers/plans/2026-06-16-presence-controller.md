# 在线状态 + 单控制端 + 断线提醒 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 唯一 cid + 先到为主单控制端(非控制端只读)+ 导播可释放 + 控制端断线置顶 toast。

**Architecture:** 极简在线模型(只追 `active_client_id` + `active_seen_ms`);`/api/status` 带 cid 认领、控制类接口加 `is_controller` 校验;后台 watcher 检测断线跳变→释放+toast。详见 `docs/superpowers/specs/2026-06-16-presence-controller-design.md`。

**Tech Stack:** Rust(Axum/Tauri/tokio)、Vue 3、原生 JS。

**验证命令:**
- 原生测试: `P=/data/sunyunbo/miniconda3/envs/tauri-dev; PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml`
- 交叉编译: `cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
- 前端构建: `cd /data/sunyunbo/www/StageDeckRemote && pnpm build`

---

### Task 1: 后端(在线模型 + 认领 + 控制门控 + watcher)

**Files:** `src-tauri/src/app_state.rs`、`src-tauri/src/routes.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`

- [ ] **Step 1: `app_state.rs` 字段 + 纯函数 + 测试**

在 `Inner` 里 `pub active_client_id: Option<String>,` 之后加:

```rust
    /// 当前控制端最后心跳(服务端时钟毫秒);配合 active_client_id 做在线判定。
    pub active_seen_ms: u64,
```

在 `AppState::new` 的 `Inner { ... }` 里 `active_client_id: None,` 之后加:

```rust
                active_seen_ms: 0,
```

在 `app_state.rs` 的 `#[cfg(test)]` 之前加:

```rust
/// 客户端在线判定阈值(>该值无心跳视为离线)。
pub const PRESENCE_TIMEOUT_MS: u64 = 4000;

/// 最近一次心跳是否仍算在线。纯函数。
pub fn client_online(last_seen_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_seen_ms) <= PRESENCE_TIMEOUT_MS
}

/// 记录 cid 心跳并按"先到为主 / 控制端离线可被接管"更新控制端;返回该 cid 是否为控制端。
pub fn note_presence_and_claim(inner: &mut Inner, cid: &str, now_ms: u64) -> bool {
    let controller_live =
        inner.active_client_id.is_some() && client_online(inner.active_seen_ms, now_ms);
    let is_me = inner.active_client_id.as_deref() == Some(cid);
    if is_me || !controller_live {
        inner.active_client_id = Some(cid.to_string());
        inner.active_seen_ms = now_ms;
        true
    } else {
        false
    }
}

/// 控制端当前是否在线。
pub fn controller_online(inner: &Inner, now_ms: u64) -> bool {
    inner.active_client_id.is_some() && client_online(inner.active_seen_ms, now_ms)
}
```

在 `#[cfg(test)] mod tests` 里加:

```rust
    #[test]
    fn client_online_threshold() {
        assert!(client_online(1000, 1000));
        assert!(client_online(1000, 1000 + PRESENCE_TIMEOUT_MS));
        assert!(!client_online(1000, 1000 + PRESENCE_TIMEOUT_MS + 1));
    }

    #[test]
    fn claim_first_come_takeover_viewonly() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let st = AppState::new("t".into(), 0, tx);
        let mut i = st.inner.lock().unwrap();
        // 空槽 → A 认领
        assert!(note_presence_and_claim(&mut i, "A", 1000));
        assert_eq!(i.active_client_id.as_deref(), Some("A"));
        // A 仍在线时 B 轮询 → 只读
        assert!(!note_presence_and_claim(&mut i, "B", 1500));
        assert_eq!(i.active_client_id.as_deref(), Some("A"));
        // A 在线时 A 自己轮询 → 保持 + 刷新心跳
        assert!(note_presence_and_claim(&mut i, "A", 2000));
        // A 超时离线后 B 轮询 → 接管
        assert!(note_presence_and_claim(&mut i, "B", 2000 + PRESENCE_TIMEOUT_MS + 1));
        assert_eq!(i.active_client_id.as_deref(), Some("B"));
    }
```

- [ ] **Step 2: 跑测试确认通过**

Run 原生测试。Expected:2 个新测试通过,既有全绿(共 **26 passed**)。

- [ ] **Step 3: `routes.rs` —— TokenQuery 加 cid + is_controller 助手**

把:

```rust
#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}
```

改为:

```rust
#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
    cid: Option<String>,
}
```

在 `token_ok` 函数之后加:

```rust
/// 请求体 client_id 是否为当前控制端(单控制端门控)。
fn is_controller(state: &AppState, cid: &Option<String>) -> bool {
    let c = cid.as_deref();
    state
        .inner
        .lock()
        .map(|i| c.is_some() && i.active_client_id.as_deref() == c)
        .unwrap_or(false)
}
```

- [ ] **Step 4: `routes.rs` —— status 认领 + is_controller 字段**

把 `status` 处理器的 body 块:

```rust
    let body = {
        let i = state.inner.lock().expect("state lock poisoned");
        json!({
            "ok": true,
            "ppt_window_found": i.selected_window.is_some(),
            "remote_enabled": i.remote_enabled,
            "control_locked": i.control_locked,
            "presenter_unlocked": i.presenter_unlocked,
            "active_client_id": i.active_client_id,
            "director_message": i.director_message.clone(),
            "timer": i.timer.as_ref().map(|t| crate::app_state::timer_view_of(t, crate::app_state::now_ms())),
            "server_time": now_secs(),
        })
    };
```

替换为:

```rust
    let body = {
        let mut i = state.inner.lock().expect("state lock poisoned");
        let is_controller = match &q.cid {
            Some(cid) => crate::app_state::note_presence_and_claim(&mut i, cid, crate::app_state::now_ms()),
            None => false,
        };
        json!({
            "ok": true,
            "ppt_window_found": i.selected_window.is_some(),
            "remote_enabled": i.remote_enabled,
            "control_locked": i.control_locked,
            "presenter_unlocked": i.presenter_unlocked,
            "active_client_id": i.active_client_id,
            "is_controller": is_controller,
            "director_message": i.director_message.clone(),
            "timer": i.timer.as_ref().map(|t| crate::app_state::timer_view_of(t, crate::app_state::now_ms())),
            "server_time": now_secs(),
        })
    };
```

- [ ] **Step 5: `routes.rs` —— 6 个控制接口加 controller 校验**

每个都在「token 校验块」之后、原有下一行之前,插入控制端校验。逐个替换:

**click**(把下面这段):

```rust
async fn click(State(state): State<Arc<AppState>>, Json(req): Json<ClickReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    // 坐标必须是 [0,1] 归一化值（spec §13.3）。
```

替换为:

```rust
async fn click(State(state): State<Arc<AppState>>, Json(req): Json<ClickReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    // 坐标必须是 [0,1] 归一化值（spec §13.3）。
```

**lock**:

```rust
async fn lock(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if let Ok(mut i) = state.inner.lock() {
```

替换为:

```rust
async fn lock(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    if let Ok(mut i) = state.inner.lock() {
```

**unlock**:

```rust
async fn unlock(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    // 只有导播放行（远程开启且未强制锁）时才允许主持人解锁。
```

替换为:

```rust
async fn unlock(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    // 只有导播放行（远程开启且未强制锁）时才允许主持人解锁。
```

**black**:

```rust
async fn black(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::set_black(true) {
```

替换为:

```rust
async fn black(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::set_black(true) {
```

**restore**:

```rust
async fn restore(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::set_black(false) {
```

替换为:

```rust
async fn restore(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::set_black(false) {
```

**action**:

```rust
async fn action(State(state): State<Arc<AppState>>, Json(req): Json<ActionReqBody>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::trigger_action(&req.shape) {
```

替换为:

```rust
async fn action(State(state): State<Arc<AppState>>, Json(req): Json<ActionReqBody>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::trigger_action(&req.shape) {
```

- [ ] **Step 6: `routes.rs` —— next/prev/page_turn 加 client_id + 控制校验**

把 `next`/`prev`/`page_turn` 三处:

```rust
async fn next(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    page_turn(&state, &req.token, true)
}

async fn prev(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    page_turn(&state, &req.token, false)
}

fn page_turn(state: &AppState, token: &Option<String>, forward: bool) -> Response {
    if !token_ok(state, token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    // 翻页与宏/点坐标同级:需三层全通(含主持人 ARM)。
    if !state.click_allowed() {
```

替换为:

```rust
async fn next(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    page_turn(&state, &req.token, &req.client_id, true)
}

async fn prev(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    page_turn(&state, &req.token, &req.client_id, false)
}

fn page_turn(
    state: &AppState,
    token: &Option<String>,
    client_id: &Option<String>,
    forward: bool,
) -> Response {
    if !token_ok(state, token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(state, client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    // 翻页与宏/点坐标同级:需三层全通(含主持人 ARM)。
    if !state.click_allowed() {
```

- [ ] **Step 7: `routes.rs` —— message 的 from**

把 message 处理器里:

```rust
    let msg = crate::app_state::HostMessage {
        from: "presenter".to_string(),
```

替换为:

```rust
    let msg = crate::app_state::HostMessage {
        from: "主持人".to_string(),
```

- [ ] **Step 8: `commands.rs` —— DirectorStatus.presenter_online + get_status + release_controller**

把 `DirectorStatus` 里 `pub timer: Option<crate::app_state::TimerView>,` 之后加:

```rust
    pub presenter_online: bool,
```

把 `get_status` 构造里 `timer: i ... ,`(以 `crate::app_state::timer_view_of(t, crate::app_state::now_ms())),` 结尾那段)之后加:

```rust
        presenter_online: crate::app_state::controller_online(&i, crate::app_state::now_ms()),
```

在 `commands.rs` 末尾加命令:

```rust
/// 导播 GUI:释放控制权(清空控制端,下一台轮询即接管)。
#[tauri::command]
pub fn release_controller(state: State<Arc<AppState>>) {
    if let Ok(mut i) = state.inner.lock() {
        i.active_client_id = None;
    }
    log::event(&state, "导播:释放控制权");
}
```

- [ ] **Step 9: `lib.rs` —— 注册命令 + 断线 watcher**

在 `tauri::generate_handler![ ... ]` 里 `commands::timer_reset,` 之后加:

```rust
            commands::release_controller,
```

在 `run()` 里 `let preset_state = state.clone();` 之后加:

```rust
    let watch_state = state.clone();
```

在 setup 闭包里(接收主持人消息那个 `tauri::async_runtime::spawn` 之后、`Ok(())` 之前)加断线 watcher:

```rust
            // 控制端断线 watcher:在线→离线跳变 → 释放控制槽 + 日志 + 置顶 toast。
            let ws = watch_state.clone();
            tauri::async_runtime::spawn(async move {
                let mut was_online = false;
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
                loop {
                    interval.tick().await;
                    let now = app_state::now_ms();
                    let online = if let Ok(mut i) = ws.inner.lock() {
                        let on = app_state::controller_online(&i, now);
                        if was_online && !on {
                            i.active_client_id = None;
                        }
                        on
                    } else {
                        was_online
                    };
                    if was_online && !online {
                        crate::log::event(&ws, "主持人已断开连接");
                        let _ = ws.message_tx.send(app_state::HostMessage {
                            from: "系统".to_string(),
                            text: "主持人已断开连接".to_string(),
                            at_ms: now,
                        });
                    }
                    was_online = online;
                }
            });
```

> 注:`app_state` 在 `lib.rs` 已 `mod app_state;`(用 `app_state::now_ms` 等)。watcher 持锁只读/改字段、不跨 `.await`;`log`/`send` 在放锁后(`online` 块结束即放锁)。

- [ ] **Step 10: 交叉编译 + 原生测试**

Run 交叉编译(0 error)+ 原生测试(**26 passed**)。

- [ ] **Step 11: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/app_state.rs src-tauri/src/routes.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(presence): first-come single controller + presence + disconnect watcher/toast"
```

---

### Task 2: 导播前端(api + store + ControlStatus + ToastView from)

**Files:** `src/utils/api.ts`、`src/stores/appState.ts`、`src/components/ControlStatus.vue`、`src/views/ToastView.vue`

- [ ] **Step 1: `api.ts` 字段 + 方法**

在 `DirectorStatus` 接口里 `timer: TimerView | null;` 之后加:

```ts
  presenter_online: boolean;
```

在 `export const api = { ... }` 里 `timerReset: ...` 之后加:

```ts
  releaseController: () => invoke<void>("release_controller"),
```

- [ ] **Step 2: `appState.ts` presenterOnline + 操作**

在 `const timer = ref<TimerView | null>(null);` 之后加:

```ts
  const presenterOnline = ref(false);
```

在 `applyStatus` 里 `timer.value = s.timer;` 之后加:

```ts
    presenterOnline.value = s.presenter_online;
```

在 `timerReset` 操作之后加:

```ts
  async function releaseController() {
    await api.releaseController();
    await refresh();
  }
```

在 `return { ... }` 里加(`timerReset,` 之后):

```ts
    presenterOnline,
    releaseController,
```

- [ ] **Step 3: `ControlStatus.vue` 在线行 + 释放按钮**

把 script 的 storeToRefs 解构:

```ts
const { remoteEnabled, controlLocked, presenterUnlocked, selectedWindow, activeClientId } =
  storeToRefs(app);
```

改为:

```ts
const { remoteEnabled, controlLocked, presenterUnlocked, selectedWindow, activeClientId, presenterOnline } =
  storeToRefs(app);
```

把模板里「当前控制端」那一行(`<li>` ... `activeClientId` ...)整体替换为下面三行(在线状态 + 控制端 id + 释放按钮):

```vue
      <li>
        <span class="k">主持人</span>
        <span class="v" :class="presenterOnline ? 'ok' : 'bad'">
          {{ presenterOnline ? "在线" : "离线" }}
        </span>
      </li>
      <li>
        <span class="k">当前控制端</span>
        <span class="v">{{ activeClientId ? activeClientId.slice(0, 6) : "—" }}</span>
      </li>
      <li class="release-row">
        <button class="release" :disabled="!activeClientId" @click="app.releaseController()">
          释放控制权
        </button>
      </li>
```

> 注:原模板里那行是 `<li><span class="k">当前控制端</span><span class="v">{{ activeClientId ?? "—" }}</span></li>`(可能带 `canControl` 的"主持人 可控制"行在其上)。**保留**其它行,仅把"当前控制端"那一行替换为上面三行。若已有"主持人 可控制/已锁定"行,保留不动(它表达三层锁,与"在线"互补)。

在 `<style scoped>` 末尾加:

```css
.release-row {
  border-bottom: none;
  padding-top: 8px;
}
.release {
  width: 100%;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px;
}
.release:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
```

- [ ] **Step 4: `ToastView.vue` 头部显示 from**

在 `<script setup>` 里 `const extra = ref(0);` 之后加:

```ts
const fromLabel = ref("主持人");
```

在 `show(msg)` 里 `time.value = fmtTime(msg.at_ms);` 之后加:

```ts
  fromLabel.value = msg.from || "主持人";
```

把模板里:

```vue
      <span class="who">● 主持人 · {{ time }}<span v-if="extra"> · +{{ extra }}</span></span>
```

替换为:

```vue
      <span class="who">● {{ fromLabel }} · {{ time }}<span v-if="extra"> · +{{ extra }}</span></span>
```

- [ ] **Step 5: 前端构建**

Run `pnpm build`。Expected:成功。

- [ ] **Step 6: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/utils/api.ts src/stores/appState.ts src/components/ControlStatus.vue src/views/ToastView.vue
git commit -m "feat(director): presenter online indicator + release-controller; toast shows sender"
```

---

### Task 3: 主持人端(cid + 控制门控 + 只读条)

**Files:** `src/presenter/presenter.html`、`src/presenter/presenter.css`、`src/presenter/presenter.js`

- [ ] **Step 1: `presenter.html` 只读提示条**

在 `<!-- ─── status bar ─── -->` 的 `<header class="statusbar">` 之前(`.frame` 开标签之后)插入:

```html
      <!-- ─── view-only banner (非控制端) ─── -->
      <div class="viewonly" id="viewonly" hidden>● 只读 · 另一台正在控制</div>
```

- [ ] **Step 2: `presenter.css` 只读条样式(文件末尾追加)**

```css
/* ─── view-only banner ─── */
.viewonly {
  flex: 0 0 auto;
  text-align: center;
  padding: 7px;
  background: var(--danger);
  color: #fff;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
}
.viewonly[hidden] {
  display: none;
}
```

- [ ] **Step 3: `presenter.js` —— cid + 发送带 cid**

在 `var token = ...;` 之后加:

```js
  var cid = localStorage.getItem("sd.presenter.cid");
  if (!cid) {
    cid = Date.now().toString(36) + Math.random().toString(36).slice(2, 10);
    localStorage.setItem("sd.presenter.cid", cid);
  }
```

把 `api` 里 POST 体的 `client_id: "presenter"` 改为 `client_id: cid`:

```js
          body: JSON.stringify(Object.assign({ token: token, client_id: cid }, body)),
```

把 status 轮询那行:

```js
      api("/api/status?token=" + encodeURIComponent(token)).then(handle401),
```

改为:

```js
      api("/api/status?token=" + encodeURIComponent(token) + "&cid=" + encodeURIComponent(cid)).then(handle401),
```

- [ ] **Step 4: `presenter.js` —— el 引用 + isController 门控 + 只读条**

在 `el` 对象里 `timerVal: ...,` 之后加:

```js
    viewonly: document.getElementById("viewonly"),
```

在 `state` 对象里 `unlocked: false, // presenter_unlocked` 之后加:

```js
    isController: false,
```

把 `canAct` / `canTransport`:

```js
  function canAct() {
    return state.online && state.allowed && state.unlocked;
  }
```

改为:

```js
  function canAct() {
    return state.online && state.allowed && state.unlocked && state.isController;
  }
  function canTransport() {
    return state.online && state.allowed && state.isController;
  }
```

> 注:若文件里已有 `canTransport`(早期版本),则只在两个函数末尾各加 ` && state.isController`;**不要**重复定义。当前版本只有 `canAct`(翻页坞也用 `canAct`),所以照上面写即可——若编译/构建报 `canTransport` 重复,删掉重复的那个。

在 `render()` 里 `var armed = state.unlocked;` 之前加(只读条 + ARM 禁用补 isController):

```js
    el.viewonly.hidden = !(state.online && !state.isController);
```

把 `render()` 里 ARM 按钮禁用那行:

```js
    el.lockBtn.disabled = !state.online || (!state.allowed && !state.unlocked);
```

改为:

```js
    el.lockBtn.disabled = !state.online || !state.isController || (!state.allowed && !state.unlocked);
```

在 poll 的 `.then` 里 `state.unlocked = state.allowed && s.presenter_unlocked;` 之后加:

```js
        state.isController = !!s.is_controller;
```

在 `setDown` 里 `state.unlocked = false;` 之后加:

```js
    state.isController = false;
```

- [ ] **Step 5: 交叉编译(内嵌路径不变)+ 前端构建**

Run 交叉编译(0 error)+ `pnpm build`(成功)。

- [ ] **Step 6: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.html src/presenter/presenter.css src/presenter/presenter.js
git commit -m "feat(presenter): unique cid, controller-gated controls, view-only banner"
```

---

## Self-Review

- **Spec 覆盖:** §2 在线模型/纯函数 → Task 1 Step 1;§3 status 认领 + 8 控制接口校验 + message from → Task 1 Step 3-7;§4 导播 presenter_online + release → Task 1 Step 8 + Task 2;§5 watcher → Task 1 Step 9;§6 前端(toast from / presenter cid+门控+只读条 / ControlStatus)→ Task 2/3。
- **类型/契约一致:** `is_controller`(JSON bool)↔ presenter `s.is_controller`/`state.isController`;`cid` 在 status query + POST `client_id` 一致;控制接口 403 `not_controller`;`presenter_online`(DirectorStatus)↔ `appState.presenterOnline` ↔ ControlStatus;`release_controller` ↔ `api.releaseController`。
- **死锁/并发:** `note_presence_and_claim` 取 `&mut Inner`(status 在持锁内调用,`let mut i`);`is_controller`/`controller_online` 短临界区;watcher 不跨 await 持锁,`log`/`send` 放锁后。
- **门控不变:** 三层锁判定 `click_allowed`/`button_allowed` 不改,只在其外**叠加**控制端校验;读类与发消息不挡(非控制端能看能发消息)。
- **Placeholder:** 无;均确切 old→new 或新增块。
