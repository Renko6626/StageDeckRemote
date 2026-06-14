# 完整 COM 工具链 Implementation Plan (backend)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** 把 spike 演进为成熟 COM 工具链（当前页跟踪 + 按页 ActionSettings 宏按钮 + 翻页/黑屏）并经 HTTP 暴露给主持人（带锁 + 回退）。前端重做另见 frontend 阶段（frontend-design 技能）。

**Architecture:** 扩展 `ppt_com.rs` 的 `mod com`（加 property-put / 字符串属性助手），新增 `SlideState`/`slide_state`/`trigger_action`/`nav_*`/`set_black`；`routes.rs` 加 `/api/slide_state`、`/api/action`，并把 `/api/next|prev|black|restore` 升级为"COM 优先、失败回退"。

**Tech Stack:** Rust、windows 0.58 IDispatch、Axum、Tauri v2。

> ⚠️ 构建：纯逻辑 `cargo test`（tauri-dev env）；FFI `cargo xwin build`（xwin env）。见 `~/.claude/CLAUDE.md`。
> COM IDispatch 细节以 cross-compile 为准修正。已知常量：`DISPATCH_PROPERTYPUT = DISPATCH_FLAGS(4)`，`DISPID_PROPERTYPUT = -3`。

---

## Task 1: COM 助手扩展 + SlideState 类型

**Files:** Modify `src-tauri/src/ppt_com.rs`

- [ ] **Step 1: 加序列化类型（顶层，跨平台）**
在 `parse_macro_names` 附近、`mod com` 之前加：
```rust
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct SlideAction {
    pub label: String,
    pub macro_name: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct SlideState {
    pub available: bool,
    pub in_show: bool,
    pub current: i32,
    pub total: i32,
    pub actions: Vec<SlideAction>,
}

impl SlideState {
    pub fn unavailable() -> Self {
        SlideState { available: false, in_show: false, current: 0, total: 0, actions: Vec::new() }
    }
}
```

- [ ] **Step 2: 在 `mod com` 里加助手**
在 `mod com` 内（`call_void` 之后）加：
```rust
    use windows::Win32::System::Com::DISPATCH_PROPERTYPUT;
    const DISPID_PROPERTYPUT: i32 = -3;

    /// 读字符串属性（无参 propertyget 返回 BSTR）。
    pub fn get_str(disp: &IDispatch, name: &str) -> Result<String, String> {
        call_str(disp, name, &[])
    }

    /// 写 i32 属性（PROPERTYPUT，如 View.State）。
    pub fn set_i32(disp: &IDispatch, name: &str, value: i32) -> Result<(), String> {
        unsafe {
            let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let names = [PCWSTR(wname.as_ptr())];
            let mut dispid: i32 = 0;
            disp.GetIDsOfNames(&IID_NULL, names.as_ptr(), 1, LOCALE_USER_DEFAULT, &mut dispid)
                .map_err(|e| format!("{name}: 找不到属性（{e}）"))?;
            let mut argv = [VARIANT::from(value)];
            let mut named = [DISPID_PROPERTYPUT];
            let params = DISPPARAMS {
                rgvarg: argv.as_mut_ptr(),
                cArgs: 1,
                rgdispidNamedArgs: named.as_mut_ptr(),
                cNamedArgs: 1,
            };
            disp.Invoke(dispid, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_PROPERTYPUT, &params, None, None, None)
                .map_err(|e| format!("{name}: 写属性失败（{e}）"))?;
            Ok(())
        }
    }

    /// 调方法返回 i32（如 View.CurrentShowPosition 也可当属性，用 get_i32；此为带参方法）。
    pub fn call_i32(disp: &IDispatch, name: &str, args: &[VARIANT]) -> Result<i32, String> {
        let v = invoke(disp, name, DISPATCH_METHOD | DISPATCH_PROPERTYGET, args)?;
        i32::try_from(&v).map_err(|e| format!("{name} 不是整数（{e}）"))
    }
```

