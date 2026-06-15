# 导播页可拖拽分栏(Director Resizable Panes)设计

**日期:** 2026-06-16
**状态:** 待审阅
**范围:** 仅导播前端 —— `DirectorView.vue` + `LogPanel.vue`。后端/IPC/其它组件/锁不动。

---

## 1. 问题与目标

新版导播页(方案 B)有三处比例不顺:左栏太细(340px)、预览框过高、日志区太小;且左栏默认就要竖向滚动。

目标:让用户**拖动**调整 **① 左栏宽度** 和 **② 预览/日志高度比**,拖动即调、记忆到本地;并给更合理的默认。左栏竖向滚动(高度问题)保留为干净滚动条,本次不压缩面板(见 §6)。

用户已拍板:**可拖拽分隔条 + 更好默认**。

## 2. 机制

### 2.1 两根拖条
- **竖向拖条(`.vsplit`,`col-resize`,宽 6px)**:位于左栏与右主区之间,拖动调 `railWidth`。
- **横向拖条(`.hsplit`,`row-resize`,高 6px)**:位于预览与日志之间,拖动调 `logHeight`。

### 2.2 状态与尺寸驱动(CSS 变量)
`DirectorView` 持有两个 `ref`:`railWidth`(默认 **380**)、`logHeight`(默认 **300**),通过**内联 CSS 变量**驱动样式(便于被媒体查询覆盖):

- `.dashboard :style="{ '--rail-w': railWidth+'px', '--log-h': logHeight+'px' }"`
- `.dashboard { grid-template-columns: var(--rail-w) 6px minmax(0,1fr); }`(三轨:左栏 / 竖拖条 / 主区)
- 主区为 flex 列:预览 `flex:1 1 0; min-height:0` → `.hsplit`(`flex:0 0 6px`)→ 日志 `height: var(--log-h); flex:0 0 auto; min-height:0`。

### 2.3 拖动(Pointer 事件)
`pointerdown` 时 `setPointerCapture`,记录起点与起始尺寸,`pointermove` 实时更新、夹紧,`pointerup` 释放并持久化:
- 栏宽:`railWidth = clamp(startW + (e.clientX - startX), 300, 600)`。
- 日志高:`logHeight = clamp(startH + (startY - e.clientY), 160, mainH - 200)`,其中 `mainH` 为 `.hsplit` 所在 `.main` 的 `clientHeight`(保证预览 ≥ 200px、日志 ≥ 160px)。

### 2.4 记忆
`pointerup` 后写 `localStorage`(键 `sd.dir.railW` / `sd.dir.logH`)。`setup` 时读取并**夹紧**(栏宽 `[300,600]`、日志高 `[160,800]`)作为初值;读不到或非法用默认。

### 2.5 响应式
窄窗(`@media max-width:980px`):`.dashboard` 改 `grid-template-columns:1fr`、`height:auto`;`.vsplit`/`.hsplit` `display:none`;预览回退固定 `height:56.25vw`(16:9);日志回退 `height:280px`。(媒体查询能覆盖 `var(--log-h)`,因尺寸由样式表规则而非内联 height 决定。)

## 3. `LogPanel.vue` 改动(让日志填满)

当前 `.log ul { max-height:220px }` 写死,日志区拖高后只会多出空白。改为**填满容器**:
- `.log` 设为 flex 列(`height:100%; display:flex; flex-direction:column; min-height:0; box-sizing:border-box;`)。
- `.log h2` `flex:0 0 auto`。
- `.log ul` `flex:1 1 auto; min-height:0; max-height:none; overflow:auto;`(去掉 220 上限,改为吃满剩余高度并自身滚动)。

这样日志卡片填满 `--log-h` 给的高度,拖高即看到更多日志行。其余样式不变。

## 4. 影响文件
| 文件 | 改动 |
|---|---|
| `src/views/DirectorView.vue` | 两根拖条 + Pointer 拖动 + 夹紧 + localStorage 记忆 + CSS 变量驱动栏宽/日志高;默认 380/300;媒体查询 |
| `src/components/LogPanel.vue` | 日志列表改为填满容器(去 220 上限) |

> `DirectorPreview` 不改(它本就 `flex:1` 吃满预览区)。后端/`api.ts`/`networkState`/锁/主持人端不动。

## 5. 测试
- **前端构建:** `pnpm build`(`vue-tsc --noEmit && vite build`)通过。
- **运行时(用户):** 拖竖条改栏宽、拖横条改预览/日志比;刷新后保持;夹紧生效(拖到极限预览/日志不消失);窄窗回退单列、无拖条。

## 6. 不做(YAGNI)
- 不压缩左栏各面板内边距以消除竖向滚动(会牵动多组件;左栏保留有上界的干净滚动条,需要再单独一轮"面板紧凑化")。
- 不引入拖拽库(纯手写 Pointer)。
- 不动后端、锁、主持人端、其它组件。
