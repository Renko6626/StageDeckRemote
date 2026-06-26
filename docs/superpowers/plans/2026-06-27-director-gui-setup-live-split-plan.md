# 导播 GUI Setup/Live 双模式 — 实现 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把当前单 rail 6-widget 的导播 GUI 重构为 Setup/Live 双模式布局,演出当场不需要滚动条;同时把 `LocalControlPanel` 三合一 blob 按职责拆开。

**Architecture:** 顶栏加 `[Setup] [Live]` 切换。`DirectorView.vue` 顶层维护 `mode: ref<"setup" | "live">`,模板用 `v-if` 在两套 rail 之间切。`ControlStatus` 删除、`LocalControlPanel` 拆三块:新建 `StatusBar.vue`(吸 ControlStatus 全部 + 远控 toggle)、`EmergencyLock.vue`(紧急锁 + size prop)、`LocalNav.vue`(只翻页 + 改名自 LocalControlPanel)。后端 0 修改、Pinia store 0 修改。引入 vitest + @vue/test-utils + @pinia/testing 跑 Vue 组件单测。

**Tech Stack:** Vue 3 + Composition API、TypeScript、Pinia、Vite 6、vitest 1.x、@vue/test-utils 2.x、@pinia/testing、jsdom。

## Global Constraints

- **后端 0 修改**:不动 `src-tauri/` 和 `src/stores/`,本 plan 完全是前端组件层重构。
- **localStorage key 不动**:`sd.dir.railW` / `sd.dir.logH` 保留语义。新增 key 严禁(spec §2.1 明示 mode 不持久化)。
- **冷启动总在 Setup mode**(spec §2.1)。
- **切换 mode 不发任何 IPC 命令**(spec §4)。
- **mode 切换是纯前端 UI**:不动 ARM / control_locked / remote_enabled / token 业务状态(spec §2.1)。
- **TDD**:每个组件的测试在实现\textbf{之前}写。
- **每个 Task 结束时 `pnpm build` 必须绿**(spec 没明示但是 sane software 默认)。

---

## File Structure(本 plan 涉及的全部文件)

**新建**
- `src/components/StatusBar.vue` — 状态显示 + 远控 toggle + releaseController
- `src/components/EmergencyLock.vue` — 紧急锁,带 `size: "small" | "large"` prop
- `src/components/LocalNav.vue` — 本地翻页(上一页 / 下一页)
- `src/components/StatusBar.spec.ts`
- `src/components/EmergencyLock.spec.ts`
- `src/components/LocalNav.spec.ts`
- `src/views/DirectorView.spec.ts`

**修改**
- `package.json` — 加 vitest / @vue/test-utils / @pinia/testing / jsdom 到 devDeps + 加 `test` 脚本
- `vite.config.ts` — 加 `test` 配置块(vitest 直接读 vite.config.ts)
- `src/views/DirectorView.vue` — 加 mode ref + 顶栏 ModeToggle 内联 + 模板两套 rail

**删除**
- `src/components/ControlStatus.vue`(内容被 StatusBar 吸收)
- `src/components/LocalControlPanel.vue`(拆成 StatusBar 的远控 toggle + EmergencyLock + LocalNav)

---

## Task 1: 引入 vitest + 测试基础设施

**Files:**
- Modify: `package.json`(devDeps + scripts)
- Modify: `vite.config.ts`(加 test 配置块)
- Create: `src/test/setup.ts`(全局测试 setup,mock Tauri IPC)
- Create: `src/test/smoke.spec.ts`(冒烟测试,验证基础设施工作)

**Interfaces:**
- Produces: `pnpm test` 命令,跑 vitest 全套测试
- Produces: 全局 mock 了 `@tauri-apps/api/core` 的 `invoke`,所有后续测试不需要重复 mock
- Produces: `createTestingPinia` 可用,后续测试可注入 Pinia store

- [ ] **Step 1: 加 devDependencies + script 到 package.json**

读 `package.json`,在 `devDependencies` 里加 4 个新条目,在 `scripts` 里加 `test`:

```json
{
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "typecheck": "vue-tsc --noEmit",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "devDependencies": {
    "@pinia/testing": "^0.1.5",
    "@tauri-apps/cli": "^2",
    "@types/qrcode": "^1.5.6",
    "@vitejs/plugin-vue": "^5.2.1",
    "@vue/test-utils": "^2.4.6",
    "jsdom": "^25.0.1",
    "typescript": "~5.6.2",
    "vite": "^6.0.3",
    "vitest": "^2.1.8",
    "vue-tsc": "^2.1.10"
  }
}
```

(注:`devDependencies` 按字母排序,这里只显示新增 4 行的相对位置)

- [ ] **Step 2: 修改 vite.config.ts 加 test 块**

完整新 `vite.config.ts`:

```ts
/// <reference types="vitest" />
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],

  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{spec,test}.{ts,js}"],
  },
}));
```

- [ ] **Step 3: 创建 src/test/setup.ts**

```ts
// 全局测试 setup。所有 .spec.ts 自动执行,在 vite.config.ts 的
// test.setupFiles 配置。
import { vi } from "vitest";

// Tauri IPC 在测试环境下不可用,统一桩成 noop。后续每个测试可以
// 自己 vi.mocked(invoke).mockResolvedValue(...) 改返回。
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));
```

- [ ] **Step 4: 创建 src/test/smoke.spec.ts(冒烟测试)**

```ts
import { describe, it, expect } from "vitest";

describe("vitest infrastructure", () => {
  it("can run a basic test", () => {
    expect(1 + 1).toBe(2);
  });

  it("has jsdom environment", () => {
    const div = document.createElement("div");
    div.textContent = "ok";
    expect(div.textContent).toBe("ok");
  });
});
```

- [ ] **Step 5: 装新 dep + 跑冒烟测试**

```bash
pnpm install
pnpm test
```

Expected: `Test Files  1 passed (1)` 含 `vitest infrastructure > can run a basic test` 和 `> has jsdom environment` 两个 ok。

- [ ] **Step 6: 跑 pnpm build 确认未破坏现有构建**

```bash
pnpm build
```