- [ ] **Step 2.5: 暂时给 `invoke` 放宽可见性** — `set_i32` 用了 `invoke`/`call_str`，已在同模块内可见，无需改。确认 `VARIANT`、`PCWSTR`、`IID_NULL`、`LOCALE_USER_DEFAULT`、`DISPATCH_METHOD`、`DISPATCH_PROPERTYGET` 已在 `mod com` 的 use/const 中（spike 已引入）。

- [ ] **Step 3: 交叉编译**
Run: `cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
Expected: 0 errors（`serde` 已是依赖；`DISPATCH_PROPERTYPUT` 若导入路径报错，它在 `Win32_System_Com`，按提示修正）。

- [ ] **Step 4: Commit**
```bash
git add src-tauri/src/ppt_com.rs
git commit -m "feat(ppt_com): SlideState types + property-put/string COM helpers"
```

---

## Task 2: `slide_state()` — 当前页 + 按页宏按钮

**Files:** Modify `src-tauri/src/ppt_com.rs`

- [ ] **Step 1: 加 `slide_state`（windows 真实 + 非 windows 禁用）**
在 `list_macros` 附近加：
```rust
/// 读取当前页状态 + 该页的宏绑定按钮。连不上 PowerPoint 时返回 available=false（不报错）。
#[cfg(windows)]
pub fn slide_state() -> Result<SlideState, String> {
    let _co = com::ComGuard::init();
    let app = match com::connect() {
        Ok(a) => a,
        Err(_) => return Ok(SlideState::unavailable()),
    };
    let pres = com::get_obj(&app, "ActivePresentation").map_err(|_| "没有活动演示文稿".to_string())?;
    let slides = com::get_obj(&pres, "Slides")?;
    let total = com::get_i32(&slides, "Count").unwrap_or(0);

    // 当前页：优先放映视图，否则编辑视图。
    let shows = com::get_obj(&app, "SlideShowWindows")?;
    let show_count = com::get_i32(&shows, "Count").unwrap_or(0);
    let (in_show, slide) = if show_count >= 1 {
        let win = com::call_obj(&shows, "Item", &[windows::core::VARIANT::from(1i32)])?;
        let view = com::get_obj(&win, "View")?;
        let slide = com::get_obj(&view, "Slide")?;
        (true, slide)
    } else {
        let win = com::get_obj(&app, "ActiveWindow")?;
        let view = com::get_obj(&win, "View")?;
        let slide = com::get_obj(&view, "Slide")?;
        (false, slide)
    };
    let current = com::get_i32(&slide, "SlideIndex").unwrap_or(0);

    let actions = com::slide_actions(&slide);
    Ok(SlideState { available: true, in_show, current, total, actions })
}

#[cfg(not(windows))]
pub fn slide_state() -> Result<SlideState, String> {
    Ok(SlideState::unavailable())
}
```

- [ ] **Step 2: 在 `mod com` 加 `slide_actions`（枚举形状的 ActionSettings）**
```rust
    /// 枚举一页的形状，取所有 ppActionRunMacro 绑定 → {label, macro}。
    pub fn slide_actions(slide: &IDispatch) -> Vec<super::SlideAction> {
        let mut out = Vec::new();
        let shapes = match get_obj(slide, "Shapes") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let count = get_i32(&shapes, "Count").unwrap_or(0);
        for i in 1..=count {
            let shape = match call_obj(&shapes, "Item", &[VARIANT::from(i)]) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // ActionSettings(1=ppMouseClick).Action == 8=ppActionRunMacro
            let acts = match call_obj(&shape, "ActionSettings", &[VARIANT::from(1i32)]) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let action = get_i32(&acts, "Action").unwrap_or(0);
            if action == 8 {
                let macro_name = get_str(&acts, "Run").unwrap_or_default();
                if macro_name.is_empty() {
                    continue;
                }
                let label = shape_label(&shape);
                out.push(super::SlideAction { label, macro_name });
            }
        }
        out
    }

    /// 形状标签：可见文字优先，空则用形状名。
    fn shape_label(shape: &IDispatch) -> String {
        let text = get_obj(shape, "TextFrame")
            .and_then(|tf| get_obj(&tf, "TextRange"))
            .and_then(|tr| get_str(&tr, "Text"))
            .unwrap_or_default();
        let text = text.trim().to_string();
        if !text.is_empty() {
            return text;
        }
        get_str(shape, "Name").unwrap_or_default()
    }
