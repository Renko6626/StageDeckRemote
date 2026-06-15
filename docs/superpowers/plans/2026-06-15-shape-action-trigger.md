# 形状级动作触发 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让主持人能触发「共享处理器 + 形状参数」型 VBA 按钮(如 `ToggleOption(shp As Shape)`),修复 `quiz_template.pptm` 第三页选项按钮无法触发的问题。

**Architecture:** 动作标识从「宏名」改为「形状名」。`slide_state` 给每个动作带 `id = Shape.Name`;触发时后端按名在当前页定位形状、服务端自读其绑定宏、`Application.Run(宏, 形状)` 复刻点击,无参宏走「带形状失败→无参重试」回退。详见 `docs/superpowers/specs/2026-06-15-shape-action-trigger-design.md`。

**Tech Stack:** Rust + windows-rs 0.58(IDispatch/VARIANT/COM),Axum HTTP,vanilla JS 前端,Vue/TS 类型。

**验证约定(每个含 Rust 改动的任务都跑):**
- **交叉编译(权威,验证 FFI 正确性):**
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 \
    PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
    cargo xwin build --target x86_64-pc-windows-msvc
  ```
  期望:`Finished`，0 error。
- **原生测试(纯逻辑回归):**
  ```bash
  P=/data/sunyunbo/miniconda3/envs/tauri-dev; \
  PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH \
    cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml
  ```
  期望:既有 17 测试全绿。

> COM/FFI 代码在 Linux 上不可单测;正确性以交叉编译 0 错误 + 用户 Windows 运行时为准。纯逻辑(`parse_macro_names`)的既有单测保持绿即回归通过。

## 文件结构

| 文件 | 职责 | 本计划改动 |
|---|---|---|
| `src-tauri/src/ppt_com.rs` | COM 控制核心 | `SlideAction.id`;`com::current_slide`/`com::dispatch_variant`/`com::run_macro_with_shape`;`trigger_action` 改形状触发 |
| `src-tauri/src/routes.rs` | 主持人 HTTP API | `ActionReqBody.shape`;`action` 处理器与日志 |
| `src/presenter/presenter.js` | 主持人前端 | 按钮 `data-id`、动作请求体 `{shape}`、`lastActionsKey` |
| `src/utils/api.ts` | 前端类型 | `SlideAction.id` |

---

### Task 1: `SlideAction.id` + 在枚举时填充形状名

**Files:**
- Modify: `src-tauri/src/ppt_com.rs`(struct `SlideAction` 约 5-9 行;`com::slide_actions` 约 203-210 行)

- [ ] **Step 1: 给 `SlideAction` 增加 `id` 字段**

把 struct 改成:

```rust
#[derive(Serialize, Clone, Debug)]
pub struct SlideAction {
    pub id: String,         // 形状名(Shape.Name),触发标识
    pub label: String,      // 显示:可见文字优先,空则形状名
    pub macro_name: String, // 绑定的宏名(显示/日志;触发时服务端重新读取)
}
```

- [ ] **Step 2: 在 `com::slide_actions` 的 `action == 8` 分支里读形状名并填充**

把该分支改成(读 `Name`,空名跳过):

```rust
if action == 8 {
    let macro_name = get_str(&acts, "Run").unwrap_or_default();
    if macro_name.is_empty() {
        continue;
    }
    let id = get_str(&shape, "Name").unwrap_or_default();
    if id.is_empty() {
        continue;
    }
    let label = shape_label(&shape);
    out.push(super::SlideAction { id, label, macro_name });
}
```

- [ ] **Step 3: 交叉编译 + 原生测试**(见顶部命令)。期望:交叉编译 0 错误;测试 17/17。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/ppt_com.rs
git commit -m "feat(ppt_com): add SlideAction.id (shape name) for shape-level triggering"
```

---

### Task 2: 形状级 `trigger_action` + 传形状调用(核心)

**Files:**
- Modify: `src-tauri/src/ppt_com.rs`(`mod com` 内新增 3 个函数;`slide_state` 复用 `current_slide`;重写 `trigger_action` 的 windows / non-windows 两版)

- [ ] **Step 1: 在 `mod com` 内新增「取当前页」助手**

在 `mod com` 里(例如 `slide_actions` 之前)加入:

```rust
/// 取当前页:放映视图优先,否则编辑视图。返回 (是否放映中, Slide)。
pub fn current_slide(app: &IDispatch) -> Result<(bool, IDispatch), String> {
    let shows = get_obj(app, "SlideShowWindows")?;
    if get_i32(&shows, "Count").unwrap_or(0) >= 1 {
        let win = call_obj(&shows, "Item", &[VARIANT::from(1i32)])?;
        let view = get_obj(&win, "View")?;
        Ok((true, get_obj(&view, "Slide")?))
    } else {
        let win = get_obj(app, "ActiveWindow")?;
        let view = get_obj(&win, "View")?;
        Ok((false, get_obj(&view, "Slide")?))
    }
}
```

