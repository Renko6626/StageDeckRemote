# StageDeck Remote 项目文档

## 1. 项目概述

**StageDeck Remote** 是一个用于线下舞台活动的局域网 PPT 远程控制系统。

它的核心目标是：在不暴露导播电脑完整桌面的前提下，让主持人通过手机或平板看到导播电脑上的 PowerPoint 当前画面，并对 PPT 画面中的按钮进行点击操作，同时支持翻页、黑屏、恢复、锁定控制等舞台常用功能。

典型场景包括：

* 答题活动
* 互动小游戏
* 舞台主持流程控制
* PPT 内按钮点击
* PPT 内超链接 / 动画 / 跳转触发
* 导播房和主持人距离较远、沟通不便的活动现场

本项目不是远程桌面软件，而是一个**受限的舞台控制面板**。

---

## 2. 背景需求

当前活动流程中，导播电脑负责：

```text
PowerPoint 全屏播放
→ OBS 捕捉 PPT 画面
→ HDMI 输出到舞台屏幕
```

问题是：

1. PPT 的实际控制权在导播电脑上。
2. 主持人位于舞台前方，难以及时和导播沟通。
3. 仅仅支持“下一页 / 上一页”不足以覆盖答题活动。
4. PPT 内可能存在按钮，例如 A/B/C/D 选项、显示答案、跳转下一题等。
5. 直接使用远程桌面风险较高，主持人可能误点 OBS、任务栏、聊天窗口或其他桌面元素。

因此需要一个系统，让主持人可以：

```text
看到当前 PPT 画面
→ 点击 PPT 上的按钮
→ 控制下一页 / 上一页
→ 必要时黑屏 / 恢复
```

但不能让主持人完整接管导播电脑。

---

## 3. 目标

### 3.1 核心目标

* 导播电脑运行一个桌面应用。
* 桌面应用捕获当前 PowerPoint 放映画面。
* 主持人用手机或平板浏览器访问控制页面。
* 控制页面显示当前 PPT 预览画面。
* 主持人在预览画面上点击某个位置。
* 导播电脑将手机端点击坐标映射到 PPT 窗口坐标。
* 导播电脑模拟鼠标点击，触发 PPT 内按钮。
* 支持下一页、上一页、锁定控制、黑屏、恢复等常用操作。
* 导播端始终拥有最高控制权，可以随时接管或禁用主持人控制。

### 3.2 非目标

本项目不做：

* 通用远程桌面。
* 通用会议投屏。
* 云端服务。
* 跨公网控制。
* 完整 OBS 替代品。
* PowerPoint 编辑器。
* 多人在线协作文档系统。

本项目只解决一个明确问题：

> 在舞台活动现场，主持人如何安全、稳定、受限地远程控制导播电脑上的 PPT 放映画面。

---

## 4. 使用场景

### 4.1 标准舞台拓扑

```text
主持人手机 / 平板
        ↓ Wi-Fi
自带路由器 / AP
        ↓ 网线
导播电脑
        ↓
PowerPoint 放映窗口
        ↓
OBS 捕捉
        ↓ HDMI
舞台屏幕
```

### 4.2 软件拓扑

```text
导播电脑
├── PowerPoint 全屏或窗口化放映
├── OBS 捕捉 PPT
└── StageDeck Remote
      ├── 导播端 GUI
      ├── 本地 HTTP / WebSocket 服务
      ├── 截取 PPT 当前画面
      ├── 推送画面给主持人手机
      ├── 接收主持人点击坐标
      ├── 映射为导播电脑屏幕坐标
      ├── 模拟鼠标点击
      └── 模拟键盘翻页
```

### 4.3 主持人端界面

```text
┌────────────────────────────┐
│        当前 PPT 画面         │
│                            │
│    主持人点哪里就点击哪里     │
│                            │
├────────────────────────────┤
│  上一页   下一页   黑屏      │
│  恢复     锁定 / 释放控制    │
└────────────────────────────┘
```

---

## 5. 技术选型

### 5.1 总体选型