Expected: `vue-tsc --noEmit` 通过(没 type error);`vite build` 输出 `dist/` 目录,无 error。

- [ ] **Step 7: Commit**

```bash
git add package.json pnpm-lock.yaml vite.config.ts src/test/
git commit -m "test: introduce vitest + @vue/test-utils + jsdom infrastructure

设计 spec §5.1 要求引 @vue/test-utils 做 Vue 组件单测。引这次顺手
把 vitest + jsdom + @pinia/testing 一起加上。

- vite.config.ts 加 test 块,vitest 直接读;jsdom + globals
- src/test/setup.ts 全局 mock @tauri-apps/api/core, /event, /plugin-opener
  → 后续测试不需要重复 mock IPC
- src/test/smoke.spec.ts 验证 vitest + jsdom 跑得通
- pnpm test 跑 vitest run;pnpm test:watch 跑 watch 模式

后续 Task 2-5 的所有组件单测都走这套基础设施。"
```

---

## Task 2: EmergencyLock.vue 组件

**Files:**
- Create: `src/components/EmergencyLock.vue`
- Create: `src/components/EmergencyLock.spec.ts`

**Interfaces:**
- Consumes: Pinia `useAppState` 的 `controlLocked` ref + `forceLock(boolean)` action
- Produces: 新组件 `<EmergencyLock size="small" />` 或 `<EmergencyLock size="large" />`
- Produces: 点击按钮调用 `app.forceLock(!controlLocked.value)`(toggle)
- Produces: 按钮文字根据 `controlLocked` 切换("紧急锁定" / "已紧急锁定(点击释放)")
- Produces: `size="large"` 时按钮高度变大(实现里 CSS data attr 切)

- [ ] **Step 1: 写失败的测试**

`src/components/EmergencyLock.spec.ts`:

```ts
import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";

import EmergencyLock from "./EmergencyLock.vue";
import { useAppState } from "../stores/appState";

function mountWithPinia(props: { size: "small" | "large" }, initialLocked = false) {
  const pinia = createTestingPinia({
    createSpy: vi.fn,
    initialState: {
      appState: {
        controlLocked: initialLocked,
      },
    },
    stubActions: true,
  });
  return mount(EmergencyLock, { props, global: { plugins: [pinia] } });
}

describe("EmergencyLock", () => {
  it("renders 紧急锁定 when not locked", () => {
    const wrapper = mountWithPinia({ size: "small" });
    expect(wrapper.text()).toContain("紧急锁定");
    expect(wrapper.text()).not.toContain("点击释放");
  });

  it("renders 已紧急锁定 when locked", () => {
    const wrapper = mountWithPinia({ size: "small" }, true);
    expect(wrapper.text()).toContain("已紧急锁定");
    expect(wrapper.text()).toContain("点击释放");
  });

  it("calls app.forceLock(true) when clicked from unlocked", async () => {
    const wrapper = mountWithPinia({ size: "small" }, false);
    const app = useAppState();
    await wrapper.find("button").trigger("click");
    expect(app.forceLock).toHaveBeenCalledWith(true);
  });

  it("calls app.forceLock(false) when clicked from locked", async () => {
    const wrapper = mountWithPinia({ size: "small" }, true);
    const app = useAppState();
    await wrapper.find("button").trigger("click");
    expect(app.forceLock).toHaveBeenCalledWith(false);
  });

  it("applies data-size='small' to button when size=small", () => {
    const wrapper = mountWithPinia({ size: "small" });
    expect(wrapper.find("button").attributes("data-size")).toBe("small");
  });

  it("applies data-size='large' to button when size=large", () => {
    const wrapper = mountWithPinia({ size: "large" });
    expect(wrapper.find("button").attributes("data-size")).toBe("large");
  });
});
```

- [ ] **Step 2: 跑测试,确认失败(组件还没创建)**

```bash
pnpm test src/components/EmergencyLock.spec.ts
```

Expected: `FAIL` with `Failed to resolve import "./EmergencyLock.vue"`。

- [ ] **Step 3: 创建 src/components/EmergencyLock.vue**

```vue
<script setup lang="ts">
// 紧急锁(从 LocalControlPanel.vue 拆出)。
// size="small" — Setup 模式 / 普通按钮高度。
// size="large" — Live 模式 / 顶部巨型按钮,占满整宽,红色高亮。
import { ref } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";

defineProps<{ size: "small" | "large" }>();

const app = useAppState();
const { controlLocked } = storeToRefs(app);

const lastError = ref<string | null>(null);

async function toggle() {
  lastError.value = null;
  try {
    await app.forceLock(!controlLocked.value);
  } catch (e) {
    lastError.value = String(e);
  }
}
</script>

<template>
  <section class="card">
    <button
      :data-size="$props.size"
      :class="{ danger: controlLocked }"
      @click="toggle"
    >
      {{ controlLocked ? "已紧急锁定(点击释放)" : "紧急锁定" }}
    </button>
    <p v-if="lastError" class="error">{{ lastError }}</p>
  </section>
</template>

<style scoped>
button {
  width: 100%;
  font-family: var(--mono);
  letter-spacing: 0.16em;
  text-transform: uppercase;
  font-weight: 600;
}

button[data-size="small"] {
  padding: 11px 12px;
  font-size: 11px;
}

button[data-size="large"] {
  padding: 28px 16px;
  font-size: 16px;
  letter-spacing: 0.22em;
}

button.danger {
  background: var(--danger);
  color: #fff;
  border-color: var(--danger);
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

- [ ] **Step 4: 跑测试,确认通过**

```bash
pnpm test src/components/EmergencyLock.spec.ts
```

Expected: `Test Files  1 passed (1)`,6 个测试全绿。

- [ ] **Step 5: pnpm build 确认 type check 过**

```bash
pnpm build
```

Expected: 无 type error,`dist/` 输出。

- [ ] **Step 6: Commit**

```bash
git add src/components/EmergencyLock.vue src/components/EmergencyLock.spec.ts
git commit -m "feat(director): EmergencyLock 组件 + 6 个单测

