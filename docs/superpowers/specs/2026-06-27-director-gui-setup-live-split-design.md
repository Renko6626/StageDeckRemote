# 导播 GUI 系统性布局优化:Setup / Live 双模式

**日期**:2026-06-27
**作者**:与项目组通过 brainstorming skill 协作得到
**状态**:Design approved,等待 implementation plan

---

## 1. 背景与问题

### 1.1 现状

导播 GUI(`src/views/DirectorView.vue`)目前是两栏 grid 布局:

```
┌──────────────────────────────────────────┐
│ 左 rail (300-600px 可拉,默认 380px)    │ 右 main          │
│ ────────────                              │                  │
│ WindowPicker                              │ DirectorPreview  │
│ ControlStatus                             │ (上)             │
│ LocalControlPanel                         │ ─── hsplit ───   │
│ DirectorTimer                             │ LogPanel         │
│ DirectorMessages                          │ (下)             │
│ QrCodePanel                               │                  │
│ ↑↓ overflow-y:auto                       │                  │
└──────────────────────────────────────────┘
```

左 rail 是 `flex-direction: column` + `overflow-y: auto`,6 个 widget
依次堆叠。在 1080p 屏 + rail 默认宽度下,会出现纵向滚动条。

### 1.2 现状各组件实际职责

(self-review 阶段读源码核对得到 —— 名字和职责并不总是 1:1)

| 当前 rail 顺序 | 组件实际装的是 | 阶段定位 |
|---|---|---|
| `WindowPicker` | 选 PPT 窗口列表 + 刷新 | Setup-only |
| `ControlStatus` | \textbf{只读}状态显示(服务/窗口/远控/锁/主持人)+ `releaseController` 按钮 | 两边都看 |
| `LocalControlPanel` | \textbf{远控 toggle + 紧急锁按钮 + 本地翻页}(三合一)| 两边都用,但权重不同 |
| `DirectorTimer` | 演出大字计时 + 暂停/重置/预设 | Live-only |
| `DirectorMessages` | 快捷消息网格 | Live-only |
| `QrCodePanel` | LAN/Relay 二维码 + URL + 重置 token | Setup-only |

### 1.3 根本问题

两层问题:

1. **Setup-only 块和 Live-only 块混在同一个滚动列里**。
   WindowPicker 和 QrCodePanel(setup-only)与 DirectorTimer 和
   DirectorMessages(live-only)堆在一起,演出中要找 Timer 得绕过
   WindowPicker。
2. **`LocalControlPanel` 把三件事捏在一起**:远控 toggle 是 setup
   阶段动作、紧急锁是 live 阶段动作、本地翻页是 live 阶段备份。三件
   事的视觉权重在两个阶段完全不同,但目前是一个无法拆分调权重的
   blob。

### 1.3 目标

- 演出当场(Live 阶段)所有高频功能 0.5 秒内可达,不需要滚动条。
- 演出前的设置类功能不抢演出中视觉空间。
- 紧急锁在两种阶段都可达 —— 演出前也可能要按。
- 重新布局\textbf{不动后端状态机和 IPC 接口},纯前端重排。

---

## 2. 设计:Setup / Live 双模式

### 2.1 顶层状态机

`DirectorView.vue` 顶层引入一个 mode ref:

```ts
type DirectorMode = "setup" | "live";
const mode = ref<DirectorMode>("setup");
```

- **默认 `"setup"`** —— 冷启动总在 setup,符合"装好就走一遍设置"的直觉。
- **`mode` 不持久化到 localStorage** —— 故意的。防止"上次关在 live,
  重启忘了重选窗"这种事故。
- 切换 mode 是\textbf{纯 UI 操作,不动任何后端状态}:
  ARM / control_locked / remote_enabled / token / 远控 enabled
  全部保留。这是 Live 模式提供"返回 Setup"逃生通道的安全前提
  —— 切回 Setup 不会踢掉已经扫码 ARM 的主持人。

### 2.2 顶栏

```
┌──────────────────────────────────────────────────┐
│ STAGEDECK · DIRECTOR    [Setup] [Live]        ⚙ │
└──────────────────────────────────────────────────┘
```

- 左:品牌(原样,不动)
- 中:Segmented control 两键,当前 mode 高亮
- 右:齿轮 SettingsModal 触发(原样,不挪)

### 2.3 Setup 模式左 rail(从上到下)

| 位置 | 组件 | 备注 |
|---|---|---|
| 1 | StatusBar | PowerPoint 连接 / ARM 状态 / 远控 toggle(toggle 是 Setup 阶段主操作)/ releaseController 按钮 |
| 2 | WindowPicker | 选 PPT 窗口列表 + 刷新 |
| 3 | QrCodePanel | LAN/Relay 二维码切换 + URL + 重置 token 按钮 |
| 4 | LocalNav | 本地翻页(上一页 / 下一页)—— 演出前测一下走不走得通 |
| 5 | EmergencyLock(小) | 设计上承认 Setup 阶段也可能要按,但不抢眼 |