- [ ] **Step 2: 在 `mod com` 内新增「把 IDispatch 包成 VT_DISPATCH VARIANT」与「带形状运行」**

在 `mod com` 顶部 `use` 区补一行:

```rust
use windows::Win32::System::Variant::VT_DISPATCH;
```

然后加入:

```rust
/// 把一个 IDispatch 包成 VT_DISPATCH 的 VARIANT(owned;由 VARIANT 的 Drop 负责 Release)。
fn dispatch_variant(disp: &IDispatch) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut v.Anonymous.Anonymous; // VARIANT_0_0
        inner.vt = VT_DISPATCH;
        // clone() 会 AddRef;VariantClear(Drop) 会按 vt=VT_DISPATCH 做 Release,引用计数平衡。
        inner.Anonymous.pdispVal = std::mem::ManuallyDrop::new(Some(disp.clone()));
    }
    v
}

/// Application.Run(macro, shape):先带形状(复刻对 `Sub Foo(shp As Shape)` 的点击),
/// 失败退回无参(CheckAnswer/BtnReset/Click_* 等无参宏)。带形状的失败发生在「参数个数
/// 校验」阶段、宏体不执行,故无双跑风险(详见 spec §2.2)。
pub fn run_macro_with_shape(app: &IDispatch, macro_name: &str, shape: &IDispatch) -> Result<(), String> {
    let with_shape = call_void(app, "Run", &[VARIANT::from(macro_name), dispatch_variant(shape)]);
    if with_shape.is_ok() {
        return Ok(());
    }
    call_void(app, "Run", &[VARIANT::from(macro_name)])
}
```

> **FFI 校验点:** 若 `inner.Anonymous.pdispVal` / `inner.vt` 字段路径与本机 windows 0.58 的 `VARIANT` 定义不符导致编译失败,以交叉编译报错为准微调字段路径(windows-rs 的 `VARIANT_0_0_0` 联合体里 `IDispatch` 字段名为 `pdispVal`,类型 `ManuallyDrop<Option<IDispatch>>`)。备选:若该版本已实现 `From<IDispatch> for VARIANT`,可直接 `VARIANT::from(shape.clone())` 取代 `dispatch_variant`。最终以「交叉编译 0 错误」为准。

- [ ] **Step 3: `slide_state` 复用 `current_slide`**

把 `slide_state` 中「当前页:优先放映视图…」那段(取 `shows`/`show_count`/分支求 `slide`)替换为:

```rust
    let (in_show, slide) = com::current_slide(&app)?;
    let current = com::get_i32(&slide, "SlideIndex").unwrap_or(0);
```

保留前面的 `pres`/`slides`/`total`,以及末尾 `let actions = com::slide_actions(&slide); Ok(SlideState { available: true, in_show, current, total, actions })`。

- [ ] **Step 4: 重写 `trigger_action`(windows 版)为形状触发**

把现有 windows 版 `trigger_action` 整体替换为:

```rust
/// 触发当前页某个形状的鼠标动作宏(安全:仅当前页、仅 ppActionRunMacro;宏名服务端现读)。
/// 返回实际触发的宏名,供日志。
#[cfg(windows)]
pub fn trigger_action(shape_id: &str) -> Result<String, String> {
    let _co = com::ComGuard::init();
    let app = com::connect()?;
    let (_in_show, slide) = com::current_slide(&app)?;
    let shapes = com::get_obj(&slide, "Shapes")?;
    // 按名定位(对增删导致的索引漂移免疫);定位不到 → 拒绝(不在当前页)。
    let shape = com::call_obj(&shapes, "Item", &[windows::core::VARIANT::from(shape_id)])
        .map_err(|_| format!("形状不在当前页:{shape_id}"))?;
    // ActionSettings(1=ppMouseClick).Action 必须为 8=ppActionRunMacro。
    let acts = com::call_obj(&shape, "ActionSettings", &[windows::core::VARIANT::from(1i32)])
        .map_err(|_| format!("形状无鼠标动作:{shape_id}"))?;
    if com::get_i32(&acts, "Action").unwrap_or(0) != 8 {
        return Err(format!("形状不是运行宏动作:{shape_id}"));
    }
    let macro_name = com::get_str(&acts, "Run").unwrap_or_default();
    if macro_name.is_empty() {
        return Err(format!("形状未绑定宏:{shape_id}"));
    }
    com::run_macro_with_shape(&app, &macro_name, &shape)?;
    Ok(macro_name)
}
```

