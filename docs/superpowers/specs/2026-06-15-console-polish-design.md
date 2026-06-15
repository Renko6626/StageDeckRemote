# 导播控制台小修(Console Polish)设计

**日期:** 2026-06-15
**状态:** 待审阅
**范围:** 三件互相独立的导播控制台小修,合于一个 spec(同属导播前端/窗口选择子系统)。

---

## 1. 目标

1. **实装自动识别 + 手动优先** —— 补全当前的空实现 `find_powerpoint()`,让「自动识别 PowerPoint」按钮真正可用,并把「自动 vs 手动」的优先级语义写死。
2. **修对比度** —— 副文字/微标签太灰、对比度不足,提亮两档灰阶 token。
3. **实装二维码** —— 把 `utils/qrcode.ts` 的占位实现换成真离线二维码生成。

非目标(本次不做,见 §6):COM 控制(翻页/宏/黑屏)走 `GetActiveObject` 抓任意活动 PowerPoint、与手动选窗句柄可能分歧的更深问题。

---

## 2. 自动识别 + 手动优先

### 2.1 现状

- `window_manager::find_powerpoint()`(`window_manager.rs:121`)是空实现,恒 `return None` → 「自动识别 PowerPoint」按钮点了无反应。
- 手动挑窗口(`select_window`)正常,写入 `AppState.inner.selected_window` 并启动 WGC 抓取;无任何轮询会重新探测或覆盖它。
- `auto_detect_ppt` 命令(`commands.rs`)已接好:`find_powerpoint()` 返回 `Some` 时写入 `selected_window` 并启动抓取。所以**只差 `find_powerpoint` 的真实实现**。

### 2.2 `find_powerpoint` 实现

枚举顶层窗口(`EnumWindows`),按**窗口类名**识别 PowerPoint:

- `screenClass` —— PowerPoint 放映窗口(投影画面),**最优先**。
- `PPTFrameClass` —— PowerPoint 编辑主窗口,次之。
- 其它 —— 忽略。

返回排名最优的窗口,构造成 `TargetWindow`(句柄/矩形/标题),复用与 `list_windows` 相同的窗口→`TargetWindow` 构造与可抓取过滤逻辑。若都没有 → 返回 `None`。

### 2.3 纯函数 `ppt_window_rank`(可单测)

把「类名→优先级」抽成纯函数,Linux 上单测:

```rust
/// PowerPoint 窗口按类名定优先级:数字越小越优先。非 PPT 窗口返回 None。
pub fn ppt_window_rank(class: &str) -> Option<u8> {
    match class {
        "screenClass" => Some(0),   // 放映窗口
        "PPTFrameClass" => Some(1), // 编辑主窗口
        _ => None,
    }
}
```

类名按 Win32 约定**大小写精确匹配**。`find_powerpoint` 用它对每个候选窗口取 rank,保留 rank 最小者。

### 2.4 优先级语义(写死)

- **手动挑 = 权威。** 一旦 `select_window` 写入,即一直保留,任何自动路径**不得悄悄覆盖**。
- **启动自动一次。** 导播视图挂载后、首次 `refresh()` 完成、且**当前未选窗口**时,前端调用一次 `autoDetectPpt()`。因「未选窗口」才触发,不会盖掉手动选择。
- **按钮 = 显式请求。** 用户点「自动识别」属显式操作,允许(重新)识别并覆盖当前选择。
- 自动识别**未找到**时:不改动现有选择;前端在面板上提示「未找到 PowerPoint 窗口」。

> 说明:`auto_detect_ppt` 命令本身行为不变(找到即写入+启动抓取+日志,返回 `Option<TargetWindow>`)。「不悄悄覆盖手动选择」由前端只在「未选窗口」时触发启动自动来保证;显式按钮可覆盖。

### 2.5 影响

- `src-tauri/src/window_manager.rs`:`ppt_window_rank`(+ 单测)、`find_powerpoint` 真实实现。
- `src/views/DirectorView.vue`:挂载后首刷完成、未选窗口时自动识别一次。
- `src/components/WindowPicker.vue`:`autoDetect` 在返回 `null` 时给出「未找到」提示(小改)。

---

## 3. 对比度

### 3.1 现状

`App.vue` 与 `presenter.css` 各有一份**相同的** `:root` token。两个灰阶在 `--bg:#0a0a0b` 上偏暗:

