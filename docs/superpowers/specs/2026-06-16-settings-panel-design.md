# 设置面板(Settings Panel)设计

**日期:** 2026-06-16
**状态:** 待审阅
**范围:** 导播端齿轮→模态设置:重置访问码、服务端口(持久化、重启生效)、提示音开关(从消息卡移来)。不动锁/COM/计时/在线。

---

## 1. 目标与决定

导播顶栏一个**齿轮**按钮 → 打开**模态设置**,含三项:**重置 token**(踢所有主持人、出新码)、**服务端口**(持久化、下次启动生效)、**提示音开关**(从「快捷消息」卡移来)。用户已拍板这三项 + 模态形式。

## 2. 入口与外壳

- `App.vue` 顶栏右侧加齿轮按钮;`const settingsOpen = ref(false)` 控制;`<SettingsModal v-if="settingsOpen" @close="settingsOpen=false" />`。
- `SettingsModal.vue`:全屏半透遮罩 + 居中卡;点遮罩或 ✕ 关闭。

## 3. 三项功能

### 3.1 重置访问码(运行时,不持久化)
- IPC `regenerate_token()`:`auth::generate_token()` 生成新 token,写入 `inner.token`,日志。
- 效果:已连主持人下次轮询 `token_ok` 失败 → **401**(前端 `handle401` → `TOKEN✗`、断开);导播下次 `refresh()` 取到新 token → `networkState` 更新 → **二维码自动重绘**。
- 模态:按钮「重置访问码」+ **二次确认**(会踢掉所有已连主持人)。

### 3.2 服务端口(持久化、重启生效)
- IPC `set_server_port(port: u16)`:把端口**存到 `config.json`**(复用 `config.rs` 的 `Config{ server_port, last_selected_window_title }` + `load/save`,路径经 `AppHandle` 解析的 app config dir)。**不改运行中的 `inner.server_port`**(否则二维码会指向当前没在监听的端口)。
- **启动加载**:`lib.rs` setup 里**先加载 config**,把 `inner.server_port = config.server_port` 后**再 spawn web 服务**(`web_server::run` 读 `inner.server_port` 绑定)。这样改完端口、重启即生效。
- 模态:输入框(默认当前运行端口,取自 `DirectorStatus.server_port`)+ 「保存(重启生效)」+ 文案标注"下次启动生效"。端口范围校验 `[1, 65535]`。

### 3.3 提示音开关(从消息卡移来)
- 把 toast 提示音开关从 `DirectorMessages.vue` **移到** `SettingsModal.vue`;同一 `localStorage['sd.dir.toastChime']` 键(ToastView 仍读它)。`DirectorMessages.vue` 删除该开关。

## 4. 启动接线(`lib.rs`)
- setup 顺序调整:**先**用 `AppHandle` 解析 config dir、`config::Config::load(config.json)` → 设 `inner.server_port`、`presets::load` → 设 `inner.presets`,**再** spawn `web_server::run`(读到配置端口)。

## 5. 影响文件

| 文件 | 改动 |
|---|---|
| `src-tauri/src/commands.rs` | `regenerate_token()`、`set_server_port(port)` |
| `src-tauri/src/config.rs` | 启用(被 lib/commands 调用;无需改实现) |
| `src-tauri/src/lib.rs` | setup 先加载 config 端口再 spawn web;注册两个命令 |
| `src/utils/api.ts` | `regenerateToken()`、`setServerPort(port)` |
| `src/stores/appState.ts` | `regenerateToken`/`setServerPort` 操作(调用后 refresh) |
| `src/App.vue` | 顶栏齿轮按钮 + 挂 `SettingsModal` |
| `src/components/SettingsModal.vue` | **新增**:模态设置 |
| `src/components/DirectorMessages.vue` | 删除提示音开关(移到设置) |

## 6. 测试
- **交叉编译 / 原生测试:** 0 错误 / 既有 26 绿。
- **前端构建:** `pnpm build`。
- **运行时(用户):** 齿轮开模态;重置 token → 已连主持人被踢、二维码变、需重扫;改端口保存 → `config.json` 写入、重启后服务/二维码用新端口;提示音开关在设置里生效(消息卡里不再有)。

## 7. 不做(YAGNI)
- 端口**不**做运行时热切换(只重启生效)。
- 设置不含其它项。
- 不动三层锁/COM/计时/在线/捕获。