```text
导播端桌面应用：Tauri v2
前端界面：Vue 3 + TypeScript + Vite
后端核心：Rust
本地服务：Axum + Tokio
主持人端：浏览器 Web 页面
系统调用：Windows API / windows-rs
```

### 5.2 为什么使用 Tauri

选择 Tauri 的原因：

1. 比 Python 脚本更适合分发。
2. 可以打包成正常的 Windows 应用。
3. 前端可以使用熟悉的 Web 技术。
4. Rust 侧适合处理窗口枚举、截图、鼠标键盘模拟等原生能力。
5. 主持人端不需要安装 App，只需要浏览器扫码打开。
6. 后续可以扩展自动更新、配置保存、日志、OBS WebSocket 控制等功能。

### 5.3 不选择纯 Python 的原因

Python 适合快速原型，但正式分发存在问题：

* 依赖安装麻烦。
* PyInstaller 打包体积大。
* 不同 Windows 环境兼容性不稳定。
* 容易被杀软误报。
* 普通导播/主持人不适合处理 Python 环境。

因此，Python 可以用于技术验证，但正式版本采用 Tauri + Rust。

---

## 6. 核心功能

## 6.1 导播端功能

导播端是一个 Windows 桌面应用，负责系统控制和兜底。

### 6.1.1 PPT 窗口选择

功能：

* 枚举当前系统窗口。
* 自动识别 PowerPoint 放映窗口。
* 支持手动选择目标窗口。
* 显示窗口标题、位置、尺寸。
* 检测窗口是否仍然存在。

示例界面：

```text
PPT 窗口：
[PowerPoint Slide Show - 活动答题.pptx] [重新选择]
窗口位置：1920×1080 at (0, 0)
状态：已锁定
```

### 6.1.2 本地服务管理

功能：

* 启动本地 HTTP / WebSocket 服务。
* 默认监听端口，例如 `8787`。
* 显示本机局域网 IP。
* 生成主持人访问二维码。
* 显示当前连接设备数量。
* 显示主持人端延迟和状态。

示例：

```text
控制地址：
http://192.168.50.10:8787/presenter?token=xxxx

[显示二维码]
主持人：已连接
延迟：18 ms
```

### 6.1.3 控制权限管理

功能：

* 允许 / 禁止远程控制。
* 强制锁定主持人控制。
* 查看最近一次点击坐标。
* 查看最近一次操作时间。
* 防止多个设备同时抢控制。

建议权限状态：

```text
remote_enabled: true / false
control_locked: true / false
active_client_id: string | null
```

### 6.1.4 本地兜底控制

导播端提供本地按钮：

```text
[上一页] [下一页]
[黑屏] [恢复]
[强制锁定]
[允许主持人控制 ON/OFF]
```

导播端拥有最高优先级。

---

## 6.2 主持人端功能

主持人端是浏览器网页，不需要安装应用。

### 6.2.1 PPT 画面预览

功能：

* 显示当前 PPT 画面。
* 自动刷新。
* 支持横屏平板。
* 支持手机访问。
* 显示连接状态。
* 显示控制锁定状态。

预览刷新策略：

```text
MVP：HTTP 轮询 /snapshot.jpg
刷新率：2–5 fps
图片尺寸：1280×720
压缩格式：JPEG / WebP
```

### 6.2.2 远程点击

主持人点击预览画面时，前端发送归一化坐标：

```json
{
  "x": 0.723,
  "y": 0.614
}
```

后端映射为 PPT 窗口内的实际屏幕坐标：

```text
screen_x = ppt_left + x * ppt_width
screen_y = ppt_top  + y * ppt_height
```

然后模拟鼠标点击。

### 6.2.3 翻页控制

按钮：

```text
上一页
下一页
```

对应操作：

```text
上一页 → 模拟 Left Arrow / PageUp
下一页 → 模拟 Right Arrow / Space / PageDown
```

### 6.2.4 锁定控制

默认建议：

```text
主持人端默认锁定
点击“释放控制”后才允许点击 PPT
30 秒无操作自动锁定
导播端可随时强制锁定
```

锁定状态：

```text
已锁定：点击画面无效
已释放：点击画面会发送到导播电脑
```

### 6.2.5 状态显示

主持人端应显示：

