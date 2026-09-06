# 网络部署与现场操作手册

> 面向**导播 / 场控**的现场操作手册：照着做即可完成网络搭建、启动、自检与排障。
> 网络**设计**（拓扑、网段、原则）见 [`project.md` §8](project.md)，部署**流程**总览见 §16，
> 彩排清单见 §17。本文不重复设计，只讲"怎么落地"。
>
> 约定取值（来自 [`project.md` §8.3](project.md)，可按现场调整）：
> 网段 `192.168.50.0/24`，导播电脑固定 IP `192.168.50.10`，控制端口 `8787`。
>
> **v0.2.0 起新增 Relay 模式**（CLAB 等校内中继服务器），作为场馆 WiFi 不可控时的备选路径——
> 本文 §1–§6 是 LAN 直连模式，§7 是 Relay 模式的部署/使用指引；架构层面见
> [`architecture.md` §11](architecture.md)。

---

## 1. 适用前提

- 自带路由器 / AP，**不依赖场馆 Wi-Fi**。
- 导播电脑优先**有线**连路由器；主持人设备走 **5GHz Wi-Fi**。
- 目标：主持人手机/平板打开 `http://192.168.50.10:8787/presenter?token=xxxx` 即可控制。

---

## 2. 一次性准备 A：路由器 / AP

> 不同品牌菜单位置不同，按"功能名"在你的后台里找即可。

1. **频段**：开启 5GHz，给主持人设备用（2.4GHz 可留作备用）。
2. **SSID / 密码**：建议专用 SSID，如 `StageDeck-Control`。
3. **关闭 AP 隔离 / Client Isolation**（关键！）
   - 一般在"无线设置 / Wi-Fi 高级 / 访客网络"里，名字可能叫
     *AP Isolation*、*Client Isolation*、*隔离*、*AP 隔离*。
   - **必须关闭**，否则手机和导播电脑在同一 Wi-Fi 下也互相不可见，扫码页面打不开。
4. **网段**：确认 LAN 网段为 `192.168.50.0/24`（或记下实际网段，后续 IP 按它来）。
5. **DHCP**：保持开启即可；导播电脑用固定 IP（下一节），主持人设备自动获取。

---

## 3. 一次性准备 B：导播电脑（Windows）

### 3.1 设置固定 IP

设置 → **网络和 Internet** → **以太网**（有线）→ 点开当前连接 →
**IP 分配** 旁 **编辑** → 选 **手动** → 打开 **IPv4**：

| 项 | 值 |
| --- | --- |
| IP 地址 | `192.168.50.10` |
| 子网掩码 / 子网前缀长度 | `255.255.255.0`（前缀 `24`） |
| 网关 | `192.168.50.1`（路由器 LAN 口地址） |
| DNS | 可留空或填 `192.168.50.1` |

> 经典路径：控制面板 → 网络和共享中心 → 更改适配器设置 → 右键以太网 → 属性 →
> Internet 协议版本 4 (TCP/IPv4) → 属性 → 使用下面的 IP 地址。

### 3.2 放行防火墙端口 8787

- **最简单**：首次启动 StageDeck Remote、本地服务绑定时，Windows 会弹"是否允许"，
  勾选**专用网络**并允许即可。
- **手动**：Windows Defender 防火墙 → 高级设置 → **入站规则** → 新建规则 →
  **端口** → TCP → 特定本地端口 `8787` → 允许连接 → 勾选**专用** → 命名 `StageDeck 8787`。

### 3.3 显示缩放设为 100%

设置 → 系统 → 显示 → **缩放 100%**。
缩放非 100% 会导致点击坐标偏移（原因见 [`project.md` §18.1](project.md)）。

---

## 4. 每次开场流程

1. 路由器开机；导播电脑用网线连路由器，确认固定 IP 生效。
2. 主持人平板连 `StageDeck-Control`（5GHz）。
3. 打开 PowerPoint 开始放映；打开 OBS 捕捉 PPT 窗口（如使用）。
4. 打开 **StageDeck Remote** → 选择 PPT 窗口 → **开启远程控制**。
5. 应用界面显示**本机 IP / 二维码 / 访问地址**；主持人**扫码**进入控制页。
6. 主持人**释放控制**后，点一下预览图、翻一次页做自检。

> 访问地址带 token，**只发给主持人 / 场控**，避免他人误连（[`project.md` §18.5](project.md)）。

---

## 5. 排障清单（按症状）

### 5.1 主持人连不上 Wi-Fi / 信号弱
- 确认连的是 5GHz 专用 SSID；让主持人站到实际站位再测。
- 自带 AP 尽量靠近主持人站位；避开大金属/人群遮挡。