### 2.4 Live 模式左 rail(从上到下)

| 位置 | 组件 | 备注 |
|---|---|---|
| 1 | StatusBar | 状态条 + 远控 toggle 缩成一行(Live 阶段不应该动远控)/ releaseController 按钮 |
| 2 | EmergencyLock(巨型) | 占满整宽,红色高亮,顶部第二位 —— 永远 0.5s 内可达 |
| 3 | DirectorTimer | 大字计时 + 暂停/重置 |
| 4 | DirectorMessages | 快捷消息网格 |
| 5 | LocalNav | 主持人手机挂了能上的本地翻页备份 |

### 2.5 右侧

完全不变:Preview(上) + hsplit + LogPanel(下)。用户拖过的高度
跨模式记住(已经有 `sd.dir.logH` 这个 localStorage key,沿用)。

### 2.6 模式切换行为

- 点 Setup ↔ Live 立即切。
- 不需要确认 dialog —— 误点的代价就是再点一下回去。
- 切换瞬间 150 ms fade 过渡,避免突兀。
- 切换是\textbf{纯前端},不发任何 IPC 命令,不动 Pinia store 里的
  ARM/lock/token 等业务状态。

---

## 3. 组件拆分与改动

### 3.1 拆分映射(基于 self-review 阶段的源码核对)

\textbf{源 → 目标}的搬迁:

```
ControlStatus.vue (138 行)
  ├─ 只读状态显示(服务/窗口/远控/锁/主持人)   →  StatusBar.vue
  └─ releaseController 按钮                    →  StatusBar.vue

LocalControlPanel.vue
  ├─ 远控 toggle                                →  StatusBar.vue
  ├─ 紧急锁按钮                                 →  EmergencyLock.vue
  └─ 本地翻页(上一页/下一页)                  →  LocalNav.vue (改名,职责单一)
```

### 3.2 新组件

**`src/components/StatusBar.vue`** ←
吸收 `ControlStatus.vue` 全部 + `LocalControlPanel.vue` 的远控 toggle。
装:

- 服务在线指示(绿/红 dot + 文字)
- PPT 窗口选择状态指示
- 远控开关 \textbf{toggle}(响应 `app.setRemoteEnabled`,不只是显示)
- 强制锁状态指示(\textbf{显示}而非操作 —— 操作走 EmergencyLock)
- 主持人可控制 / 已锁定指示
- releaseController 按钮(踢当前主持人)

Props:
```ts
{ mode: "setup" | "live" }
```

CSS 用 `[data-mode="live"]` selector 调字体大小 + 远控 toggle 视觉权重
(Live 下 toggle 缩小,Setup 下放大)。

**`src/components/EmergencyLock.vue`** ←
吸收 `LocalControlPanel.vue` 的紧急锁按钮。

带 `size: "small" | "large"` prop:

- `small`:普通按钮高度,文字 "紧急锁"
- `large`:占满整宽、4-5 rem 高、红色背景、"⚠ 紧急锁 ⚠" 大字

按下后行为不变 —— 调既有 `app.forceLock(true)`(走 IPC `force_lock`)。
按钮文字根据 `controlLocked` 状态切换("紧急锁定" / "已锁定 · 点击释放")。

**`src/components/LocalNav.vue`** ←
`LocalControlPanel.vue` 改名 + 内容收窄,只保留本地翻页(上一页 / 下一页)。
没有 mode prop —— 两种模式下视觉一样。

### 3.3 改动

**`src/views/DirectorView.vue`** —— 主要重写模板:

- 顶层加 `const mode = ref<DirectorMode>("setup")` 状态
- 顶栏加 ModeToggle(直接内联在 DirectorView 模板里,不必单独组件)
- 模板里左 rail 按 `v-if="mode === 'setup'"` / `'live'` 分两套
  widget 顺序
- 把原 `<ControlStatus />` 和 `<LocalControlPanel />` 引用替换成
  `<StatusBar mode="..." />` + `<EmergencyLock size="..." />` + `<LocalNav />`

**`src/components/ControlStatus.vue`** —— \textbf{删除}。内容已被
StatusBar 吸收。

**`src/components/LocalControlPanel.vue`** —— \textbf{改名}为
`LocalNav.vue`,删除远控 toggle 和紧急锁部分,只保留翻页按钮。

### 3.4 不动

- `WindowPicker.vue` / `QrCodePanel.vue` / `DirectorTimer.vue`
  / `DirectorMessages.vue` / `DirectorPreview.vue` / `LogPanel.vue`
  / `SettingsModal.vue` —— 都作为黑盒复用,只是在 rail 里的位置
  和出现条件变了