- [ ] **Step 5: 同步 non-windows 版签名**

把 non-windows 版改为返回 `Result<String, String>`:

```rust
#[cfg(not(windows))]
pub fn trigger_action(_shape_id: &str) -> Result<String, String> { Err("仅 Windows".into()) }
```

- [ ] **Step 6: 交叉编译 + 原生测试**(见顶部命令)。期望:交叉编译 0 错误;测试 17/17。

- [ ] **Step 7: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/ppt_com.rs
git commit -m "feat(ppt_com): trigger by shape; pass clicked Shape to Application.Run with no-arg fallback"
```

---

### Task 3: 路由请求体 `{shape}` + 处理器

**Files:**
- Modify: `src-tauri/src/routes.rs`(`ActionReqBody` 约 311-317 行;`action` 处理器约 319-336 行)

- [ ] **Step 1: 改请求体字段**

```rust
#[derive(Deserialize)]
struct ActionReqBody {
    token: Option<String>,
    #[allow(dead_code)]
    client_id: Option<String>,
    shape: String,
}
```

- [ ] **Step 2: 改 `action` 处理器(门控不变,改调形状触发,日志含形状名+宏名)**

```rust
async fn action(State(state): State<Arc<AppState>>, Json(req): Json<ActionReqBody>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::trigger_action(&req.shape) {
        Ok(macro_name) => {
            log::event(&state, format!("触发 {} → {}", req.shape, macro_name));
            ok_json()
        }
        Err(e) => {
            log::event(&state, format!("触发失败（{}）:{e}", req.shape));
            json_err(StatusCode::INTERNAL_SERVER_ERROR, "action_failed")
        }
    }
}
```

- [ ] **Step 3: 交叉编译 + 原生测试**(见顶部命令)。期望:交叉编译 0 错误;测试 17/17。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/routes.rs
git commit -m "feat(routes): /api/action takes shape id; log shape→macro"
```

---

### Task 4: 前端按钮传形状 + 类型

**Files:**
- Modify: `src/presenter/presenter.js`(`renderActions` 约 112-126 行;动作点击处理 约 214-219 行)
- Modify: `src/utils/api.ts`(`SlideAction` 接口)

- [ ] **Step 1: `renderActions` 给按钮加 `data-id`,并把 `id` 纳入 `lastActionsKey`**

把 `var key = ...` 一行改为(纳入 `a.id`):

```js
    var key = actions.map(function (a) { return a.id + "|" + a.label + "|" + a.macro_name; }).join("¦");
```

在 `forEach` 创建按钮处补一行 `data-id`:

```js
      actions.forEach(function (a) {
        var b = document.createElement("button");
        b.className = "act";
        b.setAttribute("data-macro", a.macro_name);
        b.setAttribute("data-id", a.id);
        b.textContent = a.label || a.macro_name;
        el.actions.appendChild(b);
      });
```

- [ ] **Step 2: 动作点击改发 `{shape}`**

```js
  el.actions.addEventListener("click", function (ev) {
    var b = ev.target.closest(".act");
    if (!b || b.disabled) return;
    var shape = b.getAttribute("data-id");
    api("/api/action", { shape: shape }).then(poll).catch(function () {});
  });
```

- [ ] **Step 3: `api.ts` 给 `SlideAction` 加 `id`**

在 `SlideAction` 接口里 `label` 之前加:

```ts
  id: string;
```

- [ ] **Step 4: 前端构建验证**

```bash
cd /data/sunyunbo/www/StageDeckRemote && npm run build
```

期望:vue-tsc 通过、vite 构建成功(无类型错误)。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.js src/utils/api.ts
git commit -m "feat(presenter): send shape id to /api/action; SlideAction.id type"
```

---

## Self-Review(写完即查)

- **Spec 覆盖:** §3.1 模型 id → Task 1;§2.2/§2.3/§3.2 触发 → Task 2;§3.3 路由 → Task 3;§3.4/§3.5 前端+类型 → Task 4。全覆盖。
- **类型一致:** `trigger_action` 两版均返回 `Result<String, String>`;`action` 处理器按 `Ok(macro_name)` 解构。`SlideAction.id` 在 Rust(序列化)、`api.ts`、`presenter.js`(`a.id`/`data-id`/请求体 `shape`)一致。
- **Placeholder:** 无 TBD;FFI 字段路径给了权威校验方式(交叉编译)与备选方案。
- **回归:** 无参宏(Reset/Check/连线题按钮)经「无参回退」继续可用;`slide_state` 复用 `current_slide` 行为等价。
