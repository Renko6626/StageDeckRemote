# 截图 / 推流优化设计(方案 A:变化才编码 + 推送)

**日期:** 2026-06-16
**状态:** 已批准(待写实现计划)
**范围:** 把"逐帧请求 + 每消费者重复编码"的截图链路,改造为"一处编码、变化才推、两端推送"的帧管线。覆盖导播预览 + 主持人推流两边。不动锁/鉴权模型,不引入视频编解码。

---

## 1. 背景与痛点

当前管线:
- **捕获(已对)**:`capture_session.rs` 用 WGC 后台会话把最新一帧紧排 BGRA 写入 `Arc<Mutex<Option<CapturedFrame>>>`。
- **编码(糙)**:每个消费者各自从头编码同一帧 —— 导播 `get_snapshot`(IPC ~400ms)+ 每个主持人 `/api/snapshot.jpg`(~300ms)都独立跑 `bgra_to_rgb`(纯量逐像素)→缩放→JPEG。同一帧被编码 N+1 次。
- **传输(糙)**:逐帧请求/响应(`<img>` load/error 链 + `?t=` 防缓存),非推流 → HTTP 往返、抖动、~2–3fps。
- **质量/能耗**:固定 1280×720 q70;PPT 大多数时间静止却仍全速重编,费 CPU 且不流畅。

用户确认:内容**基本是静态幻灯片**(切页才变);主要痛点是**卡顿 / CPU 高 / 切换闪烁**(清晰度不是首要)。结论:用"变化才推一帧"的事件驱动管线最契合 —— 静止≈零开销,切页瞬间更新。

## 2. 目标 / 非目标

**目标**
- 单点编码:每个"变化后的帧"只编码一次,扇出给所有消费者。
- 推送代替轮询:主持人 MJPEG 流;导播 Tauri Channel 推送。
- 静止时≈零 CPU;切页秒更;换帧无闪烁。
- 顺手把 JPEG 质量 q70 → q82(CPU 已释放,白送)。

**非目标(YAGNI)**
- 不上 H.264 / WebRTC / 视频编解码;不做音频;不做自适应码率协商。
- 不动三层锁 / token 鉴权(流仍是 token-gated 只读,与现 `snapshot` 一致)。
- 保留 `/api/snapshot.jpg` 与 `get_snapshot` IPC 作兜底(去风险)。

## 3. 架构 — 帧 broker(单点编码 + watch 扇出)

新增帧 broker(独立 `frame_broker.rs`;由 `CaptureManager` 持有),对外是一个 **`tokio::sync::watch::Sender<Arc<EncodedFrame>>`**。

```rust
pub struct EncodedFrame {
    pub seq: u64,        // 单调递增,消费者据此判新
    pub jpeg: Vec<u8>,   // 已编码 JPEG 字节
    pub width: u32,
    pub height: u32,
}
```

选 `watch` 而非 `broadcast`:它只保留最新值,慢消费者自动丢中间帧、只读最新 —— 正是"看最新帧"的视频语义,永不积压、无 `Lagged` 错误。新订阅者立即拿到当前帧。

**数据流:**
```
帧来源 ──notify──▶ 编码任务(spawn_blocking 循环)
                      │ ① 变化检测:对原始 BGRA 做快速非加密 hash,与上帧比
                      │ ② 变了才编码一次:bgra_to_rgb → scaled_dims → JPEG(q82)
                      │ ③ 可选限频:即便连续变化,编码不超过 ~15fps(防偶发视频打满)
                      ▼
               watch.send(Arc<EncodedFrame{seq+1, ...}>)
                 ├──▶ 主持人 MJPEG 写出(每连接一份订阅)
                 └──▶ 导播 Channel 推送(订阅 → emit 原始字节)
```

