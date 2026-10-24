# Windows 本地开发与快速迭代

本文用于在真实 Windows 电脑上调试 PowerPoint COM、窗口捕获和模拟输入。当前发布基线为 **v0.3.1**；GIF 问题的现象、证据和接手位置见 [排查交接记录](debugging/2026-10-24-gif-preview.md)。

## 1. 一次性准备

使用 Windows 原生 PowerShell 和 MSVC Rust 工具链。WSL 可以编辑代码，但不能替代真实 Windows 下的 PowerPoint/WGC 验证。本仓库 Linux 开发机使用的 `tauri-env`、`cargo-xwin`、Conda 路径不适用于这里；以下为 Windows 本机构建命令。

安装：

- Git、Node.js LTS、Rust stable（通过 rustup 安装）。
- Microsoft C++ Build Tools，勾选 **使用 C++ 的桌面开发**，包含 MSVC x64 工具和 Windows SDK。
- Microsoft Edge WebView2 Runtime；系统已有时无需重装。
- 桌面版 Microsoft PowerPoint，以及待复现的文稿和相关字体。

C++ 与 WebView2 的要求以 [Tauri 官方 Windows 前置条件](https://v2.tauri.app/start/prerequisites/#windows) 为准。安装完成后重新打开终端。

```powershell
node --version
npm install --global pnpm@9
pnpm --version
rustc --version
cargo --version
```

## 2. 拉取和启动

```powershell
git clone https://github.com/Renko6626/StageDeckRemote.git
Set-Location StageDeckRemote
git switch master
git pull --ff-only
rustup override set stable-x86_64-pc-windows-msvc
pnpm install --frozen-lockfile
pnpm tauri dev
```

已有工作目录时只需先检查 `git status`，保存自己的修改后再拉取。首次编译需要下载并编译 Rust 依赖，后续增量构建会快很多。

`pnpm tauri dev` 会启动 Vite 和桌面应用。仅执行 `pnpm dev` 打开的普通浏览器页面缺少 Tauri IPC，不能测试捕获和 COM。

启动前完全退出已安装的 StageDeck，避免两个实例抢占默认端口 8787。PowerPoint 可以一直保持打开。若 PowerPoint 以管理员身份运行而开发版没有，模拟输入可能受权限限制；通常让两者都以普通用户运行即可。

先用局域网直连测试：

1. 若已有 Relay 配置，停用后重启导播；启动日志应为 `relay_enabled=false`。只切换二维码不会停止 Relay。
2. 打开放映并选择对应放映窗口，开启远程控制。
3. 手机与电脑处于同一局域网，扫码后释放控制。Windows 防火墙允许应用在当前受信任的局域网接收连接，详见 [网络手册](network.md)。
4. 需要观察屏幕同步时切到手机的 **屏幕 / SYNC**。

每次桌面进程重启都会生成新访问码。旧手机页可能显示 401 或访问码已更改，重新扫码即可；不要把它与 GIF 故障混为一谈。

## 3. 哪些修改需要重启

| 修改位置 | 生效方式 |
| --- | --- |
| `src/**/*.vue`、普通 TS/CSS | Vite 通常热更新；观察桌面窗口即可 |
| `src-tauri/src/*.rs` | 等待 Tauri 重新编译和启动；不确定时 `Ctrl+C` 后重新运行 |
| `src/presenter/presenter.js`、`.html`、`.css` | 手机页面经 Rust `include_str!` 内嵌；停止并重启 `pnpm tauri dev`，重新扫码、刷新手机页面 |
| Relay 提供的手机页面 | 页面内嵌在 Linux Relay 中；修改后需重新构建并重启对应 Relay，再刷新手机。局域网迭代不需要 Relay |

手动翻页后，页码/宏按钮需要等下一轮 COM 扫描；画面来自独立帧推送，不受这个扫描间隔控制。v0.3.1 导播扫描每 2.5 秒一次，手机正常约 2.7 秒，心跳每 0.9 秒；手机操作结束会立即请求一次状态。

## 4. 调试速度与性能比较

日常定位用 `pnpm tauri dev`。需要比较编码耗时、CPU 或画面流畅度时，改用优化后的 Rust 构建运行：

```powershell
pnpm tauri dev --release
```

此命令仍用于本地运行，不产生安装包，不需要每次修改版本号。Debug 和 release、开启和关闭详细诊断的耗时应分别记录，避免直接混比。

按改动范围选择已有检查，不要求每次全量运行：

```powershell
pnpm typecheck
pnpm exec vitest run src/presenter/presenter.spec.ts
cargo test -p stagedeck-remote --lib
```

正在运行的桌面进程可能占用 Windows 可执行文件；遇到链接或文件占用错误，先停止开发进程再重试。真实 WGC/PowerPoint 行为仍需要人工复现，上述测试不代替实机验收。

只有确实要分发安装包时才使用 `pnpm tauri build --bundles nsis`；按项目约定先更新并提交版本号。当前 GIF 迭代无需反复制作安装包或触发 GitHub Release。

## 5. 日志和故障时快照

在导播日志面板开启 **详细诊断**，每次进程重启后重新开启。用 **打开日志目录** 找到 `diagnostics.jsonl`；及时复制日志和相关历史文件，避免轮换覆盖。每次测试记录文稿、页码、时间、屏幕分辨率/缩放、是否全屏及当前提交：

```powershell
git rev-parse --short HEAD
```

下面的命令仅用于 **局域网直连**。在 GIF 页预览冻结时，手动取得现有快照接口的输出，保持文稿停在故障页：

```powershell
New-Item -ItemType Directory -Force .\temp\gif-debug | Out-Null
$presenterUrl = Read-Host '粘贴当前局域网主持人访问地址（含 token）'
$presenterUri = [Uri]$presenterUrl
$directorOrigin = $presenterUri.GetLeftPart([UriPartial]::Authority)
Invoke-WebRequest -UseBasicParsing -Uri "$directorOrigin/api/snapshot.jpg$($presenterUri.Query)" -OutFile .\temp\gif-debug\snapshot-gif.jpg
```

`snapshot.jpg` 绕过变化检测和 broker，直接编码最新原始帧；原始帧为空时还会尝试 BitBlt/占位图。它不会强制 WGC 再捕获一帧，因此旧快照仍不能单独区分“没有回调”和“取帧被阻塞”。具体判读见交接记录。

## 6. 测试文件需要另外带过去

本次 Linux 工作目录中的 PPTM、诊断 JSONL 和 `temp/` 样本均未提交；`git clone` 不会取得它们。将需要的文件自行复制到 Windows 测试目录：

- 实际使用的 97 页文稿及其仅移除 GIF 的对照版本。
- `gif_pptm.pptm`：用户从问题页提取的两页独立样本。
- `diagnostics_1024.jsonl`、`diagnostics_1024_nogif.jsonl`：已有对照日志。

两页样本保留了宏绑定，但包内没有 `vbaProject.bin`；它适合比较捕获和扫描，不适合单独验收宏执行。宏点击测试使用能在电脑上正常执行宏的完整版。

不要把文稿、带访问地址的记录、日志、`target/` 或 `node_modules/` 一起加入提交。后续代码迭代在 Windows 本地完成后，正常提交源代码即可。
