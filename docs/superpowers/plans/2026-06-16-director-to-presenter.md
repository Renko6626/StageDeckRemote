# 导播→主持人通知 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 导播从共享预制池点一条发给主持人,主持人手机顶部横幅显示(~10 秒自消 + 可手动关)。

**Architecture:** `Inner` 加 `director_message{seq,text,at_ms}`;IPC `send_to_presenter(id)` 校验 id∈presets、置带自增 seq 的消息;折进 `/api/status`(主持人本就轮询);主持人页加固定横幅,在 status 轮询里按 seq 检测新消息(首帧只记基线)。详见 `docs/superpowers/specs/2026-06-16-director-to-presenter-design.md`。

**Tech Stack:** Rust(Tauri IPC、Axum)、Vue 3、原生 JS 主持人页。

**验证命令:**
- 原生测试(tauri-dev 环境):
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
| `src-tauri/src/app_state.rs` | `DirectorMessage` 类型;`Inner.director_message` |
| `src-tauri/src/routes.rs` | `/api/status` JSON 加 `director_message` |
| `src-tauri/src/commands.rs` | `send_to_presenter(id)` |
| `src-tauri/src/lib.rs` | 注册 `send_to_presenter` |
| `src/utils/api.ts` | `sendToPresenter(id)` |
| `src/components/DirectorMessages.vue` | 预制项可点发送 + 「已发送」反馈 |
| `src/presenter/presenter.{html,css,js}` | 顶部横幅 + status 轮询处理 |

---

### Task 1: 后端模型 + status 字段

**Files:**
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/routes.rs`

- [ ] **Step 1: `app_state.rs` 加类型与字段**

在 `HostMessage` 定义之后加:

```rust
/// 导播→主持人的即时通知(临时,不持久化)。seq 单调自增,供主持人检测新消息。
#[derive(Clone, Debug, Serialize)]
pub struct DirectorMessage {
    pub seq: u64,
    pub text: String,
    pub at_ms: u64,
}
```

在 `Inner` 里 `pub presets: Vec<PresetMessage>,` 之后加:

```rust
    /// 导播→主持人的最新通知(临时;主持人轮询 status 取走显示)。
    pub director_message: Option<DirectorMessage>,
```

在 `AppState::new` 的 `Inner { ... }` 初始化里 `presets: crate::presets::default_presets(),` 之后加:

```rust
                director_message: None,
```

- [ ] **Step 2: `routes.rs` 的 `/api/status` 加字段**

把 `status` 处理器里的 json!(第 121-129 行)中:

```rust
            "active_client_id": i.active_client_id,
            "server_time": now_secs(),
```

替换为:

```rust
            "active_client_id": i.active_client_id,
            "director_message": i.director_message.clone(),
            "server_time": now_secs(),
```

- [ ] **Step 3: 交叉编译 + 原生测试**

Run 交叉编译(0 error)+ 原生测试(23 passed)。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/app_state.rs src-tauri/src/routes.rs
git commit -m "feat(messages): DirectorMessage model + /api/status carries director_message"
```

---

### Task 2: `send_to_presenter` 命令 + 注册

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: `commands.rs` 加命令(文件末尾)**

```rust
/// 导播 GUI:从共享预制池里选一条(按 id)发给主持人(主持人页内横幅显示)。
#[tauri::command]
pub fn send_to_presenter(state: State<Arc<AppState>>, id: String) {
    let sent = {
        let Ok(mut i) = state.inner.lock() else {
            return;
        };
        let Some(text) = i.presets.iter().find(|p| p.id == id).map(|p| p.text.clone()) else {
            return; // id 不在池中:不改动(前端按池内 id 调用,不会非法)
        };
        let seq = i.director_message.as_ref().map(|m| m.seq).unwrap_or(0) + 1;
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        i.director_message = Some(crate::app_state::DirectorMessage {
            seq,
            text: text.clone(),
            at_ms,
        });
        text
    };
    log::event(&state, format!("已通知主持人：{sent}"));
}
```

- [ ] **Step 2: `lib.rs` 注册命令**

在 `tauri::generate_handler![ ... ]` 里 `commands::set_presets,` 之后加:

```rust
            commands::send_to_presenter,
```

- [ ] **Step 3: 交叉编译 + 原生测试**