从 LocalControlPanel.vue 里把紧急锁按钮拆出来,带 size prop 控制
'普通按钮' (Setup 模式用) vs '顶部巨型红色按钮' (Live 模式用) 两种视觉。

行为不变 — 点一下调 app.forceLock(!controlLocked.value),
toggle 语义和 LocalControlPanel 时代一致。

DirectorView 还没切过来 (Task 5 才切),老的 LocalControlPanel 暂时
仍含紧急锁,两份代码共存,不影响功能。"
```

---

## Task 3: StatusBar.vue 组件

**Files:**
- Create: `src/components/StatusBar.vue`
- Create: `src/components/StatusBar.spec.ts`

**Interfaces:**
- Consumes: Pinia `useAppState` 的 `remoteEnabled` / `controlLocked` / `presenterUnlocked` / `selectedWindow` / `activeClientId` / `presenterOnline` refs + `setRemoteEnabled(boolean)` + `releaseController()` actions
- Consumes: Pinia `useNetworkState` 的 `online` ref
- Produces: 新组件 `<StatusBar mode="setup" />` 或 `<StatusBar mode="live" />`
- Produces: 显示 7 个状态行 + 1 个远控 toggle 按钮 + 1 个 releaseController 按钮
- Produces: `mode` 控制 toggle 按钮的视觉权重(CSS data attribute,Setup 大、Live 小)

- [ ] **Step 1: 写失败的测试**

`src/components/StatusBar.spec.ts`:

```ts
import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";

import StatusBar from "./StatusBar.vue";
import { useAppState } from "../stores/appState";

function mountSb(
  props: { mode: "setup" | "live" },
  state: Partial<{
    remoteEnabled: boolean;
    controlLocked: boolean;
    presenterUnlocked: boolean;
    selectedWindow: { hwnd: number; title: string } | null;
    activeClientId: string | null;
    presenterOnline: boolean;
  }> = {},
  net: Partial<{ online: boolean }> = {}
) {
  const pinia = createTestingPinia({
    createSpy: vi.fn,
    initialState: {
      appState: {
        remoteEnabled: false,
        controlLocked: false,
        presenterUnlocked: false,
        selectedWindow: null,
        activeClientId: null,
        presenterOnline: false,
        ...state,
      },
      networkState: { online: true, ...net },
    },
    stubActions: true,
  });
  return mount(StatusBar, { props, global: { plugins: [pinia] } });
}

describe("StatusBar — status display", () => {
  it("shows 运行中 when network online", () => {
    const wrapper = mountSb({ mode: "setup" }, {}, { online: true });
    expect(wrapper.text()).toContain("运行中");
  });

  it("shows 未连接 when network offline", () => {
    const wrapper = mountSb({ mode: "setup" }, {}, { online: false });
    expect(wrapper.text()).toContain("未连接");
  });

  it("shows 已选择 when selectedWindow set", () => {
    const wrapper = mountSb({ mode: "setup" }, { selectedWindow: { hwnd: 1, title: "PPT" } });
    expect(wrapper.text()).toContain("已选择");
  });

  it("shows 未选择 when selectedWindow null", () => {
    const wrapper = mountSb({ mode: "setup" }, { selectedWindow: null });
    expect(wrapper.text()).toContain("未选择");
  });

  it("shows activeClientId truncated to 6 chars", () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: "abcdef-123456-extra" });
    expect(wrapper.text()).toContain("abcdef");
  });

  it("shows — for empty activeClientId", () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: null });
    expect(wrapper.text()).toContain("—");
  });
});

describe("StatusBar — remote toggle", () => {
  it("renders 已关闭 when remoteEnabled false", () => {
    const wrapper = mountSb({ mode: "setup" }, { remoteEnabled: false });
    expect(wrapper.find("[data-test='remote-toggle']").text()).toContain("已关闭");
  });

  it("renders 已开启 when remoteEnabled true", () => {
    const wrapper = mountSb({ mode: "setup" }, { remoteEnabled: true });
    expect(wrapper.find("[data-test='remote-toggle']").text()).toContain("已开启");
  });

  it("calls app.setRemoteEnabled(true) when toggling from off", async () => {
    const wrapper = mountSb({ mode: "setup" }, { remoteEnabled: false });
    const app = useAppState();
    await wrapper.find("[data-test='remote-toggle']").trigger("click");
    expect(app.setRemoteEnabled).toHaveBeenCalledWith(true);
  });

  it("calls app.setRemoteEnabled(false) when toggling from on", async () => {
    const wrapper = mountSb({ mode: "setup" }, { remoteEnabled: true });
    const app = useAppState();
    await wrapper.find("[data-test='remote-toggle']").trigger("click");
    expect(app.setRemoteEnabled).toHaveBeenCalledWith(false);
  });

  it("toggle has data-mode='setup' when prop mode=setup", () => {
    const wrapper = mountSb({ mode: "setup" });
    expect(wrapper.find("[data-test='remote-toggle']").attributes("data-mode")).toBe("setup");
  });

  it("toggle has data-mode='live' when prop mode=live", () => {
    const wrapper = mountSb({ mode: "live" });
    expect(wrapper.find("[data-test='remote-toggle']").attributes("data-mode")).toBe("live");
  });
});

describe("StatusBar — release controller", () => {
  it("disables release button when no activeClientId", () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: null });
    expect(wrapper.find("[data-test='release']").attributes("disabled")).toBeDefined();
  });

  it("enables release button when activeClientId is set", () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: "abc" });
    expect(wrapper.find("[data-test='release']").attributes("disabled")).toBeUndefined();
  });

  it("calls app.releaseController() when release clicked", async () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: "abc" });
    const app = useAppState();
    await wrapper.find("[data-test='release']").trigger("click");
    expect(app.releaseController).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: 跑测试,确认失败**

```bash
pnpm test src/components/StatusBar.spec.ts
```

Expected: `FAIL` with `Failed to resolve import "./StatusBar.vue"`。

- [ ] **Step 3: 创建 src/components/StatusBar.vue**

