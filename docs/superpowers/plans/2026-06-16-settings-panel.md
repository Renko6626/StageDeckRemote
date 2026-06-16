# 设置面板 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 导播齿轮→模态设置:重置 token、服务端口(持久化、重启生效)、提示音开关(从消息卡移来)。

**Architecture:** 新 IPC `regenerate_token`/`set_server_port`;`config.rs` 启用(setup 启动时加载端口→`inner.server_port`,改端口存 `config.json` 重启生效);前端齿轮开 `SettingsModal.vue`。详见 `docs/superpowers/specs/2026-06-16-settings-panel-design.md`。

**Tech Stack:** Rust(Tauri IPC、config 持久化)、Vue 3。

**验证命令:**
- 原生测试: `P=/data/sunyunbo/miniconda3/envs/tauri-dev; PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml`
- 交叉编译: `cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH cargo xwin build --target x86_64-pc-windows-msvc`
- 前端构建: `cd /data/sunyunbo/www/StageDeckRemote && pnpm build`

---

### Task 1: 后端(命令 + 启动加载端口)

**Files:** `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`

- [ ] **Step 1: `commands.rs` 两个命令(文件末尾)**

```rust
/// 导播 GUI:重置访问 token(踢掉所有已连主持人,二维码刷新)。
#[tauri::command]
pub fn regenerate_token(state: State<Arc<AppState>>) {
    let new = crate::auth::generate_token();
    if let Ok(mut i) = state.inner.lock() {
        i.token = new;
        i.active_client_id = None; // 都要重连,顺带释放控制端
    }
    log::event(&state, "已重置访问码(所有主持人需重新扫码)");
}

/// 导播 GUI:设置服务端口并持久化(下次启动生效;不改运行中的端口)。
#[tauri::command]
pub fn set_server_port(app: tauri::AppHandle, state: State<Arc<AppState>>, port: u16) {
    if let Ok(dir) = app.path().app_config_dir() {
        let path = dir.join("config.json");
        let mut cfg = crate::config::Config::load(&path);
        cfg.server_port = port;
        let _ = cfg.save(&path);
    }
    log::event(&state, format!("服务端口已保存为 {port}(重启生效)"));
}
```

- [ ] **Step 2: `lib.rs` setup 先加载 config 端口再 spawn web**

把 setup 闭包里的:

```rust
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
```

替换为(顺序调整:先加载 config 端口 + presets,再 spawn web 服务):

```rust
            // 先加载持久化配置(端口)+ 预制消息,再启动 Web 服务(使其绑定配置端口)。
            if let Ok(dir) = app.path().app_config_dir() {
                let cfg = config::Config::load(&dir.join("config.json"));
                let presets_loaded = presets::load(&dir.join("presets.json"));
                if let Ok(mut i) = preset_state.inner.lock() {
                    i.server_port = cfg.server_port;
                    i.presets = presets_loaded;
                }
            }

            // 后台启动局域网 Web 服务(主持人端入口)。
            let st = server_state.clone();
            tauri::async_runtime::spawn(async move {
                web_server::run(st).await;
            });
```

- [ ] **Step 3: `lib.rs` 注册命令**

在 `tauri::generate_handler![ ... ]` 里 `commands::release_controller,` 之后加:

```rust
            commands::regenerate_token,
            commands::set_server_port,
```

- [ ] **Step 4: 交叉编译 + 原生测试**

Run 交叉编译(0 error)+ 原生测试(26 passed)。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(settings): regenerate_token + set_server_port (persisted); load port on startup"
```

---

### Task 2: 前端(api + store + 模态 + 齿轮 + 移走提示音)

**Files:** `src/utils/api.ts`、`src/stores/appState.ts`、`src/components/SettingsModal.vue`(新)、`src/App.vue`、`src/components/DirectorMessages.vue`

- [ ] **Step 1: `api.ts` 两个方法**

在 `export const api = { ... }` 里 `releaseController: ...` 之后加:

```ts
  regenerateToken: () => invoke<void>("regenerate_token"),
  setServerPort: (port: number) => invoke<void>("set_server_port", { port }),
```

- [ ] **Step 2: `appState.ts` 两个操作**

在 `releaseController` 操作之后加:

```ts
  async function regenerateToken() {
    await api.regenerateToken();
    await refresh();
  }
  async function setServerPort(port: number) {
    await api.setServerPort(port);
    await refresh();
  }
