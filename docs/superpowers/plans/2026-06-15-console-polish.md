# 导播控制台小修 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实装 PowerPoint 窗口自动识别(手动优先)、提亮发灰的文字对比度、把二维码占位换成真离线生成。

**Architecture:** 三件独立小修。窗口识别用纯函数 `ppt_window_rank`(可单测)+ `EnumWindows` 按类名 `screenClass`>`PPTFrameClass` 选优;优先级语义由前端「仅在未选窗口时启动自动识别」保证。对比度是两份共享 `:root` token 的两行提亮。二维码加 `qrcode` 离线库重写 `utils/qrcode.ts`。详见 `docs/superpowers/specs/2026-06-15-console-polish-design.md`。

**Tech Stack:** Rust + windows-rs 0.58(EnumWindows/GetClassNameW),Vue 3 + Pinia,`qrcode` npm 库,Vite。

**验证约定:**
- **原生测试(纯逻辑,Linux):**
  ```bash
  P=/data/sunyunbo/miniconda3/envs/tauri-dev; \
  PKG_CONFIG_PATH=$P/lib/pkgconfig:$P/share/pkgconfig LD_LIBRARY_PATH=$P/lib PATH=$P/bin:$PATH \
    cargo test --manifest-path /data/sunyunbo/www/StageDeckRemote/src-tauri/Cargo.toml
  ```
- **交叉编译(验证 Win32 FFI):**
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 \
    PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
    cargo xwin build --target x86_64-pc-windows-msvc
  ```
- **前端构建:**
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote && npm run build
  ```

## 文件结构

| 文件 | 职责 | 改动 |
|---|---|---|
| `src-tauri/src/window_manager.rs` | 窗口枚举/识别 | `ppt_window_rank`(纯函数+单测);`find_powerpoint` 实装(windows) |
| `src/views/DirectorView.vue` | 导播主面板 | 挂载首刷后、未选窗口时自动识别一次 |
| `src/components/WindowPicker.vue` | 选窗 UI | 自动识别无结果时提示 |
| `src/App.vue` | 导播全局样式 | `:root` 两个灰阶 token |
| `src/presenter/presenter.css` | 主持人样式 | `:root` 两个灰阶 token(同步) |
| `src/utils/qrcode.ts` | 二维码渲染 | 实装离线生成 |
| `package.json`(+ lockfile) | 依赖 | `qrcode` / `@types/qrcode` |

---

### Task 1: `ppt_window_rank` 纯函数(TDD)

**Files:**
- Modify: `src-tauri/src/window_manager.rs`(在 `is_capturable_window` 后加纯函数;在 `mod tests` 加测试)

- [ ] **Step 1: 写失败测试**

在 `window_manager.rs` 的 `mod tests` 里(`accepts_normal_window` 测试之后)加入:

```rust
    #[test]
    fn ranks_ppt_window_classes() {
        assert_eq!(ppt_window_rank("screenClass"), Some(0)); // 放映窗口最优先
        assert_eq!(ppt_window_rank("PPTFrameClass"), Some(1)); // 编辑主窗口次之
        assert_eq!(ppt_window_rank("Chrome_WidgetWin_1"), None);
        assert_eq!(ppt_window_rank(""), None);
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run(见顶部原生测试命令)。Expected: 编译失败 `cannot find function ppt_window_rank`。

- [ ] **Step 3: 写最小实现**

在 `is_capturable_window` 函数之后(约第 32 行后)加入顶层纯函数:

```rust
/// PowerPoint 窗口按类名定优先级:数字越小越优先;非 PPT 窗口返回 None。
/// 类名按 Win32 约定大小写精确匹配。纯函数,便于单测。
pub fn ppt_window_rank(class: &str) -> Option<u8> {
    match class {
        "screenClass" => Some(0),   // 放映窗口(投影画面)
        "PPTFrameClass" => Some(1), // 编辑主窗口
        _ => None,
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run(原生测试命令)。Expected: 新测试通过,既有测试仍全绿。

- [ ] **Step 5: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/window_manager.rs
git commit -m "feat(window_manager): ppt_window_rank pure fn (screenClass>PPTFrameClass)"
```

---

### Task 2: `find_powerpoint` 实装(windows)

**Files:**
- Modify: `src-tauri/src/window_manager.rs`(`mod imp` 的 import;新增 `window_class`/`PptCandidate`/`find_ppt_proc`;替换 `find_powerpoint` 桩 121-124 行)

- [ ] **Step 1: 补 `GetClassNameW` import**

把 `mod imp` 里 `windows::Win32::UI::WindowsAndMessaging::{...}` 的 use(第 38-41 行)改为加入 `GetClassNameW`:

```rust
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetClientRect, GetWindowLongW, GetWindowTextLengthW,
        GetWindowTextW, IsWindow, IsWindowVisible, SetForegroundWindow, GWL_EXSTYLE, GWL_STYLE,
    };
```

- [ ] **Step 2: 替换 `find_powerpoint` 桩并加辅助**

把 `mod imp` 里现有的 `find_powerpoint`(第 121-124 行):

```rust
    pub fn find_powerpoint() -> Option<TargetWindow> {
        // 本轮保持未实现（自动识别属下一增量）。
        None
    }
```

整体替换为:

```rust
    /// 读取窗口类名。
    fn window_class(hwnd: HWND) -> String {
        unsafe {
            let mut buf = [0u16; 256];
            let n = GetClassNameW(hwnd, &mut buf);
            if n <= 0 {
                return String::new();
            }
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }

    struct PptCandidate {
        rank: u8,
        win: TargetWindow,
    }

    extern "system" fn find_ppt_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let best = &mut *(lparam.0 as *mut Option<PptCandidate>);
            if !IsWindowVisible(hwnd).as_bool() {
                return TRUE;
            }
            // 按类名识别 PowerPoint;放映窗口(screenClass)常无标题,故不套用
            // is_capturable_window(它要求非空标题),只要求有有效客户区矩形。
            let Some(rank) = super::ppt_window_rank(&window_class(hwnd)) else {
                return TRUE;
            };
            if let Some(t) = to_target(hwnd) {
                let better = match best {
                    Some(c) => rank < c.rank,
                    None => true,
                };
                if better {
                    *best = Some(PptCandidate { rank, win: t });
                }
            }
            TRUE
        }
    }

    /// 自动识别 PowerPoint 窗口:优先放映窗口(screenClass),其次编辑主窗口(PPTFrameClass)。
    pub fn find_powerpoint() -> Option<TargetWindow> {
        let mut best: Option<PptCandidate> = None;
        unsafe {
            let _ = EnumWindows(Some(find_ppt_proc), LPARAM(&mut best as *mut _ as isize));
        }
        best.map(|c| c.win)
    }
```

- [ ] **Step 3: 交叉编译 + 原生测试**

Run 交叉编译命令。Expected: `Finished`,0 error。
Run 原生测试命令。Expected: 既有 + Task 1 测试全绿(`find_powerpoint` 在非 windows 仍返回 None,不影响)。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src-tauri/src/window_manager.rs
git commit -m "feat(window_manager): implement find_powerpoint by window class"
```

---

### Task 3: 自动识别接线(启动自动 + 未找到提示)

**Files:**
- Modify: `src/views/DirectorView.vue`(`onMounted`,第 16-19 行)
- Modify: `src/components/WindowPicker.vue`(`autoDetect`,第 28-34 行)

- [ ] **Step 1: 挂载首刷后、未选窗口时自动识别一次**

把 `DirectorView.vue` 的 `onMounted`(第 16-19 行)替换为:

```ts
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
```

- [ ] **Step 2: 自动识别无结果时提示**

把 `WindowPicker.vue` 的 `autoDetect`(第 28-34 行)替换为:

```ts
async function autoDetect() {
  error.value = null;
  try {
    await app.autoDetectPpt();
    if (!selectedWindow.value) error.value = "未找到 PowerPoint 窗口";
  } catch (e) {
    error.value = String(e);
  }
}
```

- [ ] **Step 3: 前端构建**