```text
连接：正常 / 断开
延迟：xx ms
PPT 窗口：已找到 / 未找到
控制：已锁定 / 可控制
导播授权：允许 / 禁止
```

---

## 6.3 OBS 控制功能

OBS 控制属于 P1 / P2 功能，不是 MVP 必须项。

### 6.3.1 黑屏 / 恢复

可选实现方式：

1. 通过 OBS WebSocket 切换场景。
2. 本地应用创建一个全黑覆盖窗口并让 OBS 捕捉。
3. 直接让 PPT 黑屏，例如发送 `B` 键。

推荐长期方案：

```text
OBS WebSocket
```

接口：

```text
POST /api/black
POST /api/restore
```

导播端配置：

```text
OBS WebSocket 地址：ws://127.0.0.1:4455
OBS WebSocket 密码：******
PPT 场景：PPT
黑屏场景：Black
待机场景：Standby
```

---

## 7. 坐标系统设计

### 7.1 坐标原则

主持人端永远发送归一化坐标，而不是设备像素坐标。

正确：

```json
{
  "x": 0.5,
  "y": 0.5
}
```

错误：

```json
{
  "x": 842,
  "y": 391
}
```

原因：

* 手机、平板、浏览器窗口尺寸不同。
* 横屏、竖屏不同。
* CSS 缩放不同。
* 图片显示区域和原始截图尺寸可能不同。
* 导播电脑分辨率可能变化。

### 7.2 映射公式

```text
screen_x = window_left + normalized_x * window_width
screen_y = window_top  + normalized_y * window_height
```

### 7.3 点击流程

```text
主持人点击画面
→ 前端计算点击点在图片中的归一化坐标
→ POST /api/click
→ 后端验证 token 和控制权限
→ 后端读取当前 PPT 窗口矩形
→ 后端计算屏幕坐标
→ 后端模拟鼠标点击
→ 后端返回结果
```

---

## 8. 网络设计

### 8.1 推荐网络拓扑

```text
主持人平板
      ↓ 5GHz Wi-Fi
自带路由器 / AP
      ↓ 网线
导播电脑
```

### 8.2 网络原则

* 不依赖场馆 Wi-Fi。
* 不需要互联网。
* 导播电脑优先使用有线连接。
* 主持人设备使用 5GHz Wi-Fi。
* 路由器关闭 AP Isolation / Client Isolation。
* 导播电脑固定 IP。
* 使用 token 防止其他人误连控制页面。

### 8.3 推荐配置

```text
SSID: StageDeck-Control
频段: 5GHz
网段: 192.168.50.0/24
导播电脑固定 IP: 192.168.50.10
控制端口: 8787
访问地址: http://192.168.50.10:8787/presenter?token=xxxx
```

---

## 9. API 设计

### 9.1 页面接口

```text
GET /presenter?token=xxx
```

返回主持人控制页面。

---

### 9.2 状态接口

```text
GET /api/status?token=xxx
```

返回：

```json
{
  "ok": true,
  "ppt_window_found": true,
  "remote_enabled": true,
  "control_locked": false,
  "active_client_id": "host-tablet",
  "snapshot_fps": 3,
  "server_time": 1710000000
}
```

---

### 9.3 截图接口

```text
GET /api/snapshot.jpg?token=xxx
```

返回当前 PPT 窗口截图。

MVP 可以使用 HTTP 轮询。

后续可以升级为：

```text
WebSocket
MJPEG
WebRTC
```

但当前项目不建议一开始使用 WebRTC，因为复杂度过高。

---

### 9.4 点击接口

```text
POST /api/click
```

请求：

```json
{
  "token": "xxxx",
  "client_id": "host-tablet",
  "x": 0.723,
  "y": 0.614
}
```

响应：

```json
{
  "ok": true,
  "screen_x": 1388,
  "screen_y": 663
}
```

失败响应：

```json
{
  "ok": false,
  "reason": "control_locked"
}
```

可能的失败原因：

```text
invalid_token
control_locked
remote_disabled
ppt_window_not_found
coordinate_out_of_range
input_failed
```

---

### 9.5 翻页接口

