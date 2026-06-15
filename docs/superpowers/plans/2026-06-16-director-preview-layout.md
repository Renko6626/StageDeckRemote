# 导播页改版:以实时预览为中心 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把导播页重排成方案 B(左控制栏 | 右:预览↑ 日志↓),中央放只读实时捕获预览,消除空白。

**Architecture:** 纯导播前端(Vue)。新增 `DirectorPreview.vue`(预览头轮询 `get_slide_state` + `<img>` 链式抓 `/api/snapshot`,吸收原 COM 状态卡);重排 `DirectorView.vue` 为两列 flex 栅格;删 `ComStatusPanel.vue`。后端/IPC/数据契约/锁/主持人端不动。详见 `docs/superpowers/specs/2026-06-16-director-preview-layout-design.md`。

**Tech Stack:** Vue 3 `<script setup>` + Pinia,Vite。预览复用本地 HTTP `/api/snapshot.jpg`(CSP 为 `null`,webview 可直接加载)。

**布局要点(实现取舍):** `LogPanel` 内部列表 `max-height:220px` 自带滚动。因此主区用「**预览 `flex:1` 吃满剩余高度 + 日志自然高度贴底**」,而非强制 2:1(否则被截高的日志下方会空)。这忠实于 spec「各自填满、不再参差」的目标。

**验证命令:**
- 前端构建(类型检查 + 打包,会报出对已删组件的悬空引用):
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote && pnpm build
  ```
- 交叉编译(可选 sanity;导播前端经 Tauri 在 build 期内嵌 `dist/`):
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 \
    PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
    cargo xwin build --target x86_64-pc-windows-msvc
  ```

## 文件结构

| 文件 | 改动 |
|---|---|
| `src/components/DirectorPreview.vue` | **新增**:预览头(getSlideState 轮询)+ 链式快照 `<img>` + 占位 |
| `src/views/DirectorView.vue` | 重排为两列 flex 栅格;装 4 面板侧栏 + 预览/日志主区;移除 ComStatusPanel |
| `src/components/ComStatusPanel.vue` | **删除** |

---

### Task 1: 新增 `DirectorPreview.vue`

**Files:**
- Create: `src/components/DirectorPreview.vue`

- [ ] **Step 1: 写入完整组件**

```vue
<script setup lang="ts">
// 实时捕获预览(只读监看)+ COM 状态头条带。
// 预览图复用主持人端的 /api/snapshot 链式刷新;头条带轮询 get_slide_state(IPC)。
import { ref, onMounted, onUnmounted, watch } from "vue";
import { storeToRefs } from "pinia";

import { api, type SlideState } from "../utils/api";
import { useNetworkState } from "../stores/networkState";

const net = useNetworkState();
const { serverPort, token } = storeToRefs(net);

const slide = ref<SlideState | null>(null);
const img = ref<HTMLImageElement | null>(null);
const frameOk = ref(false);
let timer: number | undefined;
let started = false;

async function poll() {
  try {
    slide.value = await api.getSlideState();
  } catch {
    slide.value = null;
  }
}

// 链式刷新:上一帧 load/error 后再排下一帧,避免慢链路堆积。
function loadFrame() {
  if (!img.value || !token.value) return;
  img.value.src =
    "http://127.0.0.1:" +
    serverPort.value +
    "/api/snapshot.jpg?token=" +
    encodeURIComponent(token.value) +
    "&t=" +
    Date.now();
}
function onFrameLoad() {
  frameOk.value = true;
  window.setTimeout(loadFrame, 300);
}
function onFrameError() {
  frameOk.value = false;
  window.setTimeout(loadFrame, 1000);
}

// token 在首次状态轮询后才就位;就位即启动链式刷新(仅一次)。
function kick() {
  if (started || !token.value) return;
  started = true;
  loadFrame();
}

onMounted(() => {
  void poll();
  timer = window.setInterval(poll, 1500);
  kick();
});
watch(token, kick);
onUnmounted(() => {
  if (timer) window.clearInterval(timer);
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
      <img
        ref="img"
        v-show="frameOk"
        alt="PPT 捕获预览"
        draggable="false"
        @load="onFrameLoad"
        @error="onFrameError"
      />
      <div class="placeholder" v-if="!frameOk">
        {{ token ? "等待捕获 · 未选择 PPT 窗口" : "服务未就绪" }}
      </div>
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

- [ ] **Step 2: 构建确认组件可编译**

Run `pnpm build`。Expected:`vue-tsc` 通过、`vite build` 成功(新组件此时尚未被引用,无妨)。

- [ ] **Step 3: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/DirectorPreview.vue
git commit -m "feat(director): DirectorPreview — read-only live capture preview + COM status header"
```