- 编码在 `spawn_blocking`(JPEG 是 CPU 密集;与上轮 #3 一致,不阻塞 async worker,也不在 WGC 回调线程里编码)。
- **变化检测**:对紧排 BGRA 缓冲做一次快速 hash(如 `seahash`/`xxhash`,或采样校验和),与上一帧 hash 比较;相等则跳过编码。整帧 hash(~8MB)是内存带宽级,远低于一次 JPEG 编码。纯函数,Linux 可单测。(WGC dirty-region 可作未来精化,本期用 hash,平台无关、简单。)

## 4. 帧来源抽象(WGC 主、BitBlt 兜底)

broker 的"帧来源"可替换,下游不变:
- **WGC(主)**:`capture_session::Handler::on_frame_arrived` 写入 `latest` 后,**通知编码任务**(新增 `tokio::sync::Notify` 或 raw-seq `watch<u64>`,handler 每帧 bump,编码任务 `notified().await` 取最新原始帧)。
- **BitBlt(兜底)**:WGC 启动失败(老系统/不可用)时,起一个**定时抓取循环**(~500ms 一次 `capture_window` BitBlt → hash 去重 → 变化才编码),喂同一个 broker。
- **未选窗**:broker 推一次占位图(沿用 `placeholder_jpeg`),两端显示"等待捕获"。

`CaptureManager::start(hwnd)` 启动 WGC 会话 + 编码任务(或 BitBlt 兜底循环);`stop()` 拆除并清空 broker。

## 5. 主持人传输 — MJPEG 流

新路由 `GET /api/stream.mjpg?token=...`(沿用 `token_ok`,连接时校验一次):
- 响应头 `Content-Type: multipart/x-mixed-replace; boundary=frame`。
- body 是基于 `watch::Receiver` 的异步流(`axum::body::Body::from_stream` + `async-stream`/`WatchStream`):连上**立即写当前最新帧**,之后每次 `rx.changed().await` 写一段:
  ```
  --frame\r\nContent-Type: image/jpeg\r\nContent-Length: <N>\r\n\r\n<jpeg 字节>\r\n
  ```
- 空闲:无新帧 → 不写 → 连接保持、**零流量零 CPU**。
- 客户端断开 → body 流 drop → 订阅释放,无泄漏。单帧编码失败跳过、保持流活。

**presenter.js 改动**:SYNC 模式只需 `el.preview.src = "/api/stream.mjpg?token=" + token`;**删掉整条 `loadFrame` 链 + `?t=` 防缓存 + load/error 重排**;离开 SYNC 把 `src` 清空(关连接)。浏览器原生渲染 multipart,换帧无闪烁。保留 `/api/snapshot.jpg` 一次性兜底。

**token 轮换**:流在连接时校验;token 重置后主持人的状态轮询会 401、客户端自行拆掉 SYNC(并由 401 自愈逻辑提示重扫),无需流内反复校验。

## 6. 导播传输 — Tauri Channel(原始字节 → canvas)

导播 webview 拉环回 HTTP 会被 Chromium PNA 拦(此前正因此改 IPC),故**不走 MJPEG**:
- 新命令 `subscribe_preview(channel: tauri::ipc::Channel<...>)`:Rust 订阅 broker,每来新帧把**原始 JPEG 字节**(`tauri::ipc::Response` / `Vec<u8>`,免 base64 膨胀)推入 channel。
- `DirectorPreview.vue`:`onMounted` 建 `Channel`、调 `subscribe_preview`;`onmessage` 收到字节 → `createImageBitmap(new Blob([bytes]))` → 画到 `<canvas>`(双缓冲、零闪烁)。`onUnmounted` 关闭 channel。
- 替掉现 400ms `get_snapshot` 轮询;只在变化时收帧。`get_snapshot` IPC 保留作兜底/首帧。

## 7. 画质

编码频率大降后,把质量 q70 → **q82**,分辨率上限保持 1280×720(留参数,便于后续调)。CPU 几乎不增,文字幻灯片更清楚。非重点,顺手收益。

## 8. 错误处理小结
- MJPEG 客户端断开 → 订阅自动释放;单帧编码失败 → 跳过该帧、流不断。
- watch 慢消费者自动合并,无错误路径。
- 导播 Channel 前端卸载 → 关闭 → Rust 侧发送端 drop。
- WGC 启动失败 → 自动退 BitBlt 兜底循环;未选窗 → 占位图。

## 9. 测试
**Linux 可单测(纯函数 / broker 语义)**
- 变化检测:同一缓冲 hash 相等、改一字节 hash 变。
- MJPEG 分帧:给定 JPEG 字节,生成的 multipart 段头(boundary + Content-Type + Content-Length + CRLF)逐字节正确。
- broker:`publish` 后新订阅者立即拿到最新帧;连续 publish 后慢订阅者只见最新 seq。
- 复用既有 `scaled_dims` / `encode_scaled_jpeg` / `bgra_to_rgb` 测试。

**需 Windows 运行时**:WGC 捕获、端到端 MJPEG 流、导播 Channel→canvas 渲染、BitBlt 兜底、空闲 CPU 实测。

## 10. 影响文件
| 文件 | 改动 |
|---|---|
| `src-tauri/src/frame_broker.rs` | **新增**:`EncodedFrame`、watch 广播、`publish`/`subscribe`、变化检测 hash、编码任务循环 |
| `src-tauri/src/capture_session.rs` | 帧到达通知编码任务(Notify / raw-seq) |
| `src-tauri/src/capture.rs` | BitBlt 兜底循环喂 broker;编码质量参数 q82;复用纯函数 |
| `src-tauri/src/routes.rs` | 新增 `/api/stream.mjpg` MJPEG 流处理 |
| `src-tauri/src/commands.rs` | 新增 `subscribe_preview(channel)`;保留 `get_snapshot` |
| `src-tauri/src/lib.rs` / `app_state.rs` | broker 接线进 `CaptureManager`/`AppState`;注册命令 |
| `src/components/DirectorPreview.vue` | Channel 订阅 → canvas;删 400ms 轮询 |
| `src/presenter/presenter.js` | SYNC 用 `<img src=stream.mjpg>`;删 `loadFrame` 轮询链 |

## 11. 风险
- **WGC/Channel/MJPEG 端到端只能在 Windows 验证**:本机交叉编译(0 error)证明编译,运行行为需真机。
- 导播 Channel 二进制推送是新机制(现有代码用 event):若 `tauri::ipc::Channel` 在本工程版本行为不符,退化为"事件 + base64 + data-url img"(设计第 6 节的备选)。
- MJPEG 在所有目标手机浏览器原生支持(`<img>`),低风险;仍保留 `snapshot.jpg` 兜底。