- `src/stores/appState.ts`(Pinia store)—— 0 修改
- `src-tauri/src/commands.rs` / `routes.rs`(IPC + HTTP)—— 0 修改
- vsplit / hsplit / `sd.dir.railW` / `sd.dir.logH` localStorage —— 保留

---

## 4. 状态保持的实现保证

设计承诺:**切换 mode 不动任何后端状态**。实现保证:

1. `mode` 是 `DirectorView.vue` 的 local `ref`,不出现在 Pinia store
   里。
2. 切换 mode 时不调用任何 Tauri IPC 命令。
3. 模板里 `v-if` 切换的是\textbf{组件挂载/卸载},Pinia store 里的
   `armed` / `control_locked` / `remote_enabled` 等响应式状态不受
   组件挂载影响 —— Vue 的响应式与 DOM 挂载状态正交。
4. 测试用例需要验证:setup → live → setup 切换三次,
   `appState.armed` 数值不变。

---

## 5. 测试

### 5.1 单元测试(Linux 可跑)

引入 `@vue/test-utils`(目前未引,这次顺手加 devDependency)。
不强求引;如果不想引,跳过这一节,演出预演覆盖。

- **`StatusBar.spec.ts`**:不同 mode prop 下 DOM 类名差异;远控 toggle
  点击 emit 正确事件
- **`EmergencyLock.spec.ts`**:`size="small"` vs `"large"` DOM 差异;
  点击 emit 锁事件
- **`DirectorView.spec.ts`**:mode 切换不调用任何 Pinia action,
  appState 切前切后等值

### 5.2 集成测试 / 演出预演

实际跑通靠演出预演:

1. Setup 阶段:开 app(默认 Setup mode)→ 选窗 → 看 QR → 主持人扫码 ARM
2. 切到 Live 模式 → 看 Timer + Messages + 紧急锁巨型按钮就位
3. Live 阶段:翻几页,发条快捷消息,验证主持人能收到
4. 故意切回 Setup 模式 → 验证主持人 ARM 状态没掉
5. 在 Setup 模式按"小"紧急锁 → 验证按下后主持人立刻断,与 Live 模式
   按"大"紧急锁效果一致
6. 切回 Live 模式 → 验证状态保持

### 5.3 验收标准

- [ ] 1080p 屏幕上 Live 模式 rail 不出滚动条(default rail width 下)
- [ ] 1080p 屏幕上 Setup 模式 rail 不出滚动条(default rail width 下)
- [ ] 切换 Setup ↔ Live 三次,appState.armed 不变
- [ ] 任一模式下按 EmergencyLock 都能锁,锁后主持人 API 立刻 403
- [ ] localStorage 的 `sd.dir.railW` 跨模式记住
- [ ] localStorage 的 `sd.dir.logH` 跨模式记住
- [ ] 冷启动总在 Setup 模式

---

## 6. Non-goals(明确不做)

- **不引入 mode 持久化**。冷启动总是 Setup。
- **不引入"Setup 模式锁定后端写"**。即 Setup 模式下\textbf{仍然}
  可以按紧急锁、可以重置 token。组件常驻只是\textbf{视觉权重}
  不同。
- **不动 SettingsModal**。齿轮按钮 + modal 是独立交互层,与 mode
  正交。
- **不动右侧 Preview + Log 布局**。两种 mode 下右侧完全一样。
- **不引入新的 IPC 命令**。后端 0 修改。
- **不做响应式自适应**。当前已有的 `@media (max-width: 980px)`
  断点保留(变单列),新 mode 在小屏下表现按既有 fallback 走。

---

## 7. 风险

| 风险 | 缓解 |
|---|---|
| 切换 mode 时 Vue 把 widget unmount 又 mount,可能丢失内部 state(比如 DirectorMessages 编辑中) | 内部 state 应该提到 Pinia store。审查每个组件是否有 component-local state,必要时上提 |
| EmergencyLock 拆出来后样式回归 —— 原 ControlStatus 里某些视觉设计被打散 | 拆分时把原 ControlStatus 截图存档,新两组件视觉对齐原效果 |
| 演员肌肉记忆失效 —— 习惯了在某位置找 Timer | 不可避免;改动是一次性成本,长期收益更大 |
| Setup 模式下 EmergencyLock 太小,真急用时按不准 | 设计上设为最小 44×44 pt(iOS 触控规范),确保触屏/鼠标都好按 |

---

## 8. 路径相关索引

- 主入口:`src/views/DirectorView.vue`
- 新建:`src/components/StatusBar.vue` / `src/components/EmergencyLock.vue`
- 改名:`src/components/LocalControlPanel.vue` → `src/components/LocalNav.vue`
- 删除:`src/components/ControlStatus.vue`
- 不动:`src/stores/appState.ts`、`src-tauri/src/**`
- spec 路径(本文件):`docs/superpowers/specs/2026-06-27-director-gui-setup-live-split-design.md`