### 5.2 连上了 Wi-Fi，但扫码 / 打开页面失败
- 确认地址里的 IP 与导播电脑实际 IP 一致（见 5.5 多网卡问题）。
- 确认端口是 `8787`，且防火墙已放行（§3.2）。
- 确认手机和电脑**同网段**（手机 IP 也是 `192.168.50.x`）。
- **AP 隔离没关**是最常见原因（§2 第 3 点）——回去再确认一次。

### 5.3 能看到画面，但点击 / 翻页无效
- 导播端**远程控制是否开启**、是否处于**强制锁定**。
- 主持人端是否已**释放控制**（锁定状态下点预览图无效，这是防误触设计）。
- PPT 放映窗口是否仍被选中、是否失焦（翻页前需 PPT 窗口获得焦点，[`project.md` §18.2](project.md)）。

### 5.4 画面不刷新 / 很卡
- 网络拥塞或信号弱：靠近 AP、确认走 5GHz、减少干扰源。
- 预览为轮询刷新（约 3fps），偶有延迟属正常。

### 5.5 导播电脑显示的 IP 不对（多网卡）
- 导播电脑若同时有有线 + 无线 / 虚拟网卡，应用显示的本机 IP 可能不是主持人那张网段的。
- 处理：**临时关掉其他网卡**（无线、VPN、虚拟网卡），只留连路由器的有线网卡；
  重开应用确认显示的是 `192.168.50.10`。

### 5.6 中途掉线
- 手机状态栏分三档：`LINK`（3s 内有心跳）→ `弱 · Ns`（黄点，按钮仍可用，按下去的动作会自动重发最多 3 次）→ `DOWN`（LAN 8s / Relay 15s 无心跳，按钮全禁、遮罩"连接已断开"）。
- 翻页 / 宏 / 点击带幂等 id：网络抖动导致重发也只执行一次；若 4s 内都没送达，手机提示"未送达或结果未知，核对页码后再按"——**先看页码再决定是否再按**。
- `DOWN` 只是手机本地锁定，不改导播侧状态；网络恢复后下一次心跳若仍是 ARM 状态，按钮直接回来，不用重新释放控制。控制权 10s 内不会被别的手机抢走。

---

## 6. 备用预案

- 备用翻页笔、备用手机（提前扫码就绪）。
- 导播端本地按钮可随时人工兜底翻页 / 黑屏 / 接管（[`project.md` §6.1.4](project.md)）。
- 详见彩排清单 [`project.md` §17](project.md) 与风险预案 §18。

---

## 7. Relay 模式（场馆 WiFi 不可控时的备选路径）

**用在什么场景**：场馆 WiFi 有 AP 隔离 / 客户端隔离 / 强制门户 / 出口不稳，
但你能在校园网里拿到一台 Linux 服务器（如 PKU CLAB）。导播 PC 和主持人手机**各自独立**
连上中继，绕过场馆点对点限制。LAN 模式同时保留，开演前看场地条件挑哪条。

### 7.1 边界与不解决的问题

- 中继只解决"对等阻断"。**双方仍必须能进校园网**（导播走场馆 WiFi 出口 + 校园 VPN，
  或手机走校园 WiFi）。蜂窝走不到内网中继。
- 校园 WiFi 的**强制门户**得提前认证好，本系统不替你处理 captive portal。
- Relay 重启对正在进行的会话是硬中断（无持久化），导播会自动重连。

### 7.2 一次性准备 C：在中继服务器上部署 relay

> 部署目标：让 relay binary 作为 systemd 服务长跑，监听 `0.0.0.0:8080`。

1. **拿二进制**：在开发机 `cargo build --release -p relay` 编出 `target/release/relay`，
   `scp` 到中继服务器（如 `/usr/local/bin/stagedeck-relay`）。
2. **配置**：建 `/etc/stagedeck-relay/config.toml`（可选；缺省值能跑）：

   ```toml
   # listen 默认 0.0.0.0:8080
   listen = "0.0.0.0:8080"
   # session 宽限期（导播 WS 断后保留会话多少秒，等待重连）
   session_grace_secs = 60
   # 单次隧道请求超时
   tunnel_timeout_secs = 30
   ```

3. **systemd 单元**：`/etc/systemd/system/stagedeck-relay.service`：

   ```ini
   [Unit]
   Description=StageDeck Relay
   After=network-online.target
   Wants=network-online.target

   [Service]
   Type=simple
   ExecStart=/usr/local/bin/stagedeck-relay
   Environment=STAGEDECK_RELAY_CONFIG=/etc/stagedeck-relay/config.toml
   Restart=on-failure
   RestartSec=2

   [Install]
   WantedBy=multi-user.target
   ```

   `systemctl enable --now stagedeck-relay` 启用 + 启动；`journalctl -u stagedeck-relay -f`
   看运行日志。