---

### Task 2: 重排 `DirectorView.vue` 为方案 B

**Files:**
- Modify(整文件覆盖): `src/views/DirectorView.vue`

- [ ] **Step 1: 用以下完整内容覆盖 `src/views/DirectorView.vue`**

```vue
<script setup lang="ts">
// 导播端主面板:方案 B —— 左控制栏 | 右(预览↑ 日志↓)。定时轮询后端状态(spec §12.1)。
import { onMounted, onUnmounted } from "vue";

import { useAppState } from "../stores/appState";
import ControlStatus from "../components/ControlStatus.vue";
import WindowPicker from "../components/WindowPicker.vue";
import QrCodePanel from "../components/QrCodePanel.vue";
import LocalControlPanel from "../components/LocalControlPanel.vue";
import LogPanel from "../components/LogPanel.vue";
import DirectorPreview from "../components/DirectorPreview.vue";

const app = useAppState();
let timer: number | undefined;

onMounted(async () => {
  await app.refresh();
  // 启动时未选窗口才自动识别;手动选择为权威,绝不被自动悄悄覆盖。
  if (!app.selectedWindow) {
    try {
      await app.autoDetectPpt();
    } catch {
      /* 自动识别失败不阻塞启动 */
    }
  }
  timer = window.setInterval(() => void app.refresh(), 1500);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <div class="dashboard">
    <aside class="rail">
      <WindowPicker />
      <ControlStatus />
      <LocalControlPanel />
      <QrCodePanel />
    </aside>
    <main class="main">
      <DirectorPreview />
      <LogPanel />
    </main>
  </div>
</template>

<style scoped>
.dashboard {
  display: grid;
  grid-template-columns: 340px minmax(0, 1fr);
  gap: 16px;
  padding: 16px;
  height: calc(100vh - 44px); /* 减去顶栏(App.vue .topbar)高度 */
  overflow: hidden;
}
.rail {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  overflow-y: auto;
}
.main {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
}
/* 预览吃满剩余高度;日志贴底(自然高度,其内部列表自带 220px 滚动)。 */
.main > :first-child {
  flex: 1 1 0;
  min-height: 0;
}
.main > :last-child {
  flex: 0 0 auto;
}

@media (max-width: 980px) {
  .dashboard {
    grid-template-columns: 1fr;
    height: auto;
    overflow: visible;
  }
  /* 堆叠时预览给固定 16:9 高度 */
  .main > :first-child {
    flex: none;
    height: 56.25vw;
  }
}
</style>
```

- [ ] **Step 2: 构建**

Run `pnpm build`。Expected:成功(`DirectorView` 现引用 `DirectorPreview`、不再引用 `ComStatusPanel`;后者文件仍在但已无引用)。

- [ ] **Step 3: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/views/DirectorView.vue
git commit -m "feat(director): layout B — control rail + preview-over-log main"
```

---

### Task 3: 删除 `ComStatusPanel.vue`

**Files:**
- Delete: `src/components/ComStatusPanel.vue`

- [ ] **Step 1: 删除文件**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git rm src/components/ComStatusPanel.vue
```

- [ ] **Step 2: 构建确认无悬空引用**

Run `pnpm build`。Expected:`vue-tsc` 通过(若仍有任何文件 import ComStatusPanel 会在此报错)、`vite build` 成功。

- [ ] **Step 3: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git commit -m "refactor(director): remove ComStatusPanel (folded into preview header)"
```

---

## Self-Review

- **Spec 覆盖:** §2 版面 → Task 2;§3 新组件(预览头折叠 COM 状态 + 链式快照 + 占位)→ Task 1;删 ComStatusPanel → Task 3。
- **取舍说明:** spec §2 写「预览 flex:2 / 日志 flex:1」,实现改为「预览 flex:1 吃满 + 日志自然高度贴底」,因 `LogPanel` 列表 `max-height:220px`,强制 1/3 反而留空;目标(填满、不参差)一致,文件数不变(3 个)。
- **类型/契约一致:** `DirectorPreview` 用 `networkState.{serverPort,token}`(均已存在)与 `api.getSlideState()`/`SlideState`(已存在);`/api/snapshot.jpg` 端点与主持人端一致;无后端改动。
- **Placeholder:** 无;两个 Vue 文件给出完整内容。
- **回归:** 后端、IPC、`presenter.*`、锁、`api.ts`、`networkState` 均不动;仅导播视图与组件变化,`pnpm build` 的 vue-tsc 兜底悬空引用。