```vue
<script setup lang="ts">
// 控制状态总览 + 远控 toggle + 释放控制权 按钮。
// 从 ControlStatus.vue 和 LocalControlPanel.vue(远控 toggle 部分)
// 合并而来。
//
// mode prop 控制远控 toggle 的视觉权重:
//   setup — toggle 是 Setup 阶段主操作,放大
//   live  — toggle 在 Live 阶段不应该动,缩成小行
import { computed, ref } from "vue";
import { storeToRefs } from "pinia";

import { useAppState } from "../stores/appState";
import { useNetworkState } from "../stores/networkState";

defineProps<{ mode: "setup" | "live" }>();

const app = useAppState();
const net = useNetworkState();
const {
  remoteEnabled,
  controlLocked,
  presenterUnlocked,
  selectedWindow,
  activeClientId,
  presenterOnline,
} = storeToRefs(app);
const { online } = storeToRefs(net);

const canControl = computed(
  () => remoteEnabled.value && !controlLocked.value && presenterUnlocked.value
);

const lastError = ref<string | null>(null);

async function toggleRemote() {
  lastError.value = null;
  try {
    await app.setRemoteEnabled(!remoteEnabled.value);
  } catch (e) {
    lastError.value = String(e);
  }
}
</script>

<template>
  <section class="card">
    <h2>控制状态</h2>
    <ul class="status">
      <li>
        <span class="k">服务</span>
        <span class="v" :class="online ? 'ok' : 'bad'">{{ online ? "运行中" : "未连接" }}</span>
      </li>
      <li>
        <span class="k">PPT 窗口</span>
        <span class="v" :class="selectedWindow ? 'ok' : 'bad'">
          {{ selectedWindow ? "已选择" : "未选择" }}
        </span>
      </li>
      <li>
        <span class="k">远程控制</span>
        <span class="v" :class="remoteEnabled ? 'ok' : 'bad'">
          {{ remoteEnabled ? "允许" : "禁止" }}
        </span>
      </li>
      <li>
        <span class="k">强制锁</span>
        <span class="v" :class="controlLocked ? 'bad' : 'ok'">
          {{ controlLocked ? "锁定" : "释放" }}
        </span>
      </li>
      <li>
        <span class="k">主持人</span>
        <span class="v" :class="canControl ? 'ok' : 'warn'">
          {{ canControl ? "可控制" : "已锁定" }}
        </span>
      </li>
      <li>
        <span class="k">主持人</span>
        <span class="v" :class="presenterOnline ? 'ok' : 'bad'">
          {{ presenterOnline ? "在线" : "离线" }}
        </span>
      </li>
      <li>
        <span class="k">当前控制端</span>
        <span class="v">{{ activeClientId ? activeClientId.slice(0, 6) : "—" }}</span>
      </li>
    </ul>

    <button
      data-test="remote-toggle"
      :data-mode="$props.mode"
      :class="{ on: remoteEnabled }"
      class="remote"
      @click="toggleRemote"
    >
      远程控制:{{ remoteEnabled ? "已开启" : "已关闭" }}
    </button>

    <button
      data-test="release"
      class="release"
      :disabled="!activeClientId"
      @click="app.releaseController()"
    >
      释放控制权
    </button>

    <p v-if="lastError" class="error">{{ lastError }}</p>
  </section>
</template>

<style scoped>
.status {
  list-style: none;
  margin: 0 0 10px;
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
.v.ok {
  color: var(--ink);
}
.v.bad {
  color: var(--danger);
}
.v.warn {
  color: var(--ink-dim);
}

.remote {
  width: 100%;
  font-family: var(--mono);
  letter-spacing: 0.16em;
  text-transform: uppercase;
  font-weight: 600;
  margin-bottom: 8px;
}
.remote[data-mode="setup"] {
  padding: 11px 12px;
  font-size: 11px;
}
.remote[data-mode="live"] {
  padding: 7px 10px;
  font-size: 10px;
  opacity: 0.85;
}
.remote.on {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

.release {
  width: 100%;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px;
}
.release:disabled {
  opacity: 0.35;
  cursor: not-allowed;
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

- [ ] **Step 4: 跑测试,确认通过**

```bash
pnpm test src/components/StatusBar.spec.ts
```

Expected: `Test Files  1 passed (1)`,15 个测试全绿。

- [ ] **Step 5: pnpm build 确认 type check 过**

```bash
pnpm build
```

Expected: 无 type error。

- [ ] **Step 6: Commit**

```bash
git add src/components/StatusBar.vue src/components/StatusBar.spec.ts
git commit -m "feat(director): StatusBar 组件 + 15 个单测

吸收 ControlStatus.vue 的全部内容(7 个状态行 + releaseController 按钮),
加上 LocalControlPanel.vue 里的远控 toggle 按钮。

mode prop ('setup' | 'live') 控制远控 toggle 的视觉权重 — Setup 模式
toggle 是主操作 (padding 11px / font 11px / 全亮);Live 模式 toggle
不应该动 (padding 7px / font 10px / opacity 0.85)。CSS 用
[data-mode] selector,不分支模板。

DirectorView 还没切过来,老的 ControlStatus 暂时还在,两份代码共存。"
```

---

## Task 4: LocalNav.vue 组件

**Files:**
- Create: `src/components/LocalNav.vue`
- Create: `src/components/LocalNav.spec.ts`

**Interfaces:**
- Consumes: `api.localPrev()` / `api.localNext()`(从 `src/utils/api.ts` 直接调,不经过 store)
- Produces: 新组件 `<LocalNav />`,无 props
- Produces: 渲染 "上一页" + "下一页" 两个按钮
- Produces: 按按钮调对应的 IPC,失败显示 error

- [ ] **Step 1: 写失败的测试**

`src/components/LocalNav.spec.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";

import LocalNav from "./LocalNav.vue";
import { api } from "../utils/api";

vi.mock("../utils/api", () => ({
  api: {
    localPrev: vi.fn().mockResolvedValue(undefined),
    localNext: vi.fn().mockResolvedValue(undefined),
  },
}));