4. **健康检查**：本机 `curl http://127.0.0.1:8080/r/SANITY/api/status?token=anything`
   应返回 `502 session_unavailable`（说明 axum 起来了、路由认得，只是没注册过 `SANITY` 会话）。

5. **校园网内可达性**：从另一台校园网内的机器 `curl http://<relay-host>:8080/...`
   应能连通。**不需要公网 IP**——v1 不上 TLS，纯校园内 HTTP/WS。

### 7.3 导播端开演前：填 Relay 配置

1. 打开 **StageDeck Remote → Settings**。
2. 在 **Relay** 段填三项：
   - **base_url**：`ws://<relay-host>:8080`（注意是 `ws://`，不是 `http://`，因为 director 与 relay
     之间走 WebSocket；二维码上的 `http://` 是手机访问 relay 的方向，由系统自动算出来）。
   - **session_id**：自定的短码，正则 `[A-Z0-9-]{4,16}`，比如 `PROM-2026`、`OPEN-MIC-3`。
     这就是手机扫码 URL 里的 `<sid>`。
   - **secret**：预共享密钥（你自己定的一串字符）；**关键**——防别人占用同一个 session_id。
3. **保存**。这个改动需要**重启导播应用**才能生效（启动时才会去连 relay）。

### 7.4 开演流程（Relay 模式）

1. 路由器 / 场馆 WiFi 该启的启，导播 PC **接入校园网**（任何途径都行：有线 + VPN、场馆 WiFi 出口可达校内）。
2. 启动 StageDeck Remote → 看 Settings 里 **Relay 状态** 变成 `connected`（绿色●）；不连绿就回 §7.5 排障。
3. 二维码面板上切到 **Relay** 模式，让它生成形如
   `http://<relay-host>:8080/r/PROM-2026/presenter?token=xxxx` 的码。
4. 主持人手机扫这个码（手机也要进校园网；可走校园 WiFi 或校园 VPN）。
5. 自检：让主持人**释放控制**，点一下预览图、翻一次页。

### 7.5 Relay 模式排障

| 症状 | 可能原因 | 处理 |
| --- | --- | --- |
| Settings 里 Relay 状态卡在 `connecting` | base_url 写错 / relay 不在校园网可达 / 防火墙 | 在导播 PC 上 `curl http://<relay-host>:8080/` 看通不通；不通先回 §7.2 第 4-5 步 |
| 状态 `error: secret_mismatch` | 同一个 session_id 已被其他 secret 注册过 | 换 session_id，或在 relay 上重启服务清掉旧 session（v1 无持久化） |
| 状态 `error: bad_sid` | session_id 不符合 `[A-Z0-9-]{4,16}` | 重新填、保存、重启 |
| 状态 `connected` 但手机扫码 502 | relay 觉得没人在；多半是导播又掉了 / 二维码里 `<sid>` 拼错了 | 让导播看 Settings，确认还是 connected；二维码用 QR 面板生成、别手抄 |
| 状态 `connected` 但手机 504 | 隧道 30s 内导播没回响应（导播在线但卡住,典型是 VBA 弹窗） | 导播看 PPT 有无弹窗；切回 LAN 模式或重启导播 |
| 手机短暂 502 后自己好了 | 导播↔relay 链路半开被 25s 存活探测判死、重连成功 | 不用处理；导播日志面板会有"Relay 控制通道断开：25s 无心跳"→"已重连（第 N 次尝试）"两条 |
| 导播日志反复出现"Relay 控制通道断开 … 后重连" | 导播 PC 到 relay 的网络在抖 | 换网线 / 换出口；relay 侧 `journalctl -u stagedeck-relay` 看对应 `liveness_timeout` |
| 画面突然停了一阵 | 推流 WS 断；导播侧自动重连 | 等几秒；本地一般 ≤5s 重连。如果反复断，看 relay 与导播之间的网络抖动 |

### 7.6 关于 Relay 和 LAN 模式并存

- 两种模式**软件层完全并存**，看二维码 URL 指哪去。开演前可以临场决定走哪条。
- LAN 模式无需 relay 在场；Relay 模式无需带路由器/AP。
- 同一份主持人页源码，不感知部署模式——所有 API 都走相对路径，由 `<base href>` 锚定。
  设计层面见 [`architecture.md` §11](architecture.md)。
