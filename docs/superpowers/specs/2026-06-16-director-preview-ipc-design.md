# 导播预览改走 IPC(Director Preview via IPC)设计

**日期:** 2026-06-16
**状态:** 待审阅(bug 修复,根因已定位)
**范围:** 仅导播预览取帧通道。不动主持人端、捕获实现、锁/COM。

---

## 1. 根因(系统化调试)

导播端实时预览一直空白,而主持人端能拿到正确画面 —— 捕获 / `/api/snapshot.jpg` / JPEG 编码都是好的。差别只在取帧的网络上下文:

- **主持人**:`<img src="/api/snapshot.jpg?token=…">` —— **相对路径、同源**(它的页面就由那台 Axum 服务直出)。
- **导播**(`DirectorPreview.vue`):`<img src="http://127.0.0.1:8787/api/snapshot.jpg?…">` —— 从 **Tauri webview**(源 `tauri.localhost`)**跨源去拉环回地址**。WebView2/Chromium 对「非本地源 → 环回地址」的子资源加载有限制(Private Network Access / 环回拦截),`<img>` load 失败 → `frameOk` 永为 false → 一直显示占位。

**架构根因**:导播是 Tauri 宿主、进程内本就握着捕获帧,却绕跨源 HTTP 自取(当初"复用 `/api/snapshot`"的捷径)。主持人必须走 HTTP(远端浏览器),导播不该。

## 2. 修法:进程内 IPC 取帧

导播预览改用 **Tauri IPC** 取帧,彻底消除跨源:

- 新命令 `get_snapshot() -> Result<String, String>`:复用 `capture::snapshot_jpeg(&state, 1280, 720, 70)`(已存在,跨平台:非 Windows 返回占位图),把 `Vec<u8>` 做 **base64** 返回。
- `DirectorPreview.vue` 用 `api.getSnapshot()` **自调度链式取帧**(取一帧→`img.src = "data:image/jpeg;base64," + b64`→排下一帧 ~400ms,慢捕获不堆积),失败则显示占位。
- **移除** HTTP/`token`/`serverPort`/`networkState` 的取帧路径与 `kick`/`watch(token)`/`@load/@error` 链;预览头条带(`get_slide_state` 轮询)**不变**。

好处:无跨源、无 HTTP 往返、URL 不再塞 token;且对"另一可能子因(token 链未启动)"一并免疫。

## 3. 影响文件

| 文件 | 改动 |
|---|---|
| `src-tauri/Cargo.toml` | 加 `base64` 依赖 |
| `src-tauri/src/commands.rs` | `get_snapshot()`(base64 of `snapshot_jpeg`) |
| `src-tauri/src/lib.rs` | 注册 `get_snapshot` |
| `src/utils/api.ts` | `getSnapshot()` |
| `src/components/DirectorPreview.vue` | 取帧改 IPC + data URL(移除 HTTP/token 链) |

## 4. 测试

- **交叉编译:** `cargo xwin build` 0 错误(base64 + 命令)。
- **原生测试:** 既有 23 保持绿。
- **前端构建:** `pnpm build`(api.ts/DirectorPreview typecheck)。
- **运行时(用户 Windows):** 导播中央预览出现实时画面(选中窗口后);未选窗口显示占位;主持人端取帧不受影响。

## 5. 不做(YAGNI / 后续)
- 主持人端 HTTP 取帧**不变**(它是远端浏览器,必须 HTTP)。
- 坐标点击的优雅化(PostMessage 直投 / COM 命中测试混合)是**独立增量**,本次不做。
- 不动锁、COM、捕获实现。
