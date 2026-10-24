# GIF 页预览冻结：Windows 排查交接

记录日期：2026-10-24。发布基线：v0.3.1，提交 `aa76216`。这是未解决问题的交接记录，不是已实施的修复方案。Windows 启动与迭代命令见 [开发指南](../windows-development.md)。

## 用户已确认的复现现象

1. 从目标页的上一页开始，电脑实际 PowerPoint 放映、导播预览、手机屏幕同步最初正常。
2. 手机按“下一页”，电脑实际放映进入含 GIF 的目标页，显示和 GIF 播放正常。
3. 导播和手机预览仍冻结在上一页；最初描述的“空白”后来已修正为旧画面保留。手机页码可能同时变为 `--/--`。
4. 再按“下一页”，电脑正常进入后续无 GIF 页，导播和手机图像立即恢复。
5. 同一问题页仅移除 GIF 后，扫描和宏操作明显恢复。

**GIF 是主要触发因素，有同页对照支持。** 但尚未证明它是通过 WGC、取帧、去重还是编码影响图像更新。不要把通用采样缺陷直接写成此次故障的已证实根因，也不要把旧描述“整页空白”继续当作验收现象。

## 已有证据

以下两份日志来自 Windows 0.3.1，启动时均为 `relay_enabled=false`。原文件未提交到仓库。

| 第 87 页，放映中，六个宏按钮 | 有 GIF：`diagnostics_1024.jsonl` | 无 GIF：`diagnostics_1024_nogif.jsonl` |
| --- | --- | --- |
| 完整扫描次数 | 3 | 9 |
| 扫描耗时 | 870、1773、5380ms | 52～128ms，中位数 77ms |
| 每轮成员查询 | 133 | 130 |

有 GIF 的 5.38 秒扫描里，`GetIDsOfNames` 和 `Invoke` 各普遍约 15ms，一个成员合计约 30ms；慢在大量调用累计，不是某次 GIF 属性读取独占数秒。无 GIF 文件记录 12 次宏请求成功。有 GIF 的较早记录出现两次宏请求因等待通道超过 500ms 被拒绝。

这些数据证明扫描慢与 GIF 有强关联，**没有记录这次屏幕同步复现的 `/api/click`、`stream.mjpg` 或逐帧信息**，不能据此认定捕获 API 已卡死。

`gif_pptm.pptm` 独立样本的第二页有 37 个顶层形状、六个宏绑定，GIF 名为 `Hint_Gif_4`，360×355、140 帧、每帧 90ms、无限循环。GIF 可见且不属于页面动画时间轴；其余为六组点击触发的淡入淡出。完整版与抽取样本的形状数和宏绑定有差异，性能数据以实际文稿和日志为准。

## 当前代码中两条独立链路

```text
PowerPoint COM -> 当前页 / 宏按钮 -> 手机页码、宏列表

PowerPoint 窗口 -> WGC FrameArrived -> frame.buffer() -> 最新 BGRA
    -> 采样 hash -> JPEG 编码 -> FrameBroker
        -> 导播 Tauri Channel -> canvas
        -> 手机 MJPEG -> img
        -> 可选 Relay 推流 -> 手机 MJPEG
```

- `--/--` 来自 COM 超时/忙碌响应中的零页码。它不证明图像捕获失败。
- COM 不可用不会主动清空手机 `img` 或导播 canvas。
- 两端同时冻结而电脑放映正常，优先检查共用的 WGC→编码→broker 上游。
- 屏幕坐标点击 `/api/click` 使用 Win32 `SendInput`，不拿 COM 信号量。但屏幕模式和导播仍在后台扫描 COM，屏幕模式不等于完全关闭自动化。
- WGC 启动失败才进入 BitBlt 兜底；运行中没有自动切换后端的实现。

## 代码入口

