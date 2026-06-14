# Director Industrial Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the director GUI to a "cold industrial control surface" monochrome aesthetic matching `src/presenter/presenter.css`, and add a COM status panel while removing the VBA spike panel.

**Architecture:** All design tokens live as CSS vars in `App.vue`'s global `<style>`; scoped styles in each component reference those vars. A new `ComStatusPanel.vue` polls `api.getSlideState()` every 1500ms and displays PowerPoint COM status. The VBA spike panel is removed entirely.

**Tech Stack:** Vue 3 + TypeScript + Vite + Tauri (IPC via `@tauri-apps/api/core`)

---

## File Map

| File | Action | What changes |
|---|---|---|
| `src/App.vue` | Modify | Replace global `<style>` with design token system; update topbar brand text |
| `src/utils/api.ts` | Modify | Add `SlideAction`/`SlideState` types + `getSlideState`; remove `vbaListMacros`/`vbaRunMacro` |
| `src/components/ComStatusPanel.vue` | Create | New COM status panel polling every 1500ms |
| `src/views/DirectorView.vue` | Modify | Remove VbaSpikePanel; import + mount ComStatusPanel in col 2 before ControlStatus |
| `src/components/ControlStatus.vue` | Modify | Replace green/amber `.ok/.warn` with `--ink`/`--ink-dim`; keep `--danger` only for "锁定" |
| `src/components/LocalControlPanel.vue` | Modify | Replace green `.on` and dark red `.danger` with invert/`--danger` system |
| `src/components/WindowPicker.vue` | Modify | Mono labels, sharp corners, instrument feel |
| `src/components/QrCodePanel.vue` | Modify | Mono labels, remove `border-radius` from QR canvas wrapper |
| `src/components/LogPanel.vue` | Modify | Mono timestamps, `--line` separators, faint empty state |
| `src/components/VbaSpikePanel.vue` | Delete (`git rm`) | Gone entirely |

---

### Task 1: Update `src/utils/api.ts` — add types, add `getSlideState`, remove VBA methods

**Files:**
- Modify: `src/utils/api.ts`

- [ ] **Step 1: Apply the edit**

Replace the entire file content with:

```ts
// 导播端 IPC 客户端：封装对 Rust Tauri 命令的调用（commands.rs）。
// 注意：这些命令只在 Tauri 窗口内可用；主持人端走的是 HTTP API（见后端 routes.rs）。
import { invoke } from "@tauri-apps/api/core";

export interface TargetWindow {
  title: string;
  left: number;
  top: number;
  width: number;
  height: number;
  handle: number;
}

export interface DirectorStatus {
  remote_enabled: boolean;
  control_locked: boolean;
  presenter_unlocked: boolean;
  selected_window: TargetWindow | null;
  active_client_id: string | null;
  token: string;
  server_port: number;
  local_ip: string | null;
  access_url: string | null;
}

export interface LogEntry {
  time_ms: number;
  message: string;
}

export interface SlideAction {
  label: string;
  macro_name: string;
}

export interface SlideState {
  available: boolean;
  in_show: boolean;
  current: number;
  total: number;
  actions: SlideAction[];
}

export const api = {
  getStatus: () => invoke<DirectorStatus>("get_status"),
  listWindows: () => invoke<TargetWindow[]>("list_windows"),
  autoDetectPpt: () => invoke<TargetWindow | null>("auto_detect_ppt"),
  selectWindow: (window: TargetWindow) => invoke<void>("select_window", { window }),
  setRemoteEnabled: (enabled: boolean) => invoke<void>("set_remote_enabled", { enabled }),
  forceLock: (locked: boolean) => invoke<void>("force_lock", { locked }),
  localNext: () => invoke<void>("local_next"),
  localPrev: () => invoke<void>("local_prev"),
  localBlack: () => invoke<void>("local_black"),
  localRestore: () => invoke<void>("local_restore"),
  getLogs: () => invoke<LogEntry[]>("get_logs"),
  getSlideState: () => invoke<SlideState>("get_slide_state"),
};
```

- [ ] **Step 2: Verify typecheck still passes**

```bash
cd /data/sunyunbo/www/StageDeckRemote && pnpm typecheck
```

