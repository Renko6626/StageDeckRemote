# 在线状态 + 单控制端 + 断线提醒(Presence / Single Controller)设计

**日期:** 2026-06-16
**状态:** 待审阅
**范围:** 唯一 cid + 在线追踪 + 先到为主单控制端(非控制端只读)+ 导播可释放 + 断线置顶 toast。不动三层锁本身/COM/计时。

---

## 1. 目标与决定

- 每台手机有**唯一 id(cid)**;服务端据此做**先到为主的单控制端**:第一个连上的为控制端,其余**只读**(能看,但控制按钮禁用)。
- **导播可释放/收回**控制权;`ControlStatus` 显示主持人在线/离线 + 当前控制端。
- 控制端**断线**时,**复用置顶 toast 弹「主持人已断开」** + 导播 GUI 红指示 + 日志,并释放控制槽(让备用手机接管)。

## 2. 在线模型(`app_state.rs`,极简:只追控制端)

不存全量 client 表;只追"当前控制端 + 其心跳":
- `Inner.active_client_id: Option<String>`(已有)= 当前控制端 cid。
- 新增 `Inner.active_seen_ms: u64`(默认 0)= 控制端最后心跳(服务端时钟)。
- `pub const PRESENCE_TIMEOUT_MS: u64 = 4000;`(>4s 无心跳视为离线)。

纯/小函数:
```rust
pub fn client_online(last_seen_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_seen_ms) <= PRESENCE_TIMEOUT_MS
}

/// 记录 cid 心跳并按"先到为主 / 控制端离线可被接管"更新控制端;返回该 cid 是否为控制端。
pub fn note_presence_and_claim(inner: &mut Inner, cid: &str, now_ms: u64) -> bool {
    let controller_live = inner.active_client_id.is_some() && client_online(inner.active_seen_ms, now_ms);
    let is_me = inner.active_client_id.as_deref() == Some(cid);
    if is_me || !controller_live {
        inner.active_client_id = Some(cid.to_string());
        inner.active_seen_ms = now_ms;
        true
    } else {
        false // 另一台在线控制中 → 只读
    }
}

/// 控制端当前是否在线。
pub fn controller_online(inner: &Inner, now_ms: u64) -> bool {
    inner.active_client_id.is_some() && client_online(inner.active_seen_ms, now_ms)
}
```
单测覆盖:无控制端→认领、同 cid→保持、他人在线→只读、他人离线→接管、`client_online` 边界。

## 3. HTTP 改动(`routes.rs`)

- **`GET /api/status`** 加可选 `cid` query。若带 cid:在持锁作用域内 `note_presence_and_claim(&mut i, cid, now)` 得 `is_controller`;status JSON 增 `"is_controller"`(bool)与 `"active_client_id"`。无 cid 则 `is_controller=false`、不认领。
- **控制类接口加"控制端"校验**(在三层锁之外):`action`、`click`、`next`/`prev`(`page_turn`)、`black`、`restore`、`lock`、`unlock` —— 要求请求体 `client_id == active_client_id`,否则 `403 "not_controller"`。(请求体已带 `client_id`,改为带真实 cid。)
- **读类不限制**:`status`、`snapshot.jpg`、`slide_state`、`messages`(GET)、`message`(POST,通信)对任意已认证客户端开放——**非控制端仍能看画面、能给导播发消息**。
- `message` 处理器把 `from: "presenter"` 改为 `from: "主持人"`(配合 toast 头显示 from)。

## 4. 导播侧(`commands.rs` / `get_status`)

- `DirectorStatus` 增 `presenter_online: bool`(= `controller_online(&i, now)`)。`active_client_id` 已有。
- 新命令 `release_controller()`:`active_client_id = None` + 日志「导播释放了控制权」。

## 5. 断线提醒(`lib.rs` watcher 任务)

- setup 里 spawn 一个 tokio interval(~1s)任务,持 `state.clone()`:每拍算 `online = controller_online(&i, now)`;维护本地 `was_online`。**`was_online && !online` 跳变** → ① `active_client_id=None`(释放槽,放锁后);② `log::event("主持人已断开连接")`;③ 经 `message_tx.send(HostMessage{ from:"系统", text:"主持人已断开连接", at_ms })` → 现有接收任务弹**置顶 toast**。更新 `was_online`。
- 持锁只读/改字段、不跨 `.await`;`log`/`send` 在放锁后。

## 6. 前端

- **`ToastView.vue`**:头部 `● {{ from }} · {{ time }}`(读 payload.from;主持人消息=主持人、断线=系统)。
- **`presenter.js`**:生成并存 `localStorage['sd.presenter.cid']`(随机 hex);`api()` POST 体 `client_id` 用该 cid;status 轮询 query 加 `&cid=`。state 增 `isController`(读 `s.is_controller`);`canAct`/`canTransport` 增 `&& isController`。非控制端显示**「只读 · 另一台正在控制」**条并禁控制;**消息仍可发**。
- **`presenter.html`/`css`**:只读提示条元素 + 样式。
- **`ControlStatus.vue`**:显示「主持人:在线/离线」(`presenter_online`,离线红)与当前控制端;加「释放控制权」按钮(`active_client_id` 非空才可点)→ `releaseController`。
- **`api.ts`/`appState.ts`**:`DirectorStatus.presenter_online`、`releaseController`、`presenterOnline` ref + 操作。

## 7. 影响文件
`app_state.rs`、`routes.rs`、`commands.rs`、`lib.rs`、`src/views/ToastView.vue`、`src/utils/api.ts`、`src/stores/appState.ts`、`src/components/ControlStatus.vue`、`src/presenter/presenter.{html,css,js}`。约 11 个。

## 8. 测试
- **Rust 纯逻辑:** `client_online`/`note_presence_and_claim` 单测;既有 24 保持绿。
- **交叉编译 / 前端构建:** 0 错误 / 成功。
- **运行时(用户 + 两台设备):** 第一台控制、第二台只读(控制禁用、能看能发消息);导播「释放控制权」后下一台接管;关掉控制端手机 → 导播弹「已断开」toast + 状态转红 + 日志,备用手机接管。

## 9. 不做(YAGNI)
- 不做全量在线 client 列表/多控制端排队(只追当前控制端)。
- 不做导播"指派给特定手机"(释放→先到接管即可)。
- 不持久化;不动三层锁判定、COM、计时、捕获。
