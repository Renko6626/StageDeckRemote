# 三层锁模型微调 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 降低开场门槛(默认不强锁)并让主持人 ARM 成为其侧唯一总闸(翻页/黑屏也需 ARM)。

**Architecture:** 改两处语义——`control_locked` 默认 `false`(强锁降级为紧急按钮);主持人翻页/黑屏/恢复的门控由 `button_allowed` 升为 `click_allowed`(需 ARM)。三层结构与"导播 > 主持人"优先级不变。详见 `docs/superpowers/specs/2026-06-15-lock-model-tweaks-design.md`。

**Tech Stack:** Rust(Axum/Tauri 状态)、原生 JS 主持人前端、Vue 导播前端。

**验证命令:**
- 原生测试:
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
  cd /data/sunyunbo/www/StageDeckRemote && npm run build
  ```

## 文件结构

| 文件 | 改动 |
|---|---|
| `src-tauri/src/app_state.rs` | 默认 `control_locked: false` + 注释;新增门控单测 |
| `src-tauri/src/routes.rs` | `page_turn`/`black`/`restore` 门控 `button_allowed → click_allowed` |
| `src/presenter/presenter.js` | 翻页坞门控 `canTransport → canAct`;删 `canTransport` |
| `src/components/LocalControlPanel.vue` | 强制锁按钮文案改"紧急锁定" |

---

### Task 1: `app_state.rs` 默认不强锁 + 门控单测(TDD)

**Files:**
- Modify: `src-tauri/src/app_state.rs`

- [ ] **Step 1: 先加失败测试**

在 `src-tauri/src/app_state.rs` 文件末尾追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_not_force_locked() {
        let st = AppState::new("t".into(), 0);
        let i = st.inner.lock().unwrap();
        assert!(!i.remote_enabled, "远程默认关");
        assert!(!i.control_locked, "强制锁默认释放(新默认)");
        assert!(!i.presenter_unlocked, "默认未解锁");
    }

    #[test]
    fn gating_state_machine() {
        let st = AppState::new("t".into(), 0);
        // 远程关 → 全禁
        assert!(!st.button_allowed());
        assert!(!st.click_allowed());
        // 开远程、未强锁 → 导播层放行(button_allowed 真),但主持人未 ARM → 动作仍禁
        st.inner.lock().unwrap().remote_enabled = true;
        assert!(st.button_allowed());
        assert!(!st.click_allowed());
        // 主持人 ARM → 一切动作放行
        st.inner.lock().unwrap().presenter_unlocked = true;
        assert!(st.click_allowed());
        // 紧急强锁压制一切
        st.inner.lock().unwrap().control_locked = true;
        assert!(!st.button_allowed());
        assert!(!st.click_allowed());
    }
}
```

- [ ] **Step 2: 跑测试确认 `default_is_not_force_locked` 失败**

Run(原生测试命令)。Expected:`default_is_not_force_locked` 失败于 `强制锁默认释放(新默认)`(因为当前默认 `control_locked: true`);`gating_state_machine` 也会因初始 `control_locked=true` 在 `assert!(st.button_allowed())` 处失败。

- [ ] **Step 3: 改默认值**

把 `src-tauri/src/app_state.rs` 中:

```rust
                selected_window: None,
                // 默认最安全：远程关闭、强制锁、未解锁。导播显式开启后才可控。
                remote_enabled: false,
                control_locked: true,
                presenter_unlocked: false,
```

替换为:

```rust
                selected_window: None,
                // 默认:远程关闭(日常总闸)、强制锁释放(仅紧急用)、未解锁。
                // 导播开「远程控制」一步即放行,主持人再 ARM 即可全功能。
                remote_enabled: false,
                control_locked: false,
                presenter_unlocked: false,
```

- [ ] **Step 4: 跑测试确认通过**