```text
POST /api/next
POST /api/prev
```

请求：

```json
{
  "token": "xxxx",
  "client_id": "host-tablet"
}
```

---

### 9.6 控制锁接口

```text
POST /api/lock
POST /api/unlock
```

用于主持人端请求锁定 / 释放控制。

导播端强制锁定优先级更高。

---

### 9.7 OBS 接口

```text
POST /api/black
POST /api/restore
```

MVP 可暂时不实现。

---

## 10. 项目结构

推荐项目结构：

```text
stagedeck-remote/
├── README.md
├── docs/
│   ├── project.md
│   ├── network.md
│   ├── protocol.md
│   └── rehearsal-checklist.md
│
├── src/
│   ├── main.ts
│   ├── App.vue
│   ├── router.ts
│   ├── stores/
│   │   ├── appState.ts
│   │   └── networkState.ts
│   ├── components/
│   │   ├── WindowPicker.vue
│   │   ├── QrCodePanel.vue
│   │   ├── ControlStatus.vue
│   │   ├── LocalControlPanel.vue
│   │   └── LogPanel.vue
│   ├── presenter/
│   │   ├── presenter.html
│   │   ├── presenter.ts
│   │   └── presenter.css
│   └── utils/
│       ├── api.ts
│       └── qrcode.ts
│
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   └── src/
│       ├── main.rs
│       ├── app_state.rs
│       ├── web_server.rs
│       ├── routes.rs
│       ├── auth.rs
│       ├── window_manager.rs
│       ├── capture.rs
│       ├── input.rs
│       ├── obs.rs
│       ├── network.rs
│       ├── config.rs
│       └── log.rs
│
└── assets/
    ├── icon.png
    └── standby.png
```

---

## 11. Rust 模块职责

### 11.1 `main.rs`

职责：

* 启动 Tauri 应用。
* 初始化全局状态。
* 启动本地 Web 服务。
* 注册 Tauri commands。
* 加载配置。

---

### 11.2 `app_state.rs`

职责：

维护全局运行状态：

```rust
pub struct AppState {
    pub selected_window: Option<TargetWindow>,
    pub remote_enabled: bool,
    pub control_locked: bool,
    pub active_client_id: Option<String>,
    pub token: String,
    pub server_port: u16,
}
```

---

### 11.3 `web_server.rs`

职责：

* 启动 Axum 服务。
* 监听 `0.0.0.0:8787`。
* 挂载 API 路由。
* 提供主持人页面静态资源。

---

### 11.4 `routes.rs`

职责：

实现 HTTP API：

```text
GET  /presenter
GET  /api/status
GET  /api/snapshot.jpg
POST /api/click
POST /api/next
POST /api/prev
POST /api/lock
POST /api/unlock
POST /api/black
POST /api/restore
```

---

### 11.5 `auth.rs`

职责：

* 生成随机 token。
* 校验请求 token。
* 可选：区分主持人、导播、场控权限。

---

### 11.6 `window_manager.rs`

职责：

* 枚举系统窗口。
* 识别 PowerPoint 放映窗口。
* 获取窗口标题。
* 获取窗口矩形。
* 将窗口置前。
* 检查目标窗口是否仍然存在。

---

### 11.7 `capture.rs`

职责：

* 截取目标窗口区域。
* 压缩为 JPEG / WebP。
* 控制截图尺寸和质量。
* 缓存最近一帧，避免重复截图过快。

MVP 可以先做屏幕矩形截图。

---

### 11.8 `input.rs`

职责：

* 模拟鼠标移动。
* 模拟鼠标点击。
* 模拟键盘按键。
* 实现下一页 / 上一页。
* 点击前后可保存 / 恢复鼠标位置。

---

### 11.9 `obs.rs`

职责：

* 连接 OBS WebSocket。
* 切换场景。
* 黑屏。
* 恢复 PPT 场景。

MVP 可暂时留空。

---

### 11.10 `network.rs`

职责：

* 获取本机局域网 IP。
* 过滤虚拟网卡。
* 为导播端显示可访问地址。
* 生成访问 URL。

---

### 11.11 `config.rs`

职责：

保存配置：

