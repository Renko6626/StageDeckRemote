# 删除 OBS 集成(Remove OBS)设计

**日期:** 2026-06-16
**状态:** 待审阅
**范围:** 删掉占位的 OBS 集成与导播端本地黑屏/恢复按钮。**不动主持人端的 COM 黑屏**(那是 PowerPoint 自己的黑屏,与 OBS 无关,保留)。

---

## 1. 背景与决定

OBS 集成是纯占位:`obs.rs::black()/restore()` 恒返回"未实现",导播端「黑屏/恢复」按钮(`commands.rs::local_black/local_restore` → `obs::*`)**点了就报错、从未可用**。`config.rs`(含 OBS 配置)整个从未被加载,是死代码。画蛇添足,删掉。

用户拍板:**直接去掉导播的「黑屏/恢复」两个按钮**——制作端黑场应由真正的 OBS 软件处理,本应用不假装。主持人端黑屏(`ppt_com::set_black`,PowerPoint 放映黑屏)**保留不变**。

## 2. 改动

- **删** `src-tauri/src/obs.rs`;`lib.rs` 去掉 `mod obs;`。
- `lib.rs`:`invoke_handler` 注销 `commands::local_black`、`commands::local_restore`。
- `commands.rs`:删 `local_black`/`local_restore` 两个命令;`use crate::{input, log, network, obs, window_manager};` 去掉 `obs`。
- `config.rs`:从 `Config` 结构体与 `Default` 摘掉 OBS 字段(`obs_enabled`/`obs_url`/`obs_password`/`ppt_scene_name`/`black_scene_name`),保留 `server_port`/`last_selected_window_title`。(`config.rs` 仍是未加载的死代码,本次不删整文件。)
- `src/components/LocalControlPanel.vue`:删 `black`/`restore` 两个 handler 与模板里「黑屏/恢复」两个按钮(本地控制网格只剩 上一页/下一页);相关注释去掉"黑屏/obs"措辞。
- `src/utils/api.ts`:删 `localBlack`/`localRestore` 两个方法。

## 3. 不动

- 主持人端黑屏/恢复(`routes.rs` `/api/black`·`/api/restore` → `ppt_com::set_black`)。
- 导播本地翻页(`local_next`/`local_prev`,走 SendInput)。
- 三层锁、COM、捕获。

## 4. 测试

- **交叉编译:** `cargo xwin build` 0 错误(确认无悬空 `obs`/命令引用)。
- **原生测试:** 既有 23 保持绿。
- **前端构建:** `pnpm build`(确认 `LocalControlPanel`/`api.ts` 无悬空 `localBlack`/`localRestore`)。
- **运行时(用户):** 导播本地控制只剩 上一页/下一页(无黑屏/恢复);主持人端黑屏照常。

## 5. 不做(YAGNI)
- 不删 `config.rs` 整文件(死代码,后续单独清理)。
- 不接真 OBS(用户用真 OBS 软件)。
- 不动主持人端任何东西。