```

在 `return { ... }` 里加(`releaseController,` 之后):

```ts
    regenerateToken,
    setServerPort,
```

- [ ] **Step 3: 新建 `src/components/SettingsModal.vue`**

```vue
<script setup lang="ts">
// 导播设置模态:重置访问码 / 服务端口(重启生效)/ 提示音开关。
import { ref } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";
import { useNetworkState } from "../stores/networkState";

const emit = defineEmits<{ close: [] }>();
const app = useAppState();
const net = useNetworkState();
const { serverPort } = storeToRefs(net);

const chimeOn = ref(localStorage.getItem("sd.dir.toastChime") !== "off");
const portInput = ref<number>(serverPort.value || 8787);
const confirmReset = ref(false);
const note = ref("");

function flash(t: string) {
  note.value = t;
  window.setTimeout(() => (note.value = ""), 2400);
}
function toggleChime() {
  chimeOn.value = !chimeOn.value;
  localStorage.setItem("sd.dir.toastChime", chimeOn.value ? "on" : "off");
}
async function resetToken() {
  if (!confirmReset.value) {
    confirmReset.value = true;
    return;
  }
  confirmReset.value = false;
  await app.regenerateToken();
  flash("访问码已重置 — 主持人需重新扫码");
}
async function savePort() {
  const p = Math.min(65535, Math.max(1, Math.floor(portInput.value || 0)));
  portInput.value = p;
  await app.setServerPort(p);
  flash("端口已保存 — 重启后生效");
}
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <section class="modal">
      <header class="mhead">
        <span>设置 · SETTINGS</span>
        <button class="x" @click="emit('close')">✕</button>
      </header>

      <div class="grp">
        <div class="glabel">访问码</div>
        <p class="desc">重置后已连接的主持人立即断开,需重新扫码。</p>
        <button class="danger" @click="resetToken">
          {{ confirmReset ? "确认重置?(踢掉所有主持人)" : "重置访问码" }}
        </button>
      </div>

      <div class="grp">
        <div class="glabel">服务端口</div>
        <p class="desc">改端口需重启应用才生效;当前运行端口 {{ serverPort }}。</p>
        <div class="prow">
          <input type="number" v-model.number="portInput" min="1" max="65535" />
          <button @click="savePort">保存(重启生效)</button>
        </div>
      </div>

      <div class="grp">
        <div class="glabel">提示音</div>
        <button class="chime" :class="{ on: chimeOn }" @click="toggleChime">
          收到主持人消息提示音：{{ chimeOn ? "开" : "关" }}
        </button>
      </div>

      <p v-if="note" class="note">{{ note }}</p>
    </section>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: rgba(6, 6, 8, 0.66);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
}
.modal {
  width: 100%;
  max-width: 420px;
  background: var(--panel);
  border: 1px solid var(--line-hi);
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.6);
}
.mhead {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--line-hi);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.2em;
  text-transform: uppercase;
  color: var(--ink-dim);
}
.x {
  border: none;
  background: transparent;
  color: var(--ink-faint);
  font-size: 14px;
  padding: 2px 6px;
}
.grp {
  padding: 14px 16px;
  border-bottom: 1px solid var(--line);
}
.glabel {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.2em;
  text-transform: uppercase;
  color: var(--ink-faint);
  margin-bottom: 6px;
}
.desc {
  margin: 0 0 9px;
  font-size: 12px;
  color: var(--ink-dim);
  line-height: 1.45;
}
.danger {
  width: 100%;
  border-color: var(--danger);
  color: var(--danger);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 9px;
}
.danger:hover:not(:disabled) {
  background: var(--danger);
  color: #fff;
  border-color: var(--danger);
}
.prow {
  display: flex;
  gap: 8px;
}
.prow input {
  flex: 1 1 auto;
  background: var(--panel-2);
  border: 1px solid var(--line-hi);
  color: var(--ink);
  font-family: var(--mono);
  font-size: 13px;
  padding: 8px 10px;
}
.prow button {
  flex: 0 0 auto;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 8px 10px;
}
.chime {
  width: 100%;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.1em;
  padding: 9px;
}
.chime.on {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}
.note {
  margin: 0;
  padding: 12px 16px;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--ink);
}
</style>
```

- [ ] **Step 4: `App.vue` 齿轮 + 挂模态**

把 `App.vue` 的 `<script setup lang="ts">` 块(目前仅一行注释)替换为:

```vue
<script setup lang="ts">
// 应用外壳：标题栏 + 齿轮设置 + 路由出口。
import { ref } from "vue";