```json
{
  "server_port": 8787,
  "last_selected_window_title": "",
  "obs_enabled": false,
  "obs_url": "ws://127.0.0.1:4455",
  "obs_password": "",
  "ppt_scene_name": "PPT",
  "black_scene_name": "Black"
}
```

---

### 11.12 `log.rs`

职责：

记录关键事件：

```text
服务启动
主持人连接
主持人断开
点击坐标
翻页
黑屏
恢复
控制锁定
窗口丢失
错误信息
```

---

## 12. 前端页面设计

## 12.1 导播端 GUI

页面模块：

```text
WindowPicker
- 选择 PPT 窗口
- 自动刷新窗口列表

NetworkStatus
- 显示本机 IP
- 显示端口
- 显示二维码

ControlStatus
- 主持人连接状态
- 延迟
- 控制锁状态

LocalControlPanel
- 下一页
- 上一页
- 黑屏
- 恢复
- 强制锁定
- 允许 / 禁止远程控制

LogPanel
- 最近操作日志
```

---

## 12.2 主持人端页面

主持人端页面应尽量简单，避免误触。

布局：

```text
顶部状态栏：
连接状态 / 延迟 / 控制状态

中间：
PPT 预览图

底部大按钮：
上一页 / 下一页 / 锁定 / 释放控制 / 黑屏 / 恢复
```

建议：

* 按钮足够大。
* 适配 iPad 横屏。
* 页面禁止缩放。
* 页面尽量不滚动。
* 锁定状态下点击预览图无效。
* 断线时自动禁用所有控制按钮。

---

## 13. 安全与误触保护

### 13.1 Token 鉴权

所有主持人端接口必须带 token。

示例：

```text
http://192.168.50.10:8787/presenter?token=xxxx
```

API 请求也必须携带 token。

---

### 13.2 控制锁

至少有三层锁：

```text
1. 导播端总开关 remote_enabled
2. 导播端强制锁 control_locked
3. 主持人端本地锁 presenter_unlocked
```

只有三者都允许时，点击才会执行。

---

### 13.3 坐标检查

后端必须检查：

```text
0.0 <= x <= 1.0
0.0 <= y <= 1.0
```

非法坐标直接拒绝。

---

### 13.4 多客户端控制

MVP 只允许一个 active client 控制。

策略：

```text
第一个连接的主持人端成为 active_client
其他设备只读或需要导播授权
导播端可以清除 active_client
```

---

### 13.5 日志

所有实际操作必须记录：

```text
时间
客户端 ID
操作类型
坐标
执行结果
失败原因
```

---

## 14. MVP 范围

第一版 MVP 只做必要功能。

### P0 必须实现

* Tauri 桌面应用启动。
* Axum 本地服务启动。
* 显示本机 IP 和二维码。
* 主持人手机能打开页面。
* 手动设置 / 选择 PPT 窗口区域。
* 截取 PPT 窗口画面。
* 主持人端显示截图。
* 主持人点击截图后，导播电脑模拟点击。
* 下一页 / 上一页。
* token 鉴权。
* 控制锁。
* 导播强制关闭远程控制。

### P1 活动可用

* 自动枚举并选择 PowerPoint 放映窗口。
* 截图帧率控制。
* 连接状态检测。
* 延迟显示。
* 断线自动重连。
* 操作日志。
* 多客户端只允许一个控制者。
* 导播端显示最近一次点击坐标。
* Windows 防火墙提示说明。
* 配置持久化。

### P2 完整舞台系统

* OBS WebSocket 黑屏 / 恢复。
* 待机场景切换。
* 主持词提示。
* 倒计时。
* A/B/C/D 语义按钮。
* PPT 页码识别或 PowerPoint COM 控制。
* 多角色权限：主持人 / 导播 / 场控。
* 自动更新。
* 安装包分发。
* 彩排模式。

---

## 15. 开发路线

### 阶段 0：技术验证

目标：证明核心链路可行。

内容：

```text
固定截图区域
→ 手机网页显示截图
→ 手机点击
→ 导播电脑鼠标点击 PPT
```

不需要漂亮 UI。

---

### 阶段 1：Tauri MVP