```

- [ ] **Step 3: 交叉编译（0 errors）。** 命令同 Task 1 Step 3。

- [ ] **Step 4: Commit**
```bash
git add src-tauri/src/ppt_com.rs
git commit -m "feat(ppt_com): slide_state + per-slide ActionSettings macro enumeration"
```

---

## Task 3: trigger_action / 导航 / 黑屏

**Files:** Modify `src-tauri/src/ppt_com.rs`

- [ ] **Step 1: 加入口（windows + 非 windows 禁用），都加非 windows stub 返回 `Err("仅 Windows")`**
```rust
/// 触发当前页的某个宏按钮（安全：仅允许当前页已枚举的宏）。
#[cfg(windows)]
pub fn trigger_action(macro_name: &str) -> Result<(), String> {
    let state = slide_state()?;
    if !state.actions.iter().any(|a| a.macro_name == macro_name) {
        return Err(format!("宏不在当前页按钮中：{macro_name}"));
    }
    run_macro(macro_name)
}

/// 放映导航。
#[cfg(windows)]
fn show_view() -> Result<windows::Win32::System::Com::IDispatch, String> {
    let app = com::connect()?;
    let shows = com::get_obj(&app, "SlideShowWindows")?;
    if com::get_i32(&shows, "Count").unwrap_or(0) < 1 {
        return Err("当前没有放映".to_string());
    }
    let win = com::call_obj(&shows, "Item", &[windows::core::VARIANT::from(1i32)])?;
    com::get_obj(&win, "View")
}

#[cfg(windows)]
pub fn nav_next() -> Result<(), String> {
    let _co = com::ComGuard::init();
    com::call_void(&show_view()?, "Next", &[])
}
#[cfg(windows)]
pub fn nav_prev() -> Result<(), String> {
    let _co = com::ComGuard::init();
    com::call_void(&show_view()?, "Previous", &[])
}
#[cfg(windows)]
pub fn set_black(on: bool) -> Result<(), String> {
    let _co = com::ComGuard::init();
    // ppSlideShowBlackScreen=3, ppSlideShowRunning=1
    com::set_i32(&show_view()?, "State", if on { 3 } else { 1 })
}

#[cfg(not(windows))]
pub fn trigger_action(_macro_name: &str) -> Result<(), String> { Err("仅 Windows".into()) }
#[cfg(not(windows))]
pub fn nav_next() -> Result<(), String> { Err("仅 Windows".into()) }
#[cfg(not(windows))]
pub fn nav_prev() -> Result<(), String> { Err("仅 Windows".into()) }
#[cfg(not(windows))]
pub fn set_black(_on: bool) -> Result<(), String> { Err("仅 Windows".into()) }
```
（`com::ComGuard`/`connect`/`call_void` 已存在；`IDispatch` 在 `show_view` 返回类型用全路径。）

- [ ] **Step 2: 交叉编译（0 errors）。**
- [ ] **Step 3: Commit**
```bash
git add src-tauri/src/ppt_com.rs
git commit -m "feat(ppt_com): trigger_action + COM nav + black/restore"
```

---

## Task 4: 导播 Tauri 命令 get_slide_state

**Files:** Modify `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1: commands.rs 末尾加**
```rust
/// 导播 GUI：读当前页状态（COM）。
#[tauri::command]
pub fn get_slide_state() -> crate::ppt_com::SlideState {
    crate::ppt_com::slide_state().unwrap_or_else(|_| crate::ppt_com::SlideState::unavailable())
}
```
- [ ] **Step 2: lib.rs 在 `generate_handler![ ... ]` 里加 `commands::get_slide_state,`（在 vba 两个命令旁）。**
- [ ] **Step 3: 交叉编译（0 errors）。**
- [ ] **Step 4: Commit**
```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): get_slide_state for director GUI"
```

