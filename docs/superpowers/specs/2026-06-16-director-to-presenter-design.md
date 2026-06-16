# 导播→主持人通知(Director→Presenter)设计

**日期:** 2026-06-16
**状态:** 待审阅
**范围:** 把"导播→主持人"通知通道实装(此前 quick-messages 只做了主→导,留了 `from` 钩子)。复用同一预制池。不动锁/COM/toast 窗。

---

## 1. 目标与决定

让导播从**与主持人共享的同一预制池**里点一条,发到主持人手机上,以**顶部横幅**显示。两个方向就此对称(主→导:置顶软弹窗 6 秒;导→主:主持人页内横幅 ~10 秒)。

用户已拍板:**两边共享同一个预制池**;**主持人端顶部横幅、~10 秒自消 + 可手动关**。

## 2. 共享预制池

现有 `Inner.presets`(导播可编辑、持久化 `presets.json`)即共享池,**两个方向都从这里取**:
- 主持人 MSG 页点预制 → `POST /api/message {id}` → 发给导播(已有,不变)。
- 导播「快捷消息」卡点预制 → `send_to_presenter(id)` → 发给主持人(新)。

## 3. 导播发送(IPC)

- `Inner` 新增 `director_message: Option<DirectorMessage>`,其中 `DirectorMessage { seq: u64, text: String, at_ms: u64 }`。`seq` 单调自增,供主持人检测新消息。**不持久化**(临时通知,重启清空)。
- IPC `send_to_presenter(id: String)`:校验 `id ∈ presets`(取服务端当前文本,导播也只发池中条目);命中则 `seq = 旧 seq + 1`,置 `director_message = Some({seq, text, at_ms})`,`log::event("已通知主持人：{text}")`。id 非法则不改动(命令静默返回;前端按池内 id 调用,不会非法)。

## 4. 送达主持人(折进 status,无需新端点/推送)

- `GET /api/status`(routes.rs,主持人本就每 ~900ms 轮询)的 JSON 新增 `director_message` 字段(`Option<DirectorMessage>` 序列化为 `null` 或 `{seq,text,at_ms}`)。
- **导播端 IPC `get_status`(commands.rs)不变**(那是导播看自己的状态,不含本字段)。本字段只进 HTTP `/api/status`。

## 5. 主持人横幅(presenter)

- `presenter.html` 在 `.frame` 顶部加一个隐藏横幅元素(覆盖于状态条之上/页面之上,任意页可见)。
- `presenter.js` 在状态轮询里读 `s.director_message`,维护 `lastDirSeq`:
  - **首帧只记基线**:首次拿到 status 时 `lastDirSeq = director_message?.seq ?? 0`,**不弹**(避免一连上就回放最后一条旧消息)。
  - 之后:若 `director_message && director_message.seq > lastDirSeq` → 显示横幅(文本)、更新 `lastDirSeq = seq`、起 **10 秒**自消定时;`✕` 立即关。
- 横幅样式:顶部、醒目(ink/danger 强调)、大字文本 + 「导播 · HH:MM」+ ✕;沿用黑白工业风。横幅是**只读通知**,不阻塞下方控制(可点 ✕ 关,或 10 秒自消)。

## 6. 导播 UI(DirectorMessages.vue)

- 非编辑态的预制列表项改为**可点按钮**:点一下 = `send_to_presenter(p.id)`,随后弹瞬时「已发送给主持人 ✓」。
- 编辑态、提示音开关**不变**。

## 7. 影响文件

| 文件 | 改动 |
|---|---|
| `src-tauri/src/app_state.rs` | `DirectorMessage` 类型;`Inner.director_message` |
| `src-tauri/src/commands.rs` | `send_to_presenter(id)` |
| `src-tauri/src/lib.rs` | `invoke_handler` 注册 `send_to_presenter` |
| `src-tauri/src/routes.rs` | `/api/status` 加 `director_message` 字段 |
| `src/utils/api.ts` | `sendToPresenter(id)` |
| `src/components/DirectorMessages.vue` | 预制项可点发送 + 「已发送」反馈 |
| `src/presenter/presenter.{html,css,js}` | 顶部横幅 + 在 status 轮询里处理 `director_message` |

## 8. 测试

- **交叉编译:** `cargo xwin build` 0 错误。
- **原生测试:** 既有 23 保持绿(本期不新增纯逻辑函数;`send_to_presenter` 为命令,逻辑由交叉编译 + 运行时验证)。
- **前端构建:** `pnpm build`(api.ts/DirectorMessages typecheck)。
- **运行时(用户 Windows + 手机):** 导播点某预制 → 主持人手机顶部出现横幅、~10 秒自消、✕ 可关、任意页可见;主持人刚连上不回放旧消息;主→导方向不受影响。

## 9. 不做(YAGNI)
- 不做自由文本(共享预制池即够);不做导播侧单独预制。
- 不做已读回执(单向通知,与主→导一致)。
- `director_message` 不持久化(临时通知)。
- 不动三层锁、COM、toast 窗、主→导既有逻辑。