目标：做出可演示的桌面应用。

内容：

```text
Tauri GUI
Axum 服务
主持人页面
截图
点击
翻页
token
控制锁
二维码
```

---

### 阶段 2：活动可用版

目标：可以拿去实际小型活动使用。

内容：

```text
窗口选择
状态检测
断线重连
日志
多客户端限制
导播强制接管
配置保存
网络说明文档
```

---

### 阶段 3：正式舞台版

目标：支持复杂互动活动。

内容：

```text
OBS WebSocket
黑屏 / 恢复 / 待机
语义按钮
主持词
倒计时
权限分层
安装包
自动更新
```

---

## 16. 现场部署流程

### 16.1 硬件准备

```text
导播电脑
主持人平板 / 手机
备用控制手机
自带路由器 / AP
网线
HDMI 线
备用翻页笔
```

### 16.2 网络准备

```text
1. 路由器开机
2. 导播电脑用网线连接路由器
3. 主持人平板连接专用 Wi-Fi
4. 确认导播电脑固定 IP
5. 打开 StageDeck Remote
6. 主持人扫码进入控制页面
```

### 16.3 软件准备

```text
1. 打开 PowerPoint
2. 开始放映
3. 打开 OBS
4. OBS 捕捉 PPT 放映窗口
5. 打开 StageDeck Remote
6. 选择 PPT 窗口
7. 开启远程控制
8. 主持人端测试点击和翻页
```

---

## 17. 彩排检查清单

正式活动前必须测试：

```text
[ ] 主持人实际站位能稳定连接 Wi-Fi
[ ] 主持人端能打开控制页面
[ ] PPT 预览画面正常刷新
[ ] 点击 PPT 按钮位置准确
[ ] 上一页 / 下一页正常
[ ] 锁定 / 释放控制正常
[ ] 导播端强制锁定正常
[ ] 导播端关闭远程控制后，主持人无法操作
[ ] OBS 捕捉画面正常
[ ] HDMI 输出正常
[ ] 断网后不会误操作
[ ] 恢复网络后可以重连
[ ] 主持人设备不会自动锁屏
[ ] 有备用手机 / 翻页笔 / 导播人工兜底
```

---

## 18. 主要风险

### 18.1 DPI 缩放

Windows 显示缩放可能导致坐标不一致。

建议：

```text
导播电脑显示缩放设置为 100%
PPT 窗口固定 16:9
OBS 捕捉固定窗口
```

---

### 18.2 PPT 窗口失焦

如果 PPT 窗口失焦，键盘翻页可能失效。

解决方案：

```text
执行翻页前先激活 PPT 窗口
点击操作天然会激活窗口
导播端显示窗口状态
```

---

### 18.3 鼠标指针被观众看到

OBS 捕捉时可能捕获鼠标指针。

解决方案：

```text
OBS 关闭捕获光标
或点击后将鼠标移回角落
或真实鼠标只在点击瞬间移动
```

---

### 18.4 网络不稳定

解决方案：

```text
自带路由器
导播电脑有线连接
主持人设备使用 5GHz Wi-Fi
不依赖场馆 Wi-Fi
提供备用翻页笔和导播人工接管
```

---

### 18.5 多人误连

解决方案：

```text
随机 token
二维码只给主持人 / 场控
只允许一个 active client 控制
导播端可踢出控制端
```

---

## 19. 第一版推荐实现策略

第一版不要追求复杂系统，先完成最短闭环：

```text
Tauri 应用启动
→ 导播手动选择 PPT 窗口
→ 应用启动本地服务
→ 手机扫码打开页面
→ 手机看到 PPT 截图
→ 手机点击截图
→ 导播电脑执行鼠标点击
→ PPT 内按钮被触发
```

只要这个闭环稳定，项目就成立。

---

## 20. 项目一句话定义

StageDeck Remote 是一个面向线下舞台活动的局域网 PPT 远程控制工具。它让主持人通过手机或平板看到导播电脑上的 PPT 放映画面，并以受限、安全、可接管的方式点击 PPT 按钮、翻页和触发舞台流程，同时避免完整远程桌面带来的误操作风险。