Expected: 0 errors (VbaSpikePanel still exists at this point; that's fine — it will be removed in Task 4).

- [ ] **Step 3: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/utils/api.ts
git commit -m "feat(api): add SlideState types + getSlideState; remove vba stubs"
```

---

### Task 2: Replace global styles in `src/App.vue`

**Files:**
- Modify: `src/App.vue`

- [ ] **Step 1: Apply the edit**

Replace the entire `<style>` (non-scoped) block and update the template brand text. Full file:

```vue
<script setup lang="ts">
// 应用外壳：标题栏 + 路由出口。
</script>

<template>
  <div class="app">
    <header class="topbar">
      <strong class="brand">STAGEDECK<span class="brand-sub">&nbsp;·&nbsp;DIRECTOR</span></strong>
    </header>
    <router-view />
  </div>
</template>

<style>
:root {
  --bg: #0a0a0b;
  --panel: #0e0e10;
  --panel-2: #131316;
  --line: #26262b;
  --line-hi: #3a3a42;
  --ink: #ededee;
  --ink-dim: #8b8b93;
  --ink-faint: #5a5a62;
  --danger: #d23b3b;
  --sans: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
    "Microsoft YaHei", system-ui, sans-serif;
  --mono: ui-monospace, "Cascadia Mono", "JetBrains Mono", "SF Mono",
    "Segoe UI Mono", Menlo, Consolas, monospace;
}

* {
  box-sizing: border-box;
}

html,
body {
  margin: 0;
  min-height: 100%;
  background: var(--bg);
  color: var(--ink);
  font-family: var(--sans);
}

body::before {
  content: "";
  position: fixed;
  inset: 0;
  background-image:
    linear-gradient(var(--line) 1px, transparent 1px),
    linear-gradient(90deg, var(--line) 1px, transparent 1px);
  background-size: 28px 28px;
  opacity: 0.16;
  pointer-events: none;
  z-index: 0;
}

.app {
  position: relative;
  z-index: 1;
  min-height: 100vh;
}

.topbar {
  display: flex;
  align-items: center;
  padding: 10px 18px;
  border-bottom: 1px solid var(--line-hi);
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
}

.topbar .brand {
  font-weight: 700;
  letter-spacing: 0.18em;
  color: var(--ink);
}

.topbar .brand-sub {
  color: var(--ink-faint);
  font-weight: 400;
}

.card {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 0;
  padding: 14px 16px;
}

.card h2 {
  margin: 0 0 12px;
  font-family: var(--mono);
  font-size: 10px;
  font-weight: 400;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  color: var(--ink-dim);
}

button {
  appearance: none;
  border: 1px solid var(--line-hi);
  background: transparent;
  color: var(--ink);
  border-radius: 0;
  padding: 9px 12px;
  font-size: 14px;
  font-family: var(--sans);
  cursor: pointer;
  transition: background 0.06s linear, color 0.06s linear, border-color 0.06s linear;
}

button:hover:not(:disabled) {
  background: var(--panel-2);
  border-color: var(--ink-dim);
}

button:active:not(:disabled) {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

button:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/App.vue
git commit -m "feat(ui): App.vue — industrial design tokens, topbar, card, button base"
```

---

### Task 3: Create `src/components/ComStatusPanel.vue`

**Files:**
- Create: `src/components/ComStatusPanel.vue`

- [ ] **Step 1: Write the file**

```vue
<script setup lang="ts">
// COM 状态面板：轮询 PowerPoint COM 连接状态。
import { ref, onMounted, onUnmounted } from "vue";
import { api, type SlideState } from "../utils/api";

const state = ref<SlideState | null>(null);
const error = ref<string | null>(null);
let timer: number | undefined;

async function poll() {
  try {
    state.value = await api.getSlideState();
    error.value = null;
  } catch (e) {
    error.value = String(e);
    state.value = null;
  }
}

onMounted(() => {
  void poll();
  timer = window.setInterval(poll, 1500);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <section class="card com-panel">
    <h2>PowerPoint · COM</h2>

    <div class="rows">
      <div class="row">
        <span class="k">POWERPOINT</span>
        <span class="v" :class="state?.available ? 'on' : 'off'">
          {{ state?.available ? "已连接" : "未连接" }}
        </span>
      </div>

      <div class="row">
        <span class="k">当前页</span>
        <span class="v readout">
          <template v-if="state?.available">
            {{ state.current }}<span class="sep">/</span>{{ state.total }}
          </template>
          <template v-else>—</template>
        </span>
      </div>

      <div class="row">
        <span class="k">放映</span>
        <span class="chip" :class="state?.in_show ? 'live' : 'idle'">
          {{ state?.in_show ? "LIVE" : "IDLE" }}
        </span>
      </div>
    </div>

    <p v-if="error" class="err">{{ error }}</p>
  </section>
</template>

<style scoped>
.com-panel {
  padding: 14px 16px;
}

.rows {
  display: grid;
  gap: 10px;
}

.row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.k {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  color: var(--ink-faint);
}

.v {
  font-family: var(--mono);
  font-size: 14px;
  letter-spacing: 0.06em;
  color: var(--ink-dim);
}

.v.on {
  color: var(--ink);
}

.readout {
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--ink);
}

.sep {
  font-size: 14px;
  color: var(--ink-faint);
  margin: 0 2px;
}

.chip {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  border: 1px solid var(--line-hi);
  padding: 2px 8px;
  color: var(--ink-faint);
}

.chip.live {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

.err {
  margin: 10px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/ComStatusPanel.vue
git commit -m "feat(ui): add ComStatusPanel — COM/PowerPoint status with polling"
```

---

### Task 4: Update `src/views/DirectorView.vue` — swap VbaSpikePanel for ComStatusPanel

**Files:**
- Modify: `src/views/DirectorView.vue`
- Delete: `src/components/VbaSpikePanel.vue`

- [ ] **Step 1: Apply the edit to DirectorView.vue**

Replace entire file:

```vue
<script setup lang="ts">
// 导播端主面板：组合各控制组件，并定时轮询后端状态（spec §12.1）。
import { onMounted, onUnmounted } from "vue";

import { useAppState } from "../stores/appState";
import ControlStatus from "../components/ControlStatus.vue";
import WindowPicker from "../components/WindowPicker.vue";
import QrCodePanel from "../components/QrCodePanel.vue";
import LocalControlPanel from "../components/LocalControlPanel.vue";
import LogPanel from "../components/LogPanel.vue";
import ComStatusPanel from "../components/ComStatusPanel.vue";

const app = useAppState();
let timer: number | undefined;

onMounted(() => {
  void app.refresh();
  timer = window.setInterval(() => void app.refresh(), 1500);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <div class="dashboard">
    <div class="col">
      <WindowPicker />
      <QrCodePanel />
    </div>
    <div class="col">
      <ComStatusPanel />
      <ControlStatus />
      <LocalControlPanel />
    </div>
    <div class="col">
      <LogPanel />
    </div>
  </div>
</template>

<style scoped>
.dashboard {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
  padding: 16px;
  align-items: start;
}

.col {
  display: grid;
  gap: 16px;
}

@media (max-width: 980px) {
  .dashboard {
    grid-template-columns: 1fr;
  }
}
</style>
```

- [ ] **Step 2: Delete VbaSpikePanel.vue**

```bash
cd /data/sunyunbo/www/StageDeckRemote && git rm src/components/VbaSpikePanel.vue
```

- [ ] **Step 3: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/views/DirectorView.vue
git commit -m "feat(ui): DirectorView — mount ComStatusPanel, drop VbaSpikePanel"
```

---

### Task 5: Restyle `src/components/ControlStatus.vue`

**Files:**
- Modify: `src/components/ControlStatus.vue`

- [ ] **Step 1: Apply scoped style changes**

Replace only the `<style scoped>` block:

```vue
<style scoped>
.status {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 0;
}

.status li {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 7px 0;
  border-bottom: 1px solid var(--line);
  font-size: 13px;
}

.status li:last-child {
  border-bottom: none;
}

.k {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--ink-faint);
}

.v {
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.08em;
  color: var(--ink-dim);
}

/* ok = white (active/connected/enabled) */
.v.ok {
  color: var(--ink);
}

/* bad = danger red (locked/disconnected) — only for hard-danger states */
.v.bad {
  color: var(--danger);
}

/* warn = dim (not yet active but not dangerous) */
.v.warn {
  color: var(--ink-dim);
}
</style>
```

Note: "强制锁=锁定" maps to `.bad` (danger), which remains `--danger`. "服务=未连接" and "PPT窗口=未选择" map to `.bad` — these are "not ready" states shown in red per the design (they are functional blockers). "远程控制=禁止" maps to `.bad`. This preserves the semantics while using the design tokens.

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/ControlStatus.vue
git commit -m "feat(ui): ControlStatus — industrial mono style, danger-only red"
```

---

### Task 6: Restyle `src/components/LocalControlPanel.vue`

**Files:**
- Modify: `src/components/LocalControlPanel.vue`

- [ ] **Step 1: Apply scoped style changes**

Replace only the `<style scoped>` block:

```vue
<style scoped>
.master {
  display: grid;
  gap: 8px;
  margin-bottom: 10px;
}

.big {
  padding: 11px 12px;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  font-weight: 600;
  width: 100%;
}

/* remote enabled = inverted (active/on state) */
.big.on {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

/* force-lock active = danger red */
.big.danger {
  background: var(--danger);
  color: #fff;
  border-color: var(--danger);
}

.grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 8px;
}

.grid button {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  padding: 10px 8px;
}

.error {
  margin: 10px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/LocalControlPanel.vue
git commit -m "feat(ui): LocalControlPanel — mono buttons, invert-on/danger-lock"
```

---

### Task 7: Restyle `src/components/WindowPicker.vue`

**Files:**
- Modify: `src/components/WindowPicker.vue`

- [ ] **Step 1: Apply scoped style changes**

Replace only the `<style scoped>` block:

```vue
<style scoped>
.current {
  margin-bottom: 12px;
  padding: 8px 0;
  border-bottom: 1px solid var(--line);
}

.title {
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.06em;
  color: var(--ink);
  margin-bottom: 4px;
}

.meta {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-faint);
  letter-spacing: 0.06em;
}

.actions {
  display: flex;
  gap: 8px;
  margin-bottom: 2px;
}

.actions button {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px 10px;
}

.error {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  margin: 8px 0 0;
}

.win-list {
  list-style: none;
  padding: 0;
  margin: 10px 0 0;
  display: grid;
  gap: 4px;
  max-height: 160px;
  overflow: auto;
}

.win {
  width: 100%;
  text-align: left;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.04em;
  padding: 8px 10px;
}

.hint {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--ink-faint);
  letter-spacing: 0.06em;
  margin: 8px 0 0;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/WindowPicker.vue
git commit -m "feat(ui): WindowPicker — mono instrument styling"
```

---

### Task 8: Restyle `src/components/QrCodePanel.vue`

**Files:**
- Modify: `src/components/QrCodePanel.vue`

- [ ] **Step 1: Apply scoped style changes**

Replace only the `<style scoped>` block:

```vue
<style scoped>
.net {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.1em;
  color: var(--ink-faint);
  margin-bottom: 12px;
}

.net b {
  color: var(--ink);
  font-weight: 600;
}

.qr-row {
  display: flex;
  gap: 14px;
  align-items: flex-start;
}

.qr {
  border: 1px solid var(--line-hi);
  border-radius: 0;
  flex: 0 0 auto;
  background: #fff;
}

.url-box {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.url {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.04em;
  color: var(--ink);
  word-break: break-all;
}

.muted {
  color: var(--ink-faint);
}

.url-box button {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px 10px;
  align-self: flex-start;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/QrCodePanel.vue
git commit -m "feat(ui): QrCodePanel — mono labels, sharp QR border"
```

---

### Task 9: Restyle `src/components/LogPanel.vue`

**Files:**
- Modify: `src/components/LogPanel.vue`

- [ ] **Step 1: Apply scoped style changes**

Replace only the `<style scoped>` block:

```vue
<style scoped>
.log ul {
  list-style: none;
  margin: 0;
  padding: 0;
  max-height: 220px;
  overflow: auto;
}

.log li {
  display: flex;
  gap: 10px;
  padding: 5px 0;
  border-bottom: 1px solid var(--line);
  font-size: 12px;
  font-family: var(--mono);
  letter-spacing: 0.02em;
}

.log li:last-child {
  border-bottom: none;
}

.t {
  flex: 0 0 auto;
  color: var(--ink-faint);
  font-size: 11px;
  letter-spacing: 0.06em;
}

.m {
  color: var(--ink-dim);
  word-break: break-all;
}

.empty {
  color: var(--ink-faint);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/LogPanel.vue
git commit -m "feat(ui): LogPanel — mono instrument log rows"
```

---

### Task 10: Final verify + consolidation commit

- [ ] **Step 1: Run typecheck and build**

```bash
cd /data/sunyunbo/www/StageDeckRemote && pnpm typecheck && pnpm exec vite build
```

Expected: 0 errors, build succeeds.

- [ ] **Step 2: Verify no VbaSpikePanel references remain**

```bash
grep -r "VbaSpikePanel\|vbaListMacros\|vbaRunMacro" /data/sunyunbo/www/StageDeckRemote/src/
```

Expected: no output.

- [ ] **Step 3: Final consolidation commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add -A
git commit -m "feat(ui): director GUI → B/W industrial; COM status panel; drop spike panel"
```
