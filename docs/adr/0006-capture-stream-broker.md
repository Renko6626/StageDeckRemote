# ADR-0006：截图/推流改"变化驱动 + 单点编码 + 推送"

- 状态：Accepted（2026-06-16）
- 取代：[ADR-0005](0005-mvp-snapshot-polling.md)
- 相关：[architecture.md §4.1/§10](../architecture.md)、[protocol.md §2.4/§2.5](../protocol.md)、
  spec [`superpowers/specs/2026-06-16-capture-stream-optimization-design.md`](../superpowers/specs/2026-06-16-capture-stream-optimization-design.md)

## 背景

ADR-0005 期的 MVP 推流是手机 `<img>` 链式拉 `/api/snapshot.jpg`、每帧独立编码、约 3fps，且
当时**每个消费者(导播预览 + 每个主持人)各自从头跑 BGRA→RGB→缩放→JPEG**——同一帧被编码 N+1 次。
真机表现:CPU 持续偏高、画面卡顿、换帧轻微闪烁、PPT 静止时也在全速烧。
内容画像确认:幻灯片**绝大多数时间是静态的**(只有切页才变)。这套全速重编 + 拉取就完全错配。

## 决策

把链路改成"**变化才编码一次,推给所有消费者**":

1. **WGC 后台会话**把原始 BGRA 帧通过 `Notify` 通知一条**编码线程**(`spawn_blocking`,不阻塞 tokio worker)。
2. 编码线程对原始 BGRA 做**采样 hash**(在线程持锁内,廉价):与上一帧 hash 不同才克隆 8MB 缓冲、
   编码 JPEG (q82)、`publish` 到 **`frame_broker`** —— 一个 `tokio::sync::watch::Sender<Arc<EncodedFrame>>`。
   静止时 hash 一致 → 不编码 → ≈零 CPU。
3. **两条独立的扇出**订阅 broker:
   - 主持人端 `GET /api/stream.mjpg`(`multipart/x-mixed-replace`)直接喂浏览器,
     `<img src="api/stream.mjpg">` 原生处理换帧、空闲零流量。
   - 导播端 `subscribe_preview` IPC 用 **Tauri `Channel<tauri::ipc::Response>`** 推**原始 JPEG 字节**到 webview,
     前端 `createImageBitmap` → `<canvas>` 双缓冲——绕开 webview 拉环回 HTTP 时被 Chromium PNA 拦的老问题。
4. **WGC 不可用**(老系统/虚拟机)→ BitBlt 定时抓取 (500ms) 兜底,喂同一个 broker。
5. `/api/snapshot.jpg` 与 `get_snapshot` IPC **作为兜底保留**(首帧、调试、不支持 MJPEG 的客户端)。

## 影响

**正面**
- 静止时 CPU ≈ 0(WGC 收到帧 → 采样 hash → 一致 → 跳过),与 ADR-0005 的"3fps 全速编码"形成本质差距。
- 切页 → 编码 → broker.publish → 两条扇出**同步通知** → 主持人秒看到、导播秒看到。
- 多消费者(多个主持人 + 导播预览)只编码一次,N+1 → 1。
- 主持人端协议反而**更简单**:从 polling + cache-bust + load/error 重排的整套自愈,
  缩减到一行 `<img src=stream.mjpg>` —— 浏览器原生帮你处理换帧。
- 关键抽象 `frame_broker`(`watch::Sender<Arc<EncodedFrame>>`)的语义直接被 Relay 复用——
  ADR-0007 的 relay 端就是再起一个 watch 复刻这层扇出,导播侧零改动。

**负面 / 代价**
- WGC 是 Windows 专属;非 Windows stub 不能跑真实 capture(原来就是)。
- 引入 `tokio::sync::watch` + `tokio_stream::WatchStream` 依赖;`tauri::ipc::Channel` 也是 Tauri v2 特有 API。
- 编码线程的生命周期需要处理:WGC 自终止(窗口关闭/帧错)时,我们曾让 handler 持有唯一的信号 sender,
  自终止时 channel 自动关闭、编码线程自退——做对了细节就稳,做错了就僵尸线程。spec 与现 ADR 的实现都按此走。
- Tauri Channel 在窗口/页面销毁时 `send` **不会**报错(仅整应用退出才报)——
  靠"每次 `subscribe_preview` 重新调用就 abort 上一个推送任务"(在 `AppState.preview_task` 里保存 join handle)收尾。

## 备选方案

- **保持 ADR-0005 polling**:已知错配静态内容、CPU 浪费。否。
- **WebRTC + H.264 视频轨**:对持续运动视频最优,但对"99% 时间静止的幻灯片"是杀鸡用牛刀,
  且要在 Rust 侧引一整套 WebRTC 栈(`webrtc`/`str0m`)。**留作将来要播视频时的演进路线**,现在不上。
- **WebSocket 推 JPEG 给 `<img>`**:浏览器不能直接把二进制 WS 喂 `<img>`,得 JS 接 ArrayBuffer→Blob URL→`<img>`,
  反而比 MJPEG 多套客户端代码。否。
- **MJPEG 但保持每消费者独立编码**:解决传输模式但不解决"同一帧编码 N+1 次"。半成品。否。

## 实施轨迹

发布于 v0.1.16(2026-06-16)。后续 v0.1.18 把这套 broker 直接复用到 v0.2.0 的 Relay 推流通道——
见 ADR-0007 §3。