Run 交叉编译(0 error)+ 原生测试(23 passed)。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): send_to_presenter — push a preset to the presenter banner"
```

---

### Task 3: 导播端发送(api + 卡片)

**Files:**
- Modify: `src/utils/api.ts`
- Modify(整文件覆盖): `src/components/DirectorMessages.vue`

- [ ] **Step 1: `api.ts` 加方法**

在 `export const api = { ... }` 里 `setPresets: ...` 之后加:

```ts
  sendToPresenter: (id: string) => invoke<void>("send_to_presenter", { id }),
```

- [ ] **Step 2: 用以下完整内容覆盖 `src/components/DirectorMessages.vue`**

```vue
<script setup lang="ts">
// 快捷消息预制编辑(导播)+ 点一条发给主持人 + 提示音开关。
import { ref, onMounted } from "vue";

import { api, type PresetMessage } from "../utils/api";

const presets = ref<PresetMessage[]>([]);
const editing = ref(false);
const drafts = ref<string[]>([]);
const chimeOn = ref(localStorage.getItem("sd.dir.toastChime") !== "off");
const error = ref<string | null>(null);
const sentFlash = ref("");
let flashTimer: number | undefined;

async function load() {
  try {
    presets.value = await api.getPresets();
    error.value = null;
  } catch (e) {
    error.value = String(e);
  }
}
async function sendToPresenter(p: PresetMessage) {
  try {
    await api.sendToPresenter(p.id);
    sentFlash.value = "已发送给主持人 ✓";
  } catch (e) {
    sentFlash.value = "发送失败";
    error.value = String(e);
  }
  if (flashTimer) window.clearTimeout(flashTimer);
  flashTimer = window.setTimeout(() => (sentFlash.value = ""), 1600);
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
      <div class="hint">点一条发给主持人</div>
      <ul class="list">
        <li v-for="p in presets" :key="p.id">
          <button class="send" @click="sendToPresenter(p)">{{ p.text }}</button>
        </li>
        <li v-if="!presets.length" class="empty">无预制消息</li>
      </ul>
      <p v-if="sentFlash" class="sent">{{ sentFlash }}</p>
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
.hint {
  font-family: var(--mono);
  font-size: 9px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--ink-faint);
  margin-bottom: 8px;
}
.list {
  list-style: none;
  margin: 0 0 8px;
  padding: 0;
  display: grid;
  gap: 5px;
}
.list li.empty {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-faint);
  text-transform: uppercase;
  letter-spacing: 0.12em;
  padding: 4px 0;
}
.send {
  width: 100%;
  text-align: left;
  font-family: var(--sans);
  font-size: 13px;
  font-weight: 600;
  padding: 9px 11px;
}
.sent {
  margin: 0 0 8px;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.08em;
  color: var(--ink);
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

- [ ] **Step 3: 前端构建**

Run `pnpm build`。Expected:成功。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/utils/api.ts src/components/DirectorMessages.vue
git commit -m "feat(director): click a preset to send it to the presenter"
```

---

### Task 4: 主持人端横幅

**Files:**
- Modify: `src/presenter/presenter.html`
- Modify: `src/presenter/presenter.css`
- Modify: `src/presenter/presenter.js`

- [ ] **Step 1: `presenter.html` 加横幅元素**

在 `<!-- ─── transport dock ... -->` 的 `</div>`(dock 结束,第 78 行)之后、`.frame` 的 `</div>`(第 79 行)之前,插入:

```html
      <!-- ─── director → presenter banner (fixed overlay, any page) ─── -->
      <div class="dir-banner" id="dir-banner" hidden>
        <span class="db-head">● 导播 · <b id="dir-banner-time"></b></span>
        <span class="db-text" id="dir-banner-text"></span>
        <button class="db-x" id="dir-banner-x" aria-label="关闭">✕</button>
      </div>
```

- [ ] **Step 2: `presenter.css` 加横幅样式(文件末尾追加)**

```css
/* ─── director → presenter banner ─── */
.dir-banner {
  position: fixed;
  top: 8px;
  left: 50%;
  transform: translateX(-50%);
  width: calc(100% - 16px);
  max-width: 960px;
  z-index: 50;
  display: flex;
  align-items: center;
  gap: 10px;
  background: var(--ink);
  color: #000;
  border: 1px solid var(--ink);
  padding: 10px 12px;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.5);
}
.dir-banner[hidden] {
  display: none;
}
.db-head {
  flex: 0 0 auto;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  opacity: 0.65;
}
.db-text {
  flex: 1 1 auto;
  font-family: var(--sans);
  font-size: 16px;
  font-weight: 700;
  line-height: 1.2;
}
.db-x {
  flex: 0 0 auto;
  appearance: none;
  background: transparent;
  border: none;
  color: #000;
  font-size: 15px;
  cursor: pointer;
  padding: 2px 6px;
}
```

- [ ] **Step 3: `presenter.js` —— 加 el 引用**

在 `el` 对象里 `msgSent: document.getElementById("msg-sent"),` 之后加:

```js
    dirBanner: document.getElementById("dir-banner"),
    dirBannerText: document.getElementById("dir-banner-text"),
    dirBannerTime: document.getElementById("dir-banner-time"),
    dirBannerX: document.getElementById("dir-banner-x"),