import SettingsModal from "./components/SettingsModal.vue";

const settingsOpen = ref(false);
</script>
```

把模板里的 `<div v-else class="app"> ... </div>` 整段:

```vue
  <div v-else class="app">
    <header class="topbar">
      <strong class="brand">STAGEDECK<span class="brand-sub">&nbsp;·&nbsp;DIRECTOR</span></strong>
    </header>
    <router-view />
  </div>
```

替换为:

```vue
  <div v-else class="app">
    <header class="topbar">
      <strong class="brand">STAGEDECK<span class="brand-sub">&nbsp;·&nbsp;DIRECTOR</span></strong>
      <button class="gear" @click="settingsOpen = true" title="设置" aria-label="设置">⚙</button>
    </header>
    <router-view />
    <SettingsModal v-if="settingsOpen" @close="settingsOpen = false" />
  </div>
```

在 `<style>` 里 `.topbar .brand-sub { ... }` 规则之后加:

```css
.topbar .gear {
  margin-left: auto;
  font-size: 14px;
  line-height: 1;
  padding: 5px 10px;
  color: var(--ink-dim);
}
```

- [ ] **Step 5: `DirectorMessages.vue` 移走提示音开关**

把脚本里:

```ts
const chimeOn = ref(localStorage.getItem("sd.dir.toastChime") !== "off");
const error = ref<string | null>(null);
```

改为(删 chimeOn 行):

```ts
const error = ref<string | null>(null);
```

删掉 `toggleChime` 函数:

```ts
function toggleChime() {
  chimeOn.value = !chimeOn.value;
  localStorage.setItem("sd.dir.toastChime", chimeOn.value ? "on" : "off");
}
```

把模板里那一行(含 chime 按钮)的 `.row`:

```vue
      <div class="row">
        <button @click="startEdit">编辑</button>
        <button class="chime" :class="{ on: chimeOn }" @click="toggleChime">
          提示音：{{ chimeOn ? "开" : "关" }}
        </button>
      </div>
```

替换为:

```vue
      <div class="row">
        <button @click="startEdit">编辑</button>
      </div>
```

把第 2 行注释:

```ts
// 快捷消息预制编辑(导播)+ 点一条发给主持人 + 提示音开关。
```

改为:

```ts
// 快捷消息预制编辑(导播)+ 点一条发给主持人。
```

(`.chime.on` 的 CSS 规则可留可删;若删,移除 `<style scoped>` 里 `.chime.on { ... }` 那 5 行——非必需,留着无害。)

- [ ] **Step 6: 前端构建**

Run `pnpm build`。Expected:vue-tsc 通过(`DirectorMessages` 无悬空 `chimeOn`/`toggleChime`)、vite 成功。

- [ ] **Step 7: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/utils/api.ts src/stores/appState.ts src/components/SettingsModal.vue src/App.vue src/components/DirectorMessages.vue
git commit -m "feat(director): settings modal (reset token / server port / chime); gear in topbar"
```

---

## Self-Review
- **Spec 覆盖:** §2 入口/外壳 → Task 2 Step 3-4;§3.1 token → Task 1 + Task 2;§3.2 端口 → Task 1(命令+启动加载)+ Task 2(模态);§3.3 提示音移动 → Task 2 Step 3+5;§4 启动接线 → Task 1 Step 2。
- **类型/契约一致:** `regenerate_token()`/`set_server_port(port)` ↔ `api.regenerateToken`/`setServerPort(port)`(`{ port }`)↔ 命令签名;`config.json` 端口 ↔ 启动 `i.server_port`;`localStorage['sd.dir.toastChime']` 键在 SettingsModal 写、ToastView 读一致(从 DirectorMessages 移走)。
- **行为:** 端口**不改运行中**(`inner.server_port` 仅启动时由 config 设),避免二维码指向未监听端口;token 重置运行时生效、清控制端。
- **Placeholder:** 无;新文件给整文件,其余确切 old→new。
- **范围:** 不动锁/COM/计时/在线/捕获。