Run `npm run build`。Expected: vue-tsc 通过、vite 构建成功。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/views/DirectorView.vue src/components/WindowPicker.vue
git commit -m "feat(director): auto-detect PPT on startup when unselected; not-found hint"
```

---

### Task 4: 对比度 token 提亮

**Files:**
- Modify: `src/App.vue`(`:root`,第 22-23 行)
- Modify: `src/presenter/presenter.css`(`:root`,第 13-14 行)

- [ ] **Step 1: 提亮 `App.vue` 的两档灰**

把 `src/App.vue` 中:

```css
  --ink-dim: #8b8b93;
  --ink-faint: #5a5a62;
```

替换为:

```css
  --ink-dim: #a2a2aa;
  --ink-faint: #82828b;
```

- [ ] **Step 2: 同步提亮 `presenter.css` 的两档灰**

把 `src/presenter/presenter.css` 中:

```css
  --ink-dim: #8b8b93;
  --ink-faint: #5a5a62;
```

替换为:

```css
  --ink-dim: #a2a2aa;
  --ink-faint: #82828b;
```

- [ ] **Step 3: 前端构建**

Run `npm run build`。Expected: 构建成功(纯 CSS 值改动,无类型影响)。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/App.vue src/presenter/presenter.css
git commit -m "style: lift --ink-dim/--ink-faint for readable contrast on dark bg"
```

---

### Task 5: 二维码实装(离线 `qrcode`)

**Files:**
- Modify: `package.json`(+ lockfile,经 `npm install`)
- Modify: `src/utils/qrcode.ts`(整文件重写)

- [ ] **Step 1: 安装离线二维码库**

```bash
cd /data/sunyunbo/www/StageDeckRemote
npm install qrcode@^1.5.4
npm install -D @types/qrcode@^1.5.5
```

Expected: `package.json` 多出 `qrcode`(dependencies)与 `@types/qrcode`(devDependencies);lockfile 更新;无报错。

- [ ] **Step 2: 重写 `utils/qrcode.ts`**

把 `src/utils/qrcode.ts` 整个文件替换为:

```ts
// 二维码渲染(spec §6.1.2)。离线:qrcode 为纯 JS,打包进 bundle,场馆无网也可用。
import QRCode from "qrcode";

/// 在 canvas 上离线生成二维码(深近黑模块 + 白底,保证手机可扫)。
export async function renderQrCode(text: string, target: HTMLCanvasElement): Promise<void> {
  if (!text) return;
  await QRCode.toCanvas(target, text, {
    width: target.width, // 吃 canvas 的 160
    margin: 1,
    errorCorrectionLevel: "M",
    color: { dark: "#0a0a0bff", light: "#ffffffff" },
  });
}
```

- [ ] **Step 3: 前端构建**

Run `npm run build`。Expected: vue-tsc 通过(`@types/qrcode` 提供类型)、vite 构建成功并打包 `qrcode`。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add package.json package-lock.json src/utils/qrcode.ts
git commit -m "feat(qrcode): real offline QR via qrcode lib (replaces placeholder)"
```

> 若锁文件名不是 `package-lock.json`(如 `pnpm-lock.yaml`),`git add` 实际存在的那个。

---

## Self-Review

- **Spec 覆盖:** §2.3 `ppt_window_rank`→Task 1;§2.2 `find_powerpoint`→Task 2;§2.4 优先级接线→Task 3;§3 对比度→Task 4;§4 二维码→Task 5。全覆盖。
- **类型一致:** `ppt_window_rank(&str)->Option<u8>` 在 Task 1 定义、Task 2 经 `super::ppt_window_rank` 调用一致;`find_powerpoint()->Option<TargetWindow>` 签名不变(仅换实现),`commands.rs::auto_detect_ppt` 无需改。前端 `app.selectedWindow`/`selectedWindow.value` 为 `TargetWindow|null`,与既有 store 一致。
- **Placeholder:** 无 TBD;锁文件名的小不确定性已给出兜底指引。
- **回归:** token 仅改两值;`renderQrCode` 签名不变,`QrCodePanel.vue` 不动;`find_powerpoint` 非 windows 仍 None。