```

- [ ] **Step 4: `presenter.js` —— 轮询里处理 director_message**

把 poll 的 `.then` 里:

```js
        state.com = data[1] || state.com;
        render();
      })
```

替换为:

```js
        state.com = data[1] || state.com;
        render();
        handleDirectorMessage(s.director_message);
      })
```

- [ ] **Step 5: `presenter.js` —— 横幅逻辑(在 `// ── boot ──` 之前加)**

```js
  // ── director → presenter banner ──
  var lastDirSeq = null;
  var dirBannerTimer = null;
  function fmtHM(ms) {
    var d = new Date(ms);
    function p(n) { return (n < 10 ? "0" : "") + n; }
    return p(d.getHours()) + ":" + p(d.getMinutes());
  }
  function handleDirectorMessage(dm) {
    var seq = dm ? dm.seq : 0;
    if (lastDirSeq === null) { lastDirSeq = seq; return; } // 首帧只记基线,不回放旧消息
    if (dm && dm.seq > lastDirSeq) {
      lastDirSeq = dm.seq;
      el.dirBannerText.textContent = dm.text;
      el.dirBannerTime.textContent = fmtHM(dm.at_ms);
      el.dirBanner.hidden = false;
      if (dirBannerTimer) clearTimeout(dirBannerTimer);
      dirBannerTimer = setTimeout(hideDirBanner, 10000);
    }
  }
  function hideDirBanner() {
    if (dirBannerTimer) clearTimeout(dirBannerTimer);
    el.dirBanner.hidden = true;
  }
  el.dirBannerX.addEventListener("click", hideDirBanner);
```

- [ ] **Step 6: 交叉编译(内嵌路径不变)+ 前端构建**

Run 交叉编译(0 error)+ `pnpm build`(成功)。

- [ ] **Step 7: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.html src/presenter/presenter.css src/presenter/presenter.js
git commit -m "feat(presenter): director→presenter banner (status-fed, seq-baselined, 10s auto-hide)"
```

---

## Self-Review

- **Spec 覆盖:** §2 共享池(复用 presets)→ Task 2 校验;§3 IPC + DirectorMessage → Task 1/2;§4 status 折入 → Task 1;§5 主持人横幅 + 首帧基线 → Task 4;§6 导播 UI 可点发送 → Task 3。全覆盖。
- **类型/契约一致:** `DirectorMessage{seq,text,at_ms}`(Rust serialize)↔ status JSON `director_message` ↔ presenter `dm.seq/text/at_ms` 一致;`send_to_presenter(id)` ↔ `api.sendToPresenter(id)` ↔ 命令签名一致;校验 `id ∈ presets` 与主→导一致。
- **编译绿:** 每个 Rust 任务以交叉编译 + cargo test(23)收尾,每个前端任务以 pnpm build 收尾;`Inner.director_message` 加字段需在 `AppState::new` 初始化(Task 1 已含),无签名变更。
- **Placeholder:** 无;DirectorMessages 给整文件,其余给确切插入/替换块。
- **安全/范围:** `send_to_presenter` 是导播本机 IPC(特权,无需锁门控);主持人经 token 化 `/api/status` 收。不动三层锁、COM、toast 窗、主→导逻辑。`director_message` 不持久化。
