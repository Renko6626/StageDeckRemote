# 导播预览改走 IPC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 导播预览改用 Tauri IPC 取帧(data URL),消除 webview 跨源拉环回 HTTP 的失败 → 预览正常显示。

**Architecture:** 新 IPC 命令 `get_snapshot()` 进程内复用 `capture::snapshot_jpeg` 返回 base64 JPEG;`DirectorPreview.vue` 自调度链式 `getSnapshot` + `data:` 显示,移除原 HTTP/token 取帧链。详见 `docs/superpowers/specs/2026-06-16-director-preview-ipc-design.md`。

**Tech Stack:** Rust(Tauri IPC、base64)、Vue 3。

**验证命令:**
- 原生测试: `P=/data/sunyunbo/miniconda3/envs/tauri-dev; PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml`
- 交叉编译: `cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
- 前端构建: `cd /data/sunyunbo/www/StageDeckRemote && pnpm build`

## 文件结构

| 文件 | 改动 |
|---|---|
| `src-tauri/Cargo.toml` | 加 `base64` 依赖 |
| `src-tauri/src/commands.rs` | `get_snapshot()` |
| `src-tauri/src/lib.rs` | 注册 `get_snapshot` |
| `src/utils/api.ts` | `getSnapshot()` |
| `src/components/DirectorPreview.vue` | 取帧改 IPC + data URL |

---

### Task 1: 后端 `get_snapshot` 命令

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: `Cargo.toml` 加 base64**

在 `[dependencies]` 区(`serde_json = "1"` 那一带)加一行:

```toml
base64 = "0.22"
```

- [ ] **Step 2: `commands.rs` 加命令**

把 `commands.rs` 顶部的 `use tauri::{Manager, State};` 之后加一行:

```rust
use base64::Engine;
```

在文件末尾加命令:

```rust
/// 导播 GUI:进程内取一帧捕获,返回 base64 JPEG(避免 webview 跨源拉环回 HTTP)。
#[tauri::command]
pub fn get_snapshot(state: State<Arc<AppState>>) -> Result<String, String> {
    let bytes = crate::capture::snapshot_jpeg(&state, 1280, 720, 70).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}
```

- [ ] **Step 3: `lib.rs` 注册命令**

在 `tauri::generate_handler![ ... ]` 里 `commands::send_to_presenter,` 之后加:

```rust
            commands::get_snapshot,
```

- [ ] **Step 4: 交叉编译 + 原生测试**

Run 交叉编译(0 error)+ 原生测试(23 passed)。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): get_snapshot — in-process base64 JPEG for the director preview"
```

---

### Task 2: 导播预览改 IPC 取帧

**Files:**
- Modify: `src/utils/api.ts`
- Modify(整文件覆盖): `src/components/DirectorPreview.vue`

- [ ] **Step 1: `api.ts` 加方法**

在 `export const api = { ... }` 里 `sendToPresenter: ...` 之后加:

```ts
  getSnapshot: () => invoke<string>("get_snapshot"),
```

- [ ] **Step 2: 用以下完整内容覆盖 `src/components/DirectorPreview.vue`**