Run(原生测试命令)。Expected:两个新测试通过;既有 18 个仍绿 → 共 20 passed。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/app_state.rs
git commit -m "feat(app_state): default control_locked=false (force-lock = emergency); gating tests"
```

---

### Task 2: 翻页/黑屏/恢复门控升为 `click_allowed`

**Files:**
- Modify: `src-tauri/src/routes.rs`

> 三处都把 `button_allowed()` 改成 `click_allowed()`,失败原因串保持 `"control_locked"`(与 `action`/`click` 一致)。**不要**动 `unlock`(它保留 `button_allowed()`,原因串是 `"remote_disabled"`)。

- [ ] **Step 1: 改 `page_turn`(翻页)**

把 `routes.rs` 中:

```rust
fn page_turn(state: &AppState, token: &Option<String>, forward: bool) -> Response {
    if !token_ok(state, token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !state.button_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
```

替换为:

```rust
fn page_turn(state: &AppState, token: &Option<String>, forward: bool) -> Response {
    if !token_ok(state, token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    // 翻页与宏/点坐标同级:需三层全通(含主持人 ARM)。
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
```

- [ ] **Step 2: 改 `black`(黑屏)**

把 `routes.rs` 中:

```rust
async fn black(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !state.button_allowed() {
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
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::set_black(true) {
```

- [ ] **Step 3: 改 `restore`(恢复)**

把 `routes.rs` 中:

```rust
async fn restore(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !state.button_allowed() {
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
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::set_black(false) {
```

- [ ] **Step 4: 交叉编译 + 原生测试**

Run 交叉编译命令(Expected:0 error)。Run 原生测试命令(Expected:20 passed)。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/routes.rs
git commit -m "feat(routes): page-turn/black/restore now require click_allowed (presenter ARM)"
```

---

### Task 3: 前端翻页坞门控改 `canAct`

**Files:**
- Modify: `src/presenter/presenter.js`

- [ ] **Step 1: 删除 `canTransport`,翻页坞改用 `canAct`**

把 `presenter.js` 中的门控函数:

```js
  // ── gating ──
  function canAct() {
    return state.online && state.allowed && state.unlocked;
  }
  function canTransport() {
    return state.online && state.allowed;
  }
```

替换为:

```js
  // ── gating ──（主持人侧一切操作都需 ARM 解锁)
  function canAct() {
    return state.online && state.allowed && state.unlocked;
  }
```

- [ ] **Step 2: render() 里翻页坞禁用改用 `canAct`**

把 `presenter.js` 中:

```js
    // 翻页坞:canTransport(不要求 ARM)
    var t = !canTransport();
    el.dockBtns.forEach(function (b) {
      b.disabled = t;
    });
```

替换为:

```js
    // 翻页坞:与宏按钮一致,需 ARM(canAct)。未解锁则全禁用 →「锁就全锁」。
    var t = !canAct();
    el.dockBtns.forEach(function (b) {
      b.disabled = t;
    });
```

- [ ] **Step 3: 前端构建**

Run `npm run build`。Expected:构建成功(无 `canTransport` 残留引用)。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.js
git commit -m "feat(presenter): dock transport gates on canAct (ARM required) — lock means lock-all"
```

---

### Task 4: 导播强制锁按钮文案改"紧急"

**Files:**
- Modify: `src/components/LocalControlPanel.vue`

- [ ] **Step 1: 改文案**

把 `LocalControlPanel.vue` 中:

```html
      <button class="big" :class="{ danger: controlLocked }" @click="toggleLock">
        {{ controlLocked ? "已强制锁定（点击释放）" : "强制锁定" }}
      </button>
```

替换为:

```html
      <button class="big" :class="{ danger: controlLocked }" @click="toggleLock">
        {{ controlLocked ? "已紧急锁定（点击释放）" : "紧急锁定" }}
      </button>
```

- [ ] **Step 2: 前端构建**

Run `npm run build`。Expected:构建成功。

- [ ] **Step 3: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/LocalControlPanel.vue
git commit -m "style(director): relabel force-lock button as 紧急锁定 (emergency)"
```

---

## Self-Review

- **Spec 覆盖:** §3.1 默认+测试 → Task 1;§3.2 翻页/黑屏/恢复门控 → Task 2(unlock 保留 button_allowed,已在说明中强调不动);§3.3 前端翻页坞 → Task 3;§3.4 文案 → Task 4。全覆盖。
- **类型/语义一致:** `click_allowed`/`button_allowed` 方法签名不变;Task 2 只改调用点。`canAct` 在 Task 3 保留、`canTransport` 删除且无其它引用(render 是唯一用处)。
- **Placeholder:** 无。每步给出完整代码与确切命令。
- **优先级不变:** 仅改默认值与翻页门控层级;导播 > 主持人、清解锁行为均未改。
