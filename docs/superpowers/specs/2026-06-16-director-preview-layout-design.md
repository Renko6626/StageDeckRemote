# 导播页改版:以实时预览为中心(Director Preview Layout)设计

**日期:** 2026-06-16
**状态:** 待审阅
**范围:** 仅导播前端(Vue)。**不动后端、IPC、数据契约、三层锁。**

---

## 1. 问题与目标

导播页当前是 3 个等宽列 + `align-items:start`,各列内容量不均 → 卡片内横向填不满一半、列高参差、只放日志的第 3 列大片留白;且页面缺一个"大件"。

目标:把版面重排成**以实时捕获预览为中心**的监看控制台(方案 B),填满空白的同时补上导播当前缺的能力——**看到自己正推给主持人的画面**。预览**只读监看**(不做点预览控 PPT,导播仍用本地控制面板)。沿用现有黑白工业视觉。

用户已拍板:**方案 B(左控制栏 | 右主区:预览↑ 日志↓)** + **把「COM 状态」折进预览头**。

## 2. 版面(方案 B)

桌面窗口,顶栏(`App.vue` 现有品牌栏)下方为两列网格,**整体占满视口高度**:

```
┌ topbar (STAGEDECK · DIRECTOR) ───────────────────────────┐
├ 左控制栏(固定宽,过长自滚) ┬ 右主区(flex,占满剩余宽) ──┤
│ PPT 窗口 (WindowPicker)     │ ┌ 预览头条带 ───────────┐ │
│ 控制状态 (ControlStatus)    │ │ ● POWERPOINT 已连接 ·  │ │
│ 本地控制 (LocalControlPanel)│ │   当前页 03/05   LIVE │ │
│ 主持人访问 (QrCodePanel)    │ ├ 实时捕获预览(16:9)──┤ │
│                            │ │   [captured frame]    │ │
│                            │ ├ 日志 (LogPanel) ──────┤ │
│                            │ │   ...                 │ │
└────────────────────────────┴─────────────────────────────┘
```

- **左控制栏**:固定宽约 `340px`,`overflow-y:auto`;顺序 **窗口 → 控制状态 → 本地控制 → 主持人访问**(设置类与控制类自上而下,常用控制居中)。
- **右主区**:flex 列。**预览区 `flex:2`、日志区 `flex:1`(均 `min-height:0`)** → 两者按 2:1 分主区高度、各自填满,不再有参差留白。
- **响应式**:窄窗(`max-width:980px`)整体退化为单列堆叠(预览用固定 16:9 高度)。

## 3. 新组件 `DirectorPreview.vue`

承担"预览头 + 预览图",并**吸收原 `ComStatusPanel` 的状态展示**。

- **预览头条带**:每 ~1500ms 轮询 `api.getSlideState()`(IPC,与原 ComStatusPanel 相同),显示:
  - PowerPoint 连接(`available`)→ `已连接 / 未连接`;
  - 当前页 `current/total`;
  - `LIVE`(`in_show`)/ `IDLE`。
- **预览图**:`<img>` **链式刷新**抓本地快照(复用主持人端管线):
  - `src = http://127.0.0.1:${serverPort}/api/snapshot.jpg?token=${token}&t=${Date.now()}`(`serverPort`/`token` 取自 `networkState`)。
  - `onload` 后 `setTimeout(loadFrame, 300)` 排下一帧;`onerror` 后 `1000ms` 重试。
  - `object-fit:contain`、黑底(横版 PPT 居中留边)。
  - 占位:`token` 为空或快照失败(未选窗口 → 后端 500)时显示「等待捕获 / 未选择 PPT 窗口」遮罩,不刷爆请求。
  - **只读**:不绑定点击控制。
- CSP 为 `null`(已确认),webview 可直接加载本地 HTTP 图片,**无需后端或 CSP 改动**。

## 4. 改动文件

| 文件 | 改动 |
|---|---|
| `src/components/DirectorPreview.vue` | **新增**:预览头(getSlideState 轮询)+ 链式快照 `<img>` + 占位 |
| `src/views/DirectorView.vue` | 重排为方案 B 两列网格;左栏装 4 个面板、右主区装 `DirectorPreview` + `LogPanel`;移除 `ComStatusPanel` |
| `src/components/ComStatusPanel.vue` | **删除**(状态折进预览头) |

> `api.ts` 的 `getSlideState` 保留(预览头复用);`networkState` 已含 `serverPort`+`token`,无需改。后端、`presenter.*`、锁逻辑一律不动。

## 5. 测试

- **前端构建:** `pnpm build`(`vue-tsc --noEmit && vite build`)通过——会类型检查新组件、并报出对 `ComStatusPanel` 的悬空引用(确保已移除)。
- **交叉编译(可选 sanity):** 重新 `pnpm build` 出 `dist` 后 `cargo xwin build` 仍 0 错误(导播前端经 Tauri 在 build 期内嵌)。
- **运行时(用户 Windows):** 选中 PPT 窗口后,预览区出现实时画面、头条带显示连接/页码/LIVE;未选窗口显示占位;版面无大片留白;窄窗堆叠正常。

## 6. 不做(YAGNI)
- 不做"点预览控制 PPT"(只读监看)。
- 不加新后端接口/IPC(复用 `/api/snapshot` + `get_slide_state`)。
- 不改主持人端、锁模型、COM。
- 平板/极窄屏只做基本堆叠退化,不专门优化。