```vue
<script setup lang="ts">
// 实时捕获预览(只读监看)+ COM 状态头条带。
// 帧走 Tauri IPC(get_snapshot,进程内 → 避免 webview 跨源拉环回 HTTP);头条带轮询 get_slide_state。
import { ref, onMounted, onUnmounted } from "vue";

import { api, type SlideState } from "../utils/api";

const slide = ref<SlideState | null>(null);
const frameSrc = ref("");
const frameOk = ref(false);
let stateTimer: number | undefined;
let frameTimer: number | undefined;
let alive = true;

async function pollState() {
  try {
    slide.value = await api.getSlideState();
  } catch {
    slide.value = null;
  }
}

// 帧链:取一帧 → 显示 → 排下一帧(自调度,慢捕获不堆积)。
async function loadFrame() {
  if (!alive) return;
  try {
    const b64 = await api.getSnapshot();
    frameSrc.value = "data:image/jpeg;base64," + b64;
    frameOk.value = true;
  } catch {
    frameOk.value = false;
  }
  if (alive) frameTimer = window.setTimeout(loadFrame, 400);
}

onMounted(() => {
  void pollState();
  stateTimer = window.setInterval(pollState, 1500);
  void loadFrame();
});
onUnmounted(() => {
  alive = false;
  if (stateTimer) window.clearInterval(stateTimer);
  if (frameTimer) window.clearTimeout(frameTimer);
});
</script>

<template>
  <section class="preview">
    <header class="pv-head">
      <span class="stat">
        <span class="dot" :class="{ on: slide?.available }"></span>
        POWERPOINT <b>{{ slide?.available ? "已连接" : "未连接" }}</b>
      </span>
      <span class="stat">
        当前页 <b>{{ slide?.available ? slide.current + "/" + slide.total : "—" }}</b>
      </span>
      <span class="show-flag" :class="{ live: slide?.in_show }">
        {{ slide?.in_show ? "LIVE" : "IDLE" }}
      </span>
    </header>
    <div class="stage">
      <img v-show="frameOk" :src="frameSrc" alt="PPT 捕获预览" draggable="false" />
      <div class="placeholder" v-if="!frameOk">等待捕获 · 未选择 PPT 窗口</div>
    </div>
  </section>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.pv-head {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 9px 14px;
  border: 1px solid var(--line);
  border-bottom: none;
  background: var(--panel);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--ink-dim);
}
.pv-head .stat {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}
.pv-head b {
  color: var(--ink);
  font-weight: 600;
}
.pv-head .dot {
  width: 8px;
  height: 8px;
  background: var(--danger);
  box-shadow: 0 0 0 1px #000;
}
.pv-head .dot.on {
  background: var(--ink);
}
.pv-head .show-flag {
  margin-left: auto;
  font-size: 10px;
  letter-spacing: 0.18em;
  border: 1px solid var(--line-hi);
  padding: 2px 8px;
  color: var(--ink-faint);
}
.pv-head .show-flag.live {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}
.stage {
  flex: 1 1 auto;
  min-height: 0;
  position: relative;
  background: #000;
  border: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.stage img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  display: block;
}
.placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #000;
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.1em;
  color: var(--ink-faint);
  pointer-events: none;
}
</style>
```

- [ ] **Step 3: 前端构建**

Run `pnpm build`。Expected:`vue-tsc` 通过(不再引用 `networkState`/`storeToRefs`,无悬空)、`vite build` 成功。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/utils/api.ts src/components/DirectorPreview.vue
git commit -m "fix(director): preview frames via IPC data URL (no cross-origin loopback)"
```

---

## Self-Review

- **Spec 覆盖:** §2 命令 `get_snapshot` → Task 1;§2 前端 IPC 取帧 + 移除 HTTP/token 链 → Task 2;头条带 `get_slide_state` 不变(Task 2 保留)。
- **类型/契约一致:** `get_snapshot() -> Result<String,String>`(base64)↔ `api.getSnapshot(): Promise<string>` ↔ `frameSrc = "data:image/jpeg;base64,"+b64`。`snapshot_jpeg(&AppState,1280,720,70)->Result<Vec<u8>,CaptureError>` 复用,`map_err(|e| e.to_string())`。
- **Placeholder:** 无;DirectorPreview 给整文件,后端给确切插入块。
- **编译绿:** Task 1 含 base64 依赖 + 命令 + 注册(交叉编译 + cargo test);Task 2 前端 pnpm build。`DirectorPreview` 不再 import `networkState`/`storeToRefs`,vue-tsc 兜底悬空。
- **范围:** 不动主持人 HTTP 取帧、捕获实现、锁、COM;坐标点击优雅化留待后续。
