# 导播页可拖拽分栏 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给导播页加两根拖条(左栏宽 / 预览·日志高),拖动即调、记忆 localStorage,并给更合理默认;日志列表改为填满容器。

**Architecture:** 纯导播前端。`DirectorView.vue` 用 CSS 变量(`--rail-w`/`--log-h`)驱动栅格与日志高,两个 `ref` + Pointer 拖动(夹紧 + 持久化);`LogPanel.vue` 列表去 220px 上限改为 flex 填满。详见 `docs/superpowers/specs/2026-06-16-director-resizable-design.md`。

**Tech Stack:** Vue 3 `<script setup>`,Pointer Events,localStorage,Vite。

**验证命令:**
```bash
cd /data/sunyunbo/www/StageDeckRemote && pnpm build
```
期望:`vue-tsc --noEmit` 通过、`vite build` 成功。

## 文件结构

| 文件 | 改动 |
|---|---|
| `src/components/LogPanel.vue` | 日志列表去 `max-height:220px`,改 flex 填满容器 |
| `src/views/DirectorView.vue` | 两根拖条 + Pointer 拖动 + 夹紧 + localStorage + CSS 变量;默认 380/300;媒体查询 |

---

### Task 1: `LogPanel.vue` 列表填满容器

**Files:**
- Modify: `src/components/LogPanel.vue`(仅 `<style scoped>`)

- [ ] **Step 1: 把 `.log ul` 的固定高度改为 flex 填满**

把 `LogPanel.vue` 的 `<style scoped>` 中:

```css
.log ul {
  list-style: none;
  margin: 0;
  padding: 0;
  max-height: 220px;
  overflow: auto;
}
```

替换为:

```css
.log {
  display: flex;
  flex-direction: column;
  min-height: 0;
  box-sizing: border-box;
}

.log h2 {
  flex: 0 0 auto;
}

.log ul {
  list-style: none;
  margin: 0;
  padding: 0;
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
}
```

> 卡片自身的高度由父级 `DirectorView` 的 `.pane-log`(`height: var(--log-h)`)给定;此处只负责"在给定高度内,标题固定、列表吃满并自滚"。`box-sizing:border-box` 让 `.card` 的内边距计入该高度。

- [ ] **Step 2: 构建**

Run `pnpm build`。Expected:成功(此改动单独不影响其它页面;LogPanel 仍在导播页使用)。

- [ ] **Step 3: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/components/LogPanel.vue
git commit -m "style(log): list fills its container (drop 220px cap) for resizable log pane"
```

---

### Task 2: `DirectorView.vue` 两根拖条 + 记忆 + 默认

**Files:**
- Modify(整文件覆盖): `src/views/DirectorView.vue`

- [ ] **Step 1: 用以下完整内容覆盖 `src/views/DirectorView.vue`**

```vue
<script setup lang="ts">
// 导播端主面板:方案 B + 可拖拽分栏(左栏宽 / 预览·日志高,夹紧并记忆到 localStorage)。
import { onMounted, onUnmounted, ref } from "vue";

import { useAppState } from "../stores/appState";
import ControlStatus from "../components/ControlStatus.vue";
import WindowPicker from "../components/WindowPicker.vue";
import QrCodePanel from "../components/QrCodePanel.vue";
import LocalControlPanel from "../components/LocalControlPanel.vue";
import LogPanel from "../components/LogPanel.vue";
import DirectorPreview from "../components/DirectorPreview.vue";

const app = useAppState();
let timer: number | undefined;

// ── 可拖拽分栏 ──
const RAIL_MIN = 300;
const RAIL_MAX = 600;
const RAIL_DEFAULT = 380;
const LOG_MIN = 160;
const LOG_MAX = 800;
const LOG_DEFAULT = 300;

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

function readStored(key: string, def: number, lo: number, hi: number): number {
  const raw = localStorage.getItem(key);
  const n = raw == null ? NaN : Number(raw);
  return Number.isFinite(n) ? clamp(n, lo, hi) : def;
}

const railWidth = ref(readStored("sd.dir.railW", RAIL_DEFAULT, RAIL_MIN, RAIL_MAX));
const logHeight = ref(readStored("sd.dir.logH", LOG_DEFAULT, LOG_MIN, LOG_MAX));

// 竖向拖条:调左栏宽度。
function startRail(e: PointerEvent) {
  e.preventDefault();
  const h = e.currentTarget as HTMLElement;
  h.setPointerCapture(e.pointerId);
  const startX = e.clientX;
  const startW = railWidth.value;
  const move = (ev: PointerEvent) => {
    railWidth.value = clamp(startW + (ev.clientX - startX), RAIL_MIN, RAIL_MAX);
  };
  const up = () => {
    h.removeEventListener("pointermove", move);
    h.removeEventListener("pointerup", up);
    localStorage.setItem("sd.dir.railW", String(railWidth.value));
  };
  h.addEventListener("pointermove", move);
  h.addEventListener("pointerup", up);
}

