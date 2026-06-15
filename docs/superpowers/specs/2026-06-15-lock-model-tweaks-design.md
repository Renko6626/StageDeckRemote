# 三层锁模型微调(Lock Model Tweaks)设计

**日期:** 2026-06-15
**状态:** 待审阅
**范围:** 调整锁的**默认值**与**主持人侧门控范围**;不改三层锁的层次结构与"导播 > 主持人"的优先级。

---

## 1. 背景

审计发现两个体验问题(详见审计对话):
1. **启动门槛高**:默认三层全锁(`remote_enabled=false` + `control_locked=true` + `presenter_unlocked=false`),导播每次开场要拨两个语义重叠的开关(开远程 **且** 解强锁)主持人才有翻页。
2. **主持人"锁定"锁不住翻页/黑屏(反直觉)**:主持人 ARM 只门控「宏 + 坐标点击」(`click_allowed`),翻页/黑屏只看 `button_allowed`,所以主持人即使"锁定"自己,仍能翻页/黑屏。

用户已拍板修复方向:**① 默认不强锁;② ARM 管住主持人侧所有操作。**

## 2. 新锁模型

层次与优先级**不变**(导播严格 > 主持人;导播关远程或上强锁会清掉 `presenter_unlocked` 且主持人无法自行解锁)。仅改两处:

| 项 | 旧 | 新 |
|---|---|---|
| `control_locked` 默认 | `true`(开机即强锁) | **`false`**(强锁释放,仅作紧急按钮) |
| 主持人翻页/黑屏的门控 | `button_allowed`(不看 ARM) | **`click_allowed`**(需 ARM) |

新语义:
- **导播「远程控制」(默认关)= 日常总闸**。开启后 `button_allowed` 为真。
- **导播「强制锁」(默认释放)= 紧急按钮**。一键 `control_locked=true` 立即切断一切并清掉主持人解锁。
- **主持人「ARM 释放控制」= 主持人侧唯一总闸**:ARM 后翻页/黑屏/宏/坐标点击**全部**可用;未 ARM 则全部禁用(「锁就全锁」)。
- **启动只需导播开「远程控制」一步**;随后主持人 ARM 即可全功能。

> 这取代了早期架构里"默认强制锁 + 翻页只需 `button_allowed`"的取舍。`button_allowed` 此后**仅**用于 `/api/unlock` 的前置条件(导播必须先放行,主持人才能 ARM);所有主持人**动作**端点(翻页/黑屏/恢复/宏/坐标点击)统一走 `click_allowed`。

## 3. 改动点

### 3.1 `src-tauri/src/app_state.rs`
- `Inner` 默认 `control_locked: false`(并改注释为:默认远程关闭=总闸、强制锁释放=仅紧急、未解锁)。
- `button_allowed` / `click_allowed` 方法**逻辑不变**(仍是两层 / 三层布尔与)。
- 新增单测:构造 `AppState::new`,断言默认 `control_locked==false`;并覆盖门控状态机(remote 关→全禁;remote 开未 ARM→`button_allowed` 真但 `click_allowed` 假;ARM→`click_allowed` 真;强锁→全禁)。

### 3.2 `src-tauri/src/routes.rs`
- `page_turn`(`/api/next`、`/api/prev`)、`black`(`/api/black`)、`restore`(`/api/restore`)的门控由 `button_allowed()` 改为 **`click_allowed()`**(失败仍返回 `FORBIDDEN / "control_locked"`,与 `action`/`click` 一致)。
- `/api/unlock` **保持** `button_allowed()`(导播放行才允许主持人 ARM)。`/api/action`、`/api/click` 已是 `click_allowed`,不变。

### 3.3 `src/presenter/presenter.js`
- 底部翻页坞按钮门控由 `!canTransport()` 改为 **`!canAct()`**(与宏按钮一致 → 未 ARM 时翻页坞同样禁用)。
- 移除已无引用的 `canTransport()`。其余(轮询/锁态文案/遮罩/快照链)不变。

### 3.4 `src/components/LocalControlPanel.vue`(小)
- 强制锁按钮文案体现"紧急"语义:未锁时 `紧急锁定`、已锁时 `已紧急锁定(点击释放)`(传达它是紧急按钮而非日常开关)。`远程控制` 文案不变(它是日常总闸)。

## 4. 测试

- **Rust(Linux,tauri-dev):** 新增 `app_state` 门控单测(见 3.1);既有 18 测试保持绿 → 共 19。
- **交叉编译:** `cargo xwin build` 0 错误。
- **前端构建:** `npm run build` 通过。
- **运行时(用户 Windows + 手机):**
  - 导播仅开「远程控制」一步 → 主持人 ARM 按钮可用;ARM 后翻页/黑屏/宏全可用。
  - 主持人**未 ARM 时翻页坞与宏全部禁用**(锁就全锁)。
  - 导播点「紧急锁定」→ 主持人侧瞬间全断、解锁被清,需导播释放后重新 ARM。

## 5. 不做(YAGNI / 已知局限)
- 不合并两个导播开关(保留独立的「远程控制」总闸 + 「紧急锁定」按钮)。
- 不改"导播切换时清掉主持人解锁"的现有行为(用户未选此项)。
- 不改 COM 与手动选窗的分歧(沿用既有约定)。

## 6. 影响文件
- `src-tauri/src/app_state.rs` — 默认值 + 注释 + 单测
- `src-tauri/src/routes.rs` — 翻页/黑屏/恢复门控 → `click_allowed`
- `src/presenter/presenter.js` — 翻页坞门控 → `canAct`;删 `canTransport`
- `src/components/LocalControlPanel.vue` — 强制锁按钮文案(紧急语义)
