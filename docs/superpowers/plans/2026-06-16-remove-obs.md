# 删除 OBS 集成 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 删掉占位 OBS 集成与导播本地黑屏/恢复按钮;主持人端 COM 黑屏不动。

**Architecture:** 纯删除/重排:删 `obs.rs`、注销两个命令、摘 `config.rs` 的 OBS 字段、去前端两个按钮/方法。详见 `docs/superpowers/specs/2026-06-16-remove-obs-design.md`。

**Tech Stack:** Rust(Tauri)、Vue 3。

**验证命令:**
- 原生测试: `P=/data/sunyunbo/miniconda3/envs/tauri-dev; PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml`
- 交叉编译: `cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
- 前端构建: `cd /data/sunyunbo/www/StageDeckRemote && pnpm build`

---

### Task 1: Rust 侧删除

**Files:** `src-tauri/src/obs.rs`(删)、`src-tauri/src/lib.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/config.rs`

- [ ] **Step 1: 删 obs.rs + lib.rs 去 `mod obs;` + 注销命令**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git rm src-tauri/src/obs.rs
```

在 `src-tauri/src/lib.rs` 删掉这一行:

```rust
mod obs;
```

在 `lib.rs` 的 `tauri::generate_handler![ ... ]` 列表里删掉这两行:

```rust
            commands::local_black,
            commands::local_restore,
```

- [ ] **Step 2: `commands.rs` 去 `obs` 引用 + 删两个命令**

把 use 行:

```rust
use crate::{input, log, network, obs, window_manager};
```

改为:

```rust
use crate::{input, log, network, window_manager};
```

删掉这两个命令(连同各自的 `#[tauri::command]`):

```rust
#[tauri::command]
pub fn local_black() -> Result<(), String> {
    obs::black().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn local_restore() -> Result<(), String> {
    obs::restore().map_err(|e| e.to_string())
}
```

- [ ] **Step 3: `config.rs` 摘 OBS 字段**

把 `Config` 结构体里的:

```rust
    pub server_port: u16,
    pub last_selected_window_title: String,
    pub obs_enabled: bool,
    pub obs_url: String,
    pub obs_password: String,
    pub ppt_scene_name: String,
    pub black_scene_name: String,
```

改为:

```rust
    pub server_port: u16,
    pub last_selected_window_title: String,
```

把 `Default` 实现里的:

```rust
        Self {
            server_port: 8787,
            last_selected_window_title: String::new(),
            obs_enabled: false,
            obs_url: "ws://127.0.0.1:4455".into(),
            obs_password: String::new(),
            ppt_scene_name: "PPT".into(),
            black_scene_name: "Black".into(),
        }
```

改为:

```rust
        Self {
            server_port: 8787,
            last_selected_window_title: String::new(),
        }
```

- [ ] **Step 4: 交叉编译 + 原生测试**

Run 交叉编译(0 error,确认无悬空 `obs`/命令引用)+ 原生测试(23 passed)。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/lib.rs src-tauri/src/commands.rs src-tauri/src/config.rs
git commit -m "refactor: remove OBS integration (obs.rs, local_black/restore commands, config fields)"
```

---

### Task 2: 前端删除

**Files:** `src/components/LocalControlPanel.vue`、`src/utils/api.ts`

- [ ] **Step 1: `LocalControlPanel.vue` 去黑屏/恢复**

把脚本里:

```ts
// 本地翻页/黑屏直接走 IPC（不受远程开关影响，导播随时可操作）。
const next = () => run(() => api.localNext());
const prev = () => run(() => api.localPrev());
const black = () => run(() => api.localBlack());
const restore = () => run(() => api.localRestore());
```

改为:

```ts
// 本地翻页直接走 IPC（不受远程开关影响，导播随时可操作）。
const next = () => run(() => api.localNext());
const prev = () => run(() => api.localPrev());
```

把 `run` 上方注释:

```ts
// 统一捕获 IPC 失败并给出反馈（非 Windows 下 input/obs 会直接报错）。
```

改为:

```ts
// 统一捕获 IPC 失败并给出反馈（非 Windows 下 input 会直接报错）。
```

把模板里的本地控制网格:

```vue
    <div class="grid">
      <button @click="prev">上一页</button>
      <button @click="next">下一页</button>
      <button @click="black">黑屏</button>
      <button @click="restore">恢复</button>
    </div>
```

改为:

```vue
    <div class="grid">
      <button @click="prev">上一页</button>
      <button @click="next">下一页</button>
    </div>
```

- [ ] **Step 2: `api.ts` 删两个方法**

删掉:

```ts
  localBlack: () => invoke<void>("local_black"),
  localRestore: () => invoke<void>("local_restore"),
```

- [ ] **Step 3: 前端构建**

Run `pnpm build`。Expected:成功(无悬空 `localBlack`/`localRestore`/`black`/`restore`)。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/LocalControlPanel.vue src/utils/api.ts
git commit -m "refactor(director): drop local 黑屏/恢复 buttons (OBS removed)"
```

---

## Self-Review
- **Spec 覆盖:** §2 全部条目 → Task 1(Rust)+ Task 2(前端)。
- **完整性:** 删除后无悬空引用 —— `obs`(commands use + lib mod)、`local_black/restore`(命令定义 + 注册 + api + 组件)、OBS config 字段。交叉编译 + pnpm build 兜底任何遗漏。
- **不动:** 主持人 `/api/black|restore`→`ppt_com::set_black`、`local_next/prev`、锁、COM、捕获。
- **Placeholder:** 无;均为确切 old→new 或删除块。