| 文件 | 关注位置 |
| --- | --- |
| `src-tauri/src/capture_session.rs` | `Handler::on_frame_arrived`：回调入口、`frame.buffer()`、写入 `latest`、通知编码线程 |
| `src-tauri/src/capture.rs` | `CaptureManager::start`、`encode_changed`、`snapshot_jpeg`、BitBlt 分支 |
| `src-tauri/src/frame_broker.rs` | `sample_hash`、`FrameBroker::publish`、`seq` |
| `src-tauri/src/commands.rs` | `subscribe_preview`：导播帧订阅 |
| `src-tauri/src/routes.rs` | `stream_mjpeg`、`snapshot`、`do_click` |
| `src/components/DirectorPreview.vue` | `drawFrame`，独立的 `pollState` |
| `src/presenter/presenter.js` | `startStream`、预览点击处理、`pollSlide` |
| `src-tauri/src/ppt_com.rs` | `invoke_inner`、`slide_actions`、`read_slide_state` |

## 下一次快速取证

先按开发指南使用局域网和 `pnpm tauri dev --release` 做对照，记录分辨率、缩放和窗口/全屏模式；在复现前开启详细诊断。只测试“上一页 → GIF 页停留数秒 → 下一页”，同时观察电脑实际放映、导播、手机，避免引入 Relay 变量。

故障停留时，先手动请求已有 `snapshot.jpg`：

- 快照已是目标页，而两个预览仍旧：捕获已有新内容，重点查去重、编码、发布。
- 快照仍是上一页：最新原始缓冲也可能陈旧，需区分 WGC 回调停更和 `frame.buffer()` 未返回。
- 快照为占位/桌面画面：核对捕获窗口和捕获日志，不能按“旧 WGC 帧”直接判定。

若需要补最小诊断，只在下面三个位置加临时记录；**当前版本尚未加入**：

1. WGC 回调入口：回调编号、时间、尺寸和 `map_begin`。
2. `frame.buffer()` 返回：相同编号的 `map_end`、耗时和错误。
3. 编码线程：生产采样 hash、跳过/编码结果、发布后的 broker seq；短时按秒取覆盖全部字节的诊断指纹，与生产采样区分。

| 故障时记录 | 指向 |
| --- | --- |
| 回调停止，没有未完成的 Map | WGC 没有继续交付回调 |
| 有 `map_begin`，对应 `map_end` 缺失 | GPU 帧读回/Map 阶段等待 |
| 回调和 Map 持续，但原始内容仍是上一页 | 捕获路径返回旧内容 |
| 原始全字节指纹变化、生产采样不变、seq 不变 | 本地去重误判 |
| 生产采样变化，但未发布 | 检查编码结果 |

不记录每帧正文或把截图自动保存为长期日志；一次人工快照和短时诊断即可。先确定断点，再选择修复，不直接增加自动重启、重试队列或捕获后端切换。

## 独立只读审查发现

- **确定的代码缺陷，尚未证实为本次根因：** `sample_hash` 只取 1024 个等距字节。在 1920×1080、2560×1440 等 BGRA 缓冲下全部落在蓝通道，可能漏掉红绿变化及未命中的局部变化。注释中的“切页大面积变化，采样必然命中”不成立。
- **次要控制流问题：** `last_hash` 在 JPEG 编码成功前已更新。若编码失败，后续相同 hash 不会再次编码。当前没有本次 JPEG 失败证据。
- **低吻合度，暂缓：** `windows-capture 2.0.0` 的 control 仍持有 Handler；WGC 运行中终止后，编码线程未必按注释所说立即退出。真正终止通常不能因下一页自行恢复，不作为本次首要解释，不据此新增恢复机制。

独立审查没有改代码。随后提出的减少 COM 查询方案也尚未实施；不要把“缓存成员编号”“同页结果复用”“停止后台扫描”当作已合入功能。v0.3.1 只做了一秒通道等待、扫描降频和操作后刷新。

## 迭代边界

优先完成 Windows 人工取证与局部修复。沿用现有 Rust/Tauri/Vue/Win32 路线；新增依赖、缓存规则、模块边界或捕获后端方案先说明取舍并确认。测试围绕实际故障补最合适一层，按完整改动集中运行相关检查；不自动执行浏览器点击，不扩展大型端到端环境。

参考：[Office 线程模型](https://learn.microsoft.com/en-us/visualstudio/vsto/threading-support-in-office?view=visualstudio)、[COM 成员编号生命周期与复用](https://learn.microsoft.com/en-us/windows/win32/api/oaidl/nf-oaidl-idispatch-getidsofnames)。它们解释通用机制，不是微软对本次 GIF 缺陷的确认。