function mountLn() {
  return mount(LocalNav, {
    global: { plugins: [createTestingPinia({ createSpy: vi.fn })] },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("LocalNav", () => {
  it("renders 上一页 and 下一页 buttons", () => {
    const wrapper = mountLn();
    expect(wrapper.text()).toContain("上一页");
    expect(wrapper.text()).toContain("下一页");
  });

  it("calls api.localPrev() when 上一页 clicked", async () => {
    const wrapper = mountLn();
    await wrapper.find("[data-test='prev']").trigger("click");
    expect(api.localPrev).toHaveBeenCalled();
  });

  it("calls api.localNext() when 下一页 clicked", async () => {
    const wrapper = mountLn();
    await wrapper.find("[data-test='next']").trigger("click");
    expect(api.localNext).toHaveBeenCalled();
  });

  it("shows error if IPC throws", async () => {
    vi.mocked(api.localNext).mockRejectedValueOnce(new Error("test error"));
    const wrapper = mountLn();
    await wrapper.find("[data-test='next']").trigger("click");
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).toContain("test error");
  });
});
```

- [ ] **Step 2: 跑测试,确认失败**

```bash
pnpm test src/components/LocalNav.spec.ts
```

Expected: `FAIL` with `Failed to resolve import "./LocalNav.vue"`。

- [ ] **Step 3: 创建 src/components/LocalNav.vue**

```vue
<script setup lang="ts">
// 导播本地翻页(从 LocalControlPanel.vue 拆出,改名后只保留翻页)。
// 远控 toggle 已搬去 StatusBar,紧急锁已搬去 EmergencyLock。
import { ref } from "vue";

import { api } from "../utils/api";

const lastError = ref<string | null>(null);

async function run(fn: () => Promise<unknown>) {
  lastError.value = null;
  try {
    await fn();
  } catch (e) {
    lastError.value = String(e);
  }
}

const next = () => run(() => api.localNext());
const prev = () => run(() => api.localPrev());
</script>

<template>
  <section class="card">
    <h2>本地翻页(导播)</h2>

    <div class="grid">
      <button data-test="prev" @click="prev">上一页</button>
      <button data-test="next" @click="next">下一页</button>
    </div>

    <p v-if="lastError" class="error">{{ lastError }}</p>
  </section>
</template>

<style scoped>
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

- [ ] **Step 4: 跑测试,确认通过**

```bash
pnpm test src/components/LocalNav.spec.ts
```

Expected: `Test Files  1 passed (1)`,4 个测试全绿。

- [ ] **Step 5: pnpm build 确认 type check 过**

```bash
pnpm build
```

Expected: 无 type error。

- [ ] **Step 6: Commit**

```bash
git add src/components/LocalNav.vue src/components/LocalNav.spec.ts
git commit -m "feat(director): LocalNav 组件 + 4 个单测

从 LocalControlPanel.vue 里拆翻页按钮的部分。远控 toggle 和紧急锁
已经在 Task 2/3 移到 StatusBar 和 EmergencyLock,LocalControlPanel
剩下的就只是本地翻页 — 改名 LocalNav,职责单一。

DirectorView 还没切过来 (Task 5),老的 LocalControlPanel 还在使用,
两份代码共存。"
```

---

## Task 5: DirectorView 双模式切换 + 删旧组件

**Files:**
- Modify: `src/views/DirectorView.vue`(模板大改 + script 加 mode ref + style 加顶栏样式)
- Create: `src/views/DirectorView.spec.ts`
- Delete: `src/components/ControlStatus.vue`
- Delete: `src/components/LocalControlPanel.vue`

**Interfaces:**
- Consumes: Task 2 的 `EmergencyLock` (props: `{ size: "small" | "large" }`)
- Consumes: Task 3 的 `StatusBar` (props: `{ mode: "setup" | "live" }`)
- Consumes: Task 4 的 `LocalNav`(无 props)
- Produces: 顶部 segmented control `[Setup] [Live]`,本地 ref 持有 mode
- Produces: 左 rail 按 mode 渲染两套 widget 列表
- Produces: 切换 mode 不调用任何 IPC 命令,不动 appState 字段

- [ ] **Step 1: 写失败的测试**

`src/views/DirectorView.spec.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";
import { createRouter, createMemoryHistory } from "vue-router";

import DirectorView from "./DirectorView.vue";
import { useAppState } from "../stores/appState";

// 把所有外部 widget 桩成只渲染 tag name,避免 router/IPC/canvas 等
// 依赖在测试里炸。
vi.mock("../components/WindowPicker.vue", () => ({
  default: { name: "WindowPicker", template: "<div data-test='WindowPicker' />" },
}));
vi.mock("../components/StatusBar.vue", () => ({
  default: {
    name: "StatusBar",
    props: ["mode"],
    template: "<div data-test='StatusBar' :data-mode='mode' />",
  },
}));
vi.mock("../components/EmergencyLock.vue", () => ({
  default: {
    name: "EmergencyLock",
    props: ["size"],
    template: "<div data-test='EmergencyLock' :data-size='size' />",
  },
}));
vi.mock("../components/LocalNav.vue", () => ({
  default: { name: "LocalNav", template: "<div data-test='LocalNav' />" },
}));
vi.mock("../components/DirectorTimer.vue", () => ({
  default: { name: "DirectorTimer", template: "<div data-test='DirectorTimer' />" },
}));
vi.mock("../components/DirectorMessages.vue", () => ({
  default: { name: "DirectorMessages", template: "<div data-test='DirectorMessages' />" },
}));
vi.mock("../components/QrCodePanel.vue", () => ({
  default: { name: "QrCodePanel", template: "<div data-test='QrCodePanel' />" },
}));
vi.mock("../components/DirectorPreview.vue", () => ({
  default: { name: "DirectorPreview", template: "<div data-test='DirectorPreview' />" },
}));
vi.mock("../components/LogPanel.vue", () => ({
  default: { name: "LogPanel", template: "<div data-test='LogPanel' />" },
}));

function mountDv() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/", component: { template: "<div />" } }],
  });
  return mount(DirectorView, {
    global: {
      plugins: [
        createTestingPinia({
          createSpy: vi.fn,
          initialState: {
            appState: {
              armed: false,
              remoteEnabled: false,
              controlLocked: false,
              presenterUnlocked: false,
            },
          },
          stubActions: true,
        }),
        router,
      ],
    },
  });
}

beforeEach(() => {
  localStorage.clear();
});

describe("DirectorView — 顶栏模式切换", () => {
  it("默认在 Setup 模式", () => {
    const wrapper = mountDv();
    const setupBtn = wrapper.find("[data-test='mode-setup']");
    expect(setupBtn.attributes("data-active")).toBe("true");
  });

  it("点 Live 后切到 Live 模式", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find("[data-test='mode-live']").attributes("data-active")).toBe("true");
    expect(wrapper.find("[data-test='mode-setup']").attributes("data-active")).toBe("false");
  });

  it("再点 Setup 切回去", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    await wrapper.find("[data-test='mode-setup']").trigger("click");
    expect(wrapper.find("[data-test='mode-setup']").attributes("data-active")).toBe("true");
  });
});

describe("DirectorView — Setup rail 内容", () => {
  it("Setup 模式渲染 StatusBar / WindowPicker / QrCodePanel / LocalNav / EmergencyLock(小)", () => {
    const wrapper = mountDv();
    expect(wrapper.find("[data-test='StatusBar']").exists()).toBe(true);
    expect(wrapper.find("[data-test='WindowPicker']").exists()).toBe(true);
    expect(wrapper.find("[data-test='QrCodePanel']").exists()).toBe(true);
    expect(wrapper.find("[data-test='LocalNav']").exists()).toBe(true);
    expect(wrapper.find("[data-test='EmergencyLock']").attributes("data-size")).toBe("small");
  });

  it("Setup 模式不渲染 Timer / Messages", () => {
    const wrapper = mountDv();
    expect(wrapper.find("[data-test='DirectorTimer']").exists()).toBe(false);
    expect(wrapper.find("[data-test='DirectorMessages']").exists()).toBe(false);
  });

  it("StatusBar 在 Setup 模式下 mode prop = 'setup'", () => {
    const wrapper = mountDv();
    expect(wrapper.find("[data-test='StatusBar']").attributes("data-mode")).toBe("setup");
  });
});

describe("DirectorView — Live rail 内容", () => {
  it("Live 模式渲染 StatusBar / EmergencyLock(大) / Timer / Messages / LocalNav", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find("[data-test='StatusBar']").exists()).toBe(true);
    expect(wrapper.find("[data-test='EmergencyLock']").attributes("data-size")).toBe("large");
    expect(wrapper.find("[data-test='DirectorTimer']").exists()).toBe(true);
    expect(wrapper.find("[data-test='DirectorMessages']").exists()).toBe(true);
    expect(wrapper.find("[data-test='LocalNav']").exists()).toBe(true);
  });

  it("Live 模式不渲染 WindowPicker / QrCodePanel", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find("[data-test='WindowPicker']").exists()).toBe(false);
    expect(wrapper.find("[data-test='QrCodePanel']").exists()).toBe(false);
  });

  it("StatusBar 在 Live 模式下 mode prop = 'live'", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find("[data-test='StatusBar']").attributes("data-mode")).toBe("live");
  });
});

describe("DirectorView — mode 切换不动业务状态(spec §4)", () => {
  it("切换 mode 三次,setRemoteEnabled / forceLock 都不被调", async () => {
    const wrapper = mountDv();
    const app = useAppState();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    await wrapper.find("[data-test='mode-setup']").trigger("click");
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(app.setRemoteEnabled).not.toHaveBeenCalled();
    expect(app.forceLock).not.toHaveBeenCalled();
    expect(app.releaseController).not.toHaveBeenCalled();
  });
});

describe("DirectorView — mode 不持久化(spec §2.1)", () => {
  it("切到 Live 之后不写 mode 到 localStorage", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    await flushPromises();
    expect(localStorage.getItem("sd.dir.mode")).toBeNull();
  });
});
```

- [ ] **Step 2: 跑测试,确认失败**

```bash
pnpm test src/views/DirectorView.spec.ts
```

Expected: `FAIL` — 测试找不到 `data-test='mode-setup'` 等元素(还没改 DirectorView)。

- [ ] **Step 3: 改 src/views/DirectorView.vue**

完整新文件:

```vue
<script setup lang="ts">
// 导播端主面板:Setup / Live 双模式 + 可拖拽分栏(左栏宽 / 预览·日志高,
// 夹紧并记忆到 localStorage)。
//
// 设计来源:docs/superpowers/specs/2026-06-27-director-gui-setup-live-split-design.md
import { onMounted, onUnmounted, ref } from "vue";

import { useAppState } from "../stores/appState";
import StatusBar from "../components/StatusBar.vue";
import EmergencyLock from "../components/EmergencyLock.vue";
import WindowPicker from "../components/WindowPicker.vue";
import QrCodePanel from "../components/QrCodePanel.vue";
import LocalNav from "../components/LocalNav.vue";
import DirectorTimer from "../components/DirectorTimer.vue";
import LogPanel from "../components/LogPanel.vue";
import DirectorPreview from "../components/DirectorPreview.vue";
import DirectorMessages from "../components/DirectorMessages.vue";

type DirectorMode = "setup" | "live";

const app = useAppState();
let timer: number | undefined;
let inFlight = false;

// ── 模式(冷启动总在 Setup,不持久化到 localStorage)──
const mode = ref<DirectorMode>("setup");

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
    h.removeEventListener("pointercancel", up);
    localStorage.setItem("sd.dir.railW", String(railWidth.value));
  };
  h.addEventListener("pointermove", move);
  h.addEventListener("pointerup", up);
  h.addEventListener("pointercancel", up);
}

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
    h.removeEventListener("pointercancel", up);
    localStorage.setItem("sd.dir.logH", String(logHeight.value));
  };
  h.addEventListener("pointermove", move);
  h.addEventListener("pointerup", up);
  h.addEventListener("pointercancel", up);
}

// ── 轮询状态(原逻辑保留,仅迁移过来)──
async function pull() {
  if (inFlight) return;
  inFlight = true;
  try {
    await app.refresh();
  } finally {
    inFlight = false;
  }
}

onMounted(() => {
  pull();
  timer = window.setInterval(pull, 1500);
});

onUnmounted(() => {
  if (timer !== undefined) window.clearInterval(timer);
});
</script>

<template>
  <div class="dashboard" :style="{ '--rail-w': railWidth + 'px', '--log-h': logHeight + 'px' }">
    <!-- 顶栏 Setup / Live 切换 -->
    <header class="mode-bar">
      <button
        data-test="mode-setup"
        :data-active="mode === 'setup'"
        class="mode-btn"
        @click="mode = 'setup'"
      >
        Setup
      </button>
      <button
        data-test="mode-live"
        :data-active="mode === 'live'"
        class="mode-btn"
        @click="mode = 'live'"
      >
        Live
      </button>
    </header>

    <aside class="rail">
      <!-- Setup 模式 rail -->
      <template v-if="mode === 'setup'">
        <StatusBar mode="setup" />
        <WindowPicker />
        <QrCodePanel />
        <LocalNav />
        <EmergencyLock size="small" />
      </template>

      <!-- Live 模式 rail -->
      <template v-else>
        <StatusBar mode="live" />
        <EmergencyLock size="large" />
        <DirectorTimer />
        <DirectorMessages />
        <LocalNav />
      </template>
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
  grid-template-areas:
    "mode mode mode"
    "rail vsplit main";
  grid-template-columns: var(--rail-w) 12px minmax(0, 1fr);
  grid-template-rows: 36px 1fr;
  gap: 0;
  padding: 16px;
  height: calc(100vh - 38px);
  overflow: hidden;
}

.mode-bar {
  grid-area: mode;
  display: flex;
  gap: 4px;
  justify-content: center;
  align-items: center;
  border-bottom: 1px solid var(--line);
  padding-bottom: 8px;
  margin-bottom: 8px;
}

.mode-btn {
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  font-weight: 600;
  padding: 6px 18px;
  border: 1px solid var(--line);
  background: transparent;
  color: var(--ink-dim);
  cursor: pointer;
}

.mode-btn[data-active="true"] {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

.rail {
  grid-area: rail;
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  overflow-y: auto;
}

.vsplit {
  grid-area: vsplit;
}

.main {
  grid-area: main;
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

.vsplit,
.hsplit {
  position: relative;
  background: var(--line);
  transition: background 0.1s linear;
  touch-action: none;
  -webkit-user-select: none;
  user-select: none;
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
    grid-template-areas:
      "mode"
      "rail"
      "main";
    grid-template-columns: 1fr;
    grid-template-rows: 36px auto auto;
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

- [ ] **Step 4: 删除 ControlStatus.vue 和 LocalControlPanel.vue**

```bash
git rm src/components/ControlStatus.vue src/components/LocalControlPanel.vue
```

- [ ] **Step 5: 跑全套测试,确认通过**

```bash
pnpm test
```

Expected: 所有 `.spec.ts`(Task 1 的 smoke + Task 2 的 EmergencyLock + Task 3 的 StatusBar + Task 4 的 LocalNav + Task 5 的 DirectorView)全绿。DirectorView.spec.ts 含 12 个测试。

- [ ] **Step 6: 跑 pnpm build 确认 type check 过(死引用应该已经全部清完)**

```bash
pnpm build
```

Expected: 无 type error,`dist/` 输出成功。如果有 `Cannot find module '../components/ControlStatus.vue'` 或 LocalControlPanel 类的报错,说明还有死引用,grep 找出来:

```bash
grep -rn "ControlStatus\|LocalControlPanel" src/
```

- [ ] **Step 7: Commit**

```bash
git add src/views/DirectorView.vue src/views/DirectorView.spec.ts
git commit -m "feat(director): Setup/Live 双模式布局 + 删 ControlStatus / LocalControlPanel

设计 spec docs/superpowers/specs/2026-06-27-director-gui-setup-live-split-design.md.

- DirectorView 顶层加 mode ref ('setup' | 'live'),默认 setup,不持久化。
- 顶栏加内联 segmented control [Setup] [Live],切换不发任何 IPC、不动
  appState 业务字段。
- Setup rail 顺序: StatusBar / WindowPicker / QrCodePanel / LocalNav /
  EmergencyLock(小)。
- Live rail 顺序: StatusBar / EmergencyLock(大) / Timer / Messages /
  LocalNav。
- 删 ControlStatus.vue, LocalControlPanel.vue (内容已被 Task 2-4 的
  StatusBar / EmergencyLock / LocalNav 完全吸收)。
- 12 个 DirectorView 单测覆盖: 默认模式、切换 toggle、两套 rail 各自
  内容、mode 切换不调 IPC (spec §4)、mode 不写 localStorage (spec §2.1)。

后端 0 修改, Pinia store 0 修改, 既有 sd.dir.railW / sd.dir.logH
localStorage 保留。"
```

---

## Task 6: 全栈验证 + 真机预演清单

**Files:** 无新增。本任务是验证 + 文档归档。

**Interfaces:** 无新接口产出。验证 Task 1-5 的产出在工作流上端到端走得通。

- [ ] **Step 1: 跑全套 vitest**

```bash
pnpm test
```

Expected: 1(smoke) + 6(EmergencyLock) + 15(StatusBar) + 4(LocalNav) + 12(DirectorView)= 38 个 ts 测试全绿。

- [ ] **Step 2: pnpm typecheck**

```bash
pnpm typecheck
```

Expected: 无 type error。

- [ ] **Step 3: pnpm build**

```bash
pnpm build
```

Expected: 无 error,`dist/` 输出。

- [ ] **Step 4: Rust workspace 单测确认前端改没动到 Rust 边界**

```bash
P=/data/sunyunbo/miniconda3/envs/tauri-dev
PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH \
  cargo test --workspace
```

Expected: 49 个测试全绿,与本 plan 开始前一致(因为本 plan 后端 0 修改)。

- [ ] **Step 5: Windows 跨编 sanity**

```bash
cd src-tauri && XWIN_ACCEPT_LICENSE=1 PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
  cargo xwin build --target x86_64-pc-windows-msvc
```

Expected: `Finished dev profile` 无 error(可有 pre-existing warning)。

- [ ] **Step 6: 死引用 grep**

```bash
cd /data/sunyunbo/www/StageDeckRemote
grep -rn "ControlStatus\|LocalControlPanel" src/ src-tauri/ docs/ \
  | grep -v "specs/" | grep -v "plans/" || echo "clean"
```

Expected: `clean`(spec / plan 文档里允许出现历史引用,代码里不应该)。

- [ ] **Step 7: 写一个 README/CHANGELOG 节点**

Modify `CHANGELOG.md`(在文件顶部追加新一节,在 v0.2.0 上面):

```markdown
## [Unreleased]

- **重构**:导播 GUI 改 Setup/Live 双模式布局。顶栏加 `[Setup] [Live]`
  切换;Setup 模式 rail 装 setup-class widget(选窗 / QR / 测试翻页 /
  小型紧急锁);Live 模式 rail 装 live-class widget(大字计时 / 快捷
  消息 / 巨型紧急锁)。切换是纯前端 UI,不动主持人 ARM/lock/token。
  顺手把 LocalControlPanel 三合一 blob 拆开:远控 toggle → StatusBar,
  紧急锁 → EmergencyLock(带 size prop),本地翻页 → LocalNav(改名)。
  ControlStatus 也并入 StatusBar。新增 vitest + 38 个 Vue 组件单测。
  spec: docs/superpowers/specs/2026-06-27-director-gui-setup-live-split-design.md
```

- [ ] **Step 8: Commit + 标记真机预演条目(spec §5.2 那 6 步)**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): 双模式重构条目 + 预演清单

实现完成。真机预演按 spec §5.2 走一遍:
  1. Setup 阶段: 开 app (默认 Setup) → 选窗 → 看 QR → 主持人扫码 ARM
  2. 切到 Live 模式 → 看 Timer + Messages + 紧急锁巨型按钮就位
  3. Live 阶段: 翻几页, 发条快捷消息, 验证主持人能收到
  4. 故意切回 Setup 模式 → 验证主持人 ARM 状态没掉
  5. 在 Setup 模式按 \"小\" 紧急锁 → 验证按下后主持人立刻断
  6. 切回 Live 模式 → 验证状态保持

验收 (spec §5.3):
  - [ ] 1080p 屏 Live 模式 rail 不出滚动条
  - [ ] 1080p 屏 Setup 模式 rail 不出滚动条
  - [ ] 切换 Setup ↔ Live 三次 appState.armed 不变 (单测已覆盖)
  - [ ] 任一模式下按 EmergencyLock 都能锁
  - [ ] localStorage sd.dir.railW / sd.dir.logH 跨模式记住
  - [ ] 冷启动总在 Setup (单测已覆盖)

真机验收待 Windows 上跑。"
```

---

## Self-Review

**Spec 覆盖回扫**:
- §1.1 现状描述:本 plan 不需要直接实现,但 Task 5 把 6-widget 单 rail 重新组织,问题被消解。✓
- §1.2 现状各组件实际职责:Task 2-4 拆分时严格按这个表 lift-and-shift。✓
- §1.3 根本问题 — 滚动条 + LocalControlPanel 三合一 blob:Task 5 通过分两套 rail 解滚动;Task 2-4 拆 blob。✓
- §2.1 状态机 + 默认 Setup + 不持久化:DirectorView.vue 实现 + DirectorView.spec.ts 第 12 个测覆盖。✓
- §2.2 顶栏 segmented control:DirectorView 模板里实现。✓
- §2.3 Setup rail 顺序:Task 5 模板里硬编。✓
- §2.4 Live rail 顺序:同上。✓
- §2.5 右侧不变:Task 5 模板里 `<main>` 部分整段保留。✓
- §2.6 切换行为 + 不动后端:DirectorView.spec.ts 测 12 覆盖。✓
- §3.1 拆分映射:Task 2-4 严格执行。✓
- §3.2 新组件:Task 2-4 创建。✓
- §3.3 改动 + 删除 + 改名:Task 5 执行(改 DirectorView + 删 ControlStatus + 删 LocalControlPanel,LocalNav 在 Task 4 已创建)。✓
- §3.4 不动列表:Plan 不动这些文件。✓
- §4 状态保持实现保证:DirectorView 用 local ref + 不调 IPC,DirectorView.spec.ts 测 12 覆盖。✓
- §5.1 单元测试:Task 1 引基础设施,Task 2-5 写测试。✓
- §5.2 演出预演清单:Task 6 commit message 里完整列出。✓
- §5.3 验收标准 6 项:单测覆盖 2 项(切换不动状态、冷启动 Setup),真机覆盖 4 项,在 Task 6 commit message 列出。✓
- §6 Non-goals:Plan 严格遵守(不引 mode 持久化、不引 lock-on-setup-mode、不动 SettingsModal、不动右侧、不引新 IPC、不动响应式断点)。✓
- §7 风险:无实现行动,只是 Task 5 写测试时验证组件 unmount/remount 不丢 state(因为 state 在 Pinia store)。✓
- §8 路径索引:全部对得上。✓

**Placeholder 扫描**:
- "TBD/TODO/未定/incomplete" 在 plan 里 0 命中。
- 所有 "测试代码 / 实现代码" 都给了完整 code block,无 `// implement` / `... rest of code` 类省略。

**Type 一致性扫**:
- `mode` 类型 `"setup" | "live"` 在 Task 5 定义,Task 3 StatusBar prop 也用同型,Task 5 测试也用同型。一致。
- `size` 类型 `"small" | "large"` 在 Task 2 EmergencyLock prop 定义,Task 5 调用时使用同型字面量。一致。
- `app.setRemoteEnabled(boolean)` / `app.forceLock(boolean)` / `app.releaseController()` / `api.localNext()` / `api.localPrev()` 签名在 Task 2/3/4/5 引用一致,与现有 `src/utils/api.ts` + `src/stores/appState.ts` 对得上。
- DirectorView 测试里用的 `armed` / `remoteEnabled` / `controlLocked` / `presenterUnlocked` Pinia 字段名与 `src/stores/appState.ts` 对得上(已在 plan 写作期间核对源码)。