- `--ink-faint:#5a5a62` —— 用得最多(副标签、`.meta`、`.hint`、`brand-sub`、`.net`),对比度仅 ~2.4:1,明显发灰。
- `--ink-dim:#8b8b93` —— 卡片标题等,~4.7:1,临界。

### 3.2 改动

两份 token **同步**提亮两档(其余不动):

```css
--ink-dim: #a2a2aa;   /* was #8b8b93 — ~7:1,清晰副文 */
--ink-faint: #82828b; /* was #5a5a62 — ~5:1,微标签可读 */
```

`--ink`(#ededee)、`--bg`、`--danger`、边框 `--line`/`--line-hi` 不变 —— 冷淡工业风与黑白基调保留,仅消除「糊成一团」。

### 3.3 影响

- `src/App.vue`(`:root`)
- `src/presenter/presenter.css`(`:root`)

> 两份必须保持一致;改动仅这两行,各两处文件。

---

## 4. 二维码实装

### 4.1 现状

`utils/qrcode.ts` 是占位:在 canvas 上画「二维码占位/(待接入 qrcode 库)」。调用点 `QrCodePanel.vue` 已正确 `await renderQrCode(accessUrl, canvas)`,**无需改动**。

### 4.2 改动

- 加依赖:`qrcode`(纯 JS、离线、打包进 bundle)+ `@types/qrcode`(类型)。
- 重写 `renderQrCode`,签名不变:

```ts
import QRCode from "qrcode";

/// 在 canvas 上离线生成二维码(深近黑模块 + 白底,保证可扫)。
export async function renderQrCode(text: string, target: HTMLCanvasElement): Promise<void> {
  if (!text) return;
  await QRCode.toCanvas(target, text, {
    width: target.width,            // 吃 canvas 的 160
    margin: 1,
    errorCorrectionLevel: "M",
    color: { dark: "#0a0a0bff", light: "#ffffffff" },
  });
}
```

白底深模块是二维码可扫的硬约束(不做反白)。`QrCodePanel.vue` 的 `.qr { background:#fff }` 与之一致。

### 4.3 影响

- `package.json`(+ lockfile):新增 `qrcode` / `@types/qrcode`。
- `src/utils/qrcode.ts`:实装。

> 安装在本机构建时联网完成;运行时离线(已打包),符合场馆无网约束。

---

## 5. 测试

- **Rust 纯逻辑(Linux,tauri-dev 环境):** 新增 `ppt_window_rank` 单测(screenClass→0、PPTFrameClass→1、其它→None);既有单测保持绿。
- **交叉编译(Linux→Windows):** `cargo xwin build` 0 错误,验证 `find_powerpoint`/`EnumWindows`/`GetClassNameW` FFI 编译通过。
- **前端构建:** `npm run build`(vue-tsc + vite)通过,验证 `qrcode` 类型与 token 改动。
- **运行时(用户确认):** 二维码手机可扫;对比度不再糊;开着 PowerPoint 时启动/按钮能自动识别到放映窗口;手动挑后不被自动覆盖。

## 6. 不做(YAGNI / 已知局限)

- **COM 与手动选窗的分歧不在本次范围**:翻页/宏/黑屏经 `ppt_com::connect()` 的 `GetActiveObject` 连「当前活动的 PowerPoint」,与手动选中的窗口句柄是两套;单开一个 PPT 时一致,多开时可能各控各的。本次只做窗口选择/自动识别,留作后续(需按 HWND 反查对应 Presentation/SlideShow COM 对象)。
- 不引入窗口失效校验(选中窗口被关闭后 UI 仍显旧信息);现状 `window_rect` 失败即温和退化,暂可接受。

## 7. 影响文件汇总

| 文件 | 改动 |
|---|---|
| `src-tauri/src/window_manager.rs` | `ppt_window_rank`(+单测)、`find_powerpoint` 实装 |
| `src/views/DirectorView.vue` | 挂载后未选窗口时自动识别一次 |
| `src/components/WindowPicker.vue` | 自动识别未找到时提示 |
| `src/App.vue` | `:root` 两个灰阶 token 提亮 |
| `src/presenter/presenter.css` | `:root` 两个灰阶 token 提亮(同步) |
| `src/utils/qrcode.ts` | 实装离线二维码 |
| `package.json`(+ lockfile) | 新增 `qrcode` / `@types/qrcode` |