---

## Task 5: 主持人 HTTP API（slide_state / action + COM 升级翻页/黑屏）

**Files:** Modify `src-tauri/src/routes.rs`

- [ ] **Step 1: 路由表加两条**
在 `router()` 里加：
```rust
        .route("/api/slide_state", get(slide_state))
        .route("/api/action", post(action))
```
- [ ] **Step 2: 处理器**
```rust
async fn slide_state(State(state): State<Arc<AppState>>, Query(q): Query<TokenQuery>) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    let st = crate::ppt_com::slide_state().unwrap_or_else(|_| crate::ppt_com::SlideState::unavailable());
    Json(st).into_response()
}

#[derive(Deserialize)]
struct ActionReqBody {
    token: Option<String>,
    #[allow(dead_code)]
    client_id: Option<String>,
    r#macro: String,
}

async fn action(State(state): State<Arc<AppState>>, Json(req): Json<ActionReqBody>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::trigger_action(&req.r#macro) {
        Ok(()) => {
            log::event(&state, format!("触发宏：{}", req.r#macro));
            ok_json()
        }
        Err(e) => {
            log::event(&state, format!("触发宏失败：{e}"));
            json_err(StatusCode::INTERNAL_SERVER_ERROR, "action_failed")
        }
    }
}
```
- [ ] **Step 3: 升级 `page_turn`（COM 优先，回退 SendInput）**
把 `page_turn` 里 `let result = if forward { input::key_next() } else { input::key_prev() };` 替换为：
```rust
    // COM 翻页优先；失败回退 SendInput（先置前窗口）。
    let com_result = if forward { crate::ppt_com::nav_next() } else { crate::ppt_com::nav_prev() };
    let result = match com_result {
        Ok(()) => Ok(()),
        Err(_) => {
            if let Some(handle) = {
                let i = state.inner.lock().expect("state lock poisoned");
                i.selected_window.as_ref().map(|w| w.handle)
            } {
                crate::window_manager::bring_to_front(handle);
            }
            if forward { input::key_next() } else { input::key_prev() }
        }
    };
```
（保留现有 `bring_to_front` 那段位置——若已有，避免重复；以最终能编为准。）
- [ ] **Step 4: 升级 `black`/`restore`（COM）**
把 `black` 处理器里 `match obs::black()` 改为 `match crate::ppt_com::set_black(true)`；`restore` 改为 `crate::ppt_com::set_black(false)`；成功 `ok_json()`，失败 `json_err(StatusCode::NOT_IMPLEMENTED, "black_failed")`。
- [ ] **Step 5: 交叉编译（0 errors）。** 也跑一遍 Linux `cargo test`（非 windows 分支编过、既有测试绿）。
- [ ] **Step 6: Commit**
```bash
git add src-tauri/src/routes.rs
git commit -m "feat(routes): presenter slide_state/action; COM-first next/prev/black with fallback"
```

---

## Task 6: 全量交叉编译 + Linux 测试

- [ ] **Step 1:** `cargo xwin build`（0 errors）+ `cargo test`（tauri-dev，全绿）。
- [ ] **Step 2: Commit**（若有 lockfile 变化）
```bash
git add -A && git commit -m "build: verify COM toolchain cross-compiles + tests pass" || echo "nothing to commit"
```

---

## Self-Review
- Spec 覆盖：slide_state（T2）、按页 ActionSettings 枚举（T2 slide_actions）、trigger_action 安全校验（T3）、nav/black（T3）、导播命令（T4）、主持人 API + 回退（T5）、property-put 助手（T1）。前端在单独阶段（frontend-design）。
- 类型一致：`SlideState`/`SlideAction` 定义（T1）与 slide_state/命令/路由使用一致；`com::{get_str,set_i32,call_i32,slide_actions,shape_label}` 定义与调用一致；`trigger_action`/`nav_*`/`set_black` 两平台签名一致。
- COM 细节（DISPATCH_PROPERTYPUT 路径、DISPID_PROPERTYPUT、ActionSettings 索引）标注 cross-compile 修正。