// 横向拖条:调预览/日志高度比(向上拖增大日志)。
function startLog(e: PointerEvent) {
  e.preventDefault();
  const h = e.currentTarget as HTMLElement;
  h.setPointerCapture(e.pointerId);
  const startY = e.clientY;
  const startH = logHeight.value;
  const mainH = h.parentElement ? h.parentElement.clientHeight : 0;
  const upper = mainH > 0 ? Math.min(LOG_MAX, mainH - 200) : LOG_MAX;
  const move = (ev: PointerEvent) => {
    logHeight.value = clamp(startH + (startY - ev.clientY), LOG_MIN, upper);
  };
  const up = () => {
    h.removeEventListener("pointermove", move);
    h.removeEventListener("pointerup", up);
    localStorage.setItem("sd.dir.logH", String(logHeight.value));
  };
  h.addEventListener("pointermove", move);
  h.addEventListener("pointerup", up);
}

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
  <div class="dashboard" :style="{ '--rail-w': railWidth + 'px', '--log-h': logHeight + 'px' }">
    <aside class="rail">
      <WindowPicker />
      <ControlStatus />
      <LocalControlPanel />
      <QrCodePanel />
    </aside>
    <div class="vsplit" @pointerdown="startRail" title="拖动调整栏宽"></div>
    <main class="main">
      <DirectorPreview class="pane-preview" />
      <div class="hsplit" @pointerdown="startLog" title="拖动调整预览/日志比例"></div>
      <LogPanel class="pane-log" />
    </main>
  </div>
</template>

<style scoped>
.dashboard {
  display: grid;
  grid-template-columns: var(--rail-w) 12px minmax(0, 1fr);
  gap: 0;
  padding: 16px;
  height: calc(100vh - 38px); /* 减去顶栏(App.vue .topbar,实测约 36px)高度 */
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
  min-height: 0;
}
.pane-preview {
  flex: 1 1 0;
  min-height: 0;
}
.pane-log {
  flex: 0 0 auto;
  height: var(--log-h);
  min-height: 0;
}

/* ── splitters ── */
.vsplit,
.hsplit {
  position: relative;
  background: var(--line);
  transition: background 0.1s linear;
  touch-action: none;
}
.vsplit {
  cursor: col-resize;
}
.hsplit {
  flex: 0 0 12px;
  cursor: row-resize;
}
.vsplit:hover,
.hsplit:hover {
  background: var(--ink-dim);
}
/* 居中握把 */
.vsplit::before,
.hsplit::before {
  content: "";
  position: absolute;
  inset: 0;
  margin: auto;
  background: var(--line-hi);
}
.vsplit::before {
  width: 2px;
  height: 28px;
}
.hsplit::before {
  width: 28px;
  height: 2px;
}

@media (max-width: 980px) {
  .dashboard {
    grid-template-columns: 1fr;
    height: auto;
    overflow: visible;
  }
  .vsplit,
  .hsplit {
    display: none;
  }
  .pane-preview {
    flex: none;
    height: 56.25vw;
  }
  .pane-log {
    height: 280px;
  }
}
</style>
```

- [ ] **Step 2: 构建**

Run `pnpm build`。Expected:`vue-tsc` 通过、`vite build` 成功。

- [ ] **Step 3: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/views/DirectorView.vue
git commit -m "feat(director): draggable rail width + preview/log split (clamped, persisted)"
```

---

## Self-Review

- **Spec 覆盖:** §2.1 两拖条 → Task 2 模板/样式;§2.2 CSS 变量驱动 → Task 2;§2.3 Pointer 拖动+夹紧 → `startRail`/`startLog`;§2.4 localStorage 记忆+读时夹紧 → `readStored`/`up`;§2.5 响应式 → 媒体查询;§3 LogPanel 填满 → Task 1。全覆盖。
- **类型/选择器一致:** `.pane-preview`/`.pane-log` 类加在子组件标签上,DirectorView 的 scoped 样式作用于其根元素;`--rail-w`/`--log-h` 在 `.dashboard` 内联定义、被 `.pane-log` 继承;LogPanel 根高度来自 `.pane-log`、内部 ul 填满。`startRail`/`startLog` 签名与 `@pointerdown` 一致。
- **Placeholder:** 无;两文件均给完整内容/替换块。
- **取舍:** 夹紧上界用拖动时实测 `.main` 高度(`mainH-200`),保证预览不被拖没;读取记忆时用静态上界 `[160,800]`(无需 DOM)。媒体查询用样式表规则覆盖 `var(--log-h)`(尺寸不走内联 height,故可覆盖)。
- **回归:** 后端/`api.ts`/`networkState`/`DirectorPreview`/锁/主持人端不动;`pnpm build` 的 vue-tsc 兜底。
