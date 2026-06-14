# WGC 捕获 · Windows 手动验收

前置：出 debug 安装包并安装：
`tauri build --debug --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis`

1. 选窗（记事本/画图/PowerPoint 放映）→ 手机看到该**窗口本身**的真实内容（非占位、非黑屏）。
2. **遮挡测试**：用另一个窗口盖住目标窗口 → 手机仍显示**目标窗口内容**（不是遮挡物）。
3. **移动测试**：拖动 / 改变目标窗口大小 → 手机画面仍正确跟随其内容。
4. **GPU 内容**：PowerPoint 放映（D3D 加速）→ 手机看到真实幻灯片（非黑帧）。
5. **回退**：选一个 WGC 无法捕获的目标 / 旧系统 → 自动回退 BitBlt，不黑屏。
6. Win11 上目标窗口**无黄色捕获边框**；Win10 上有边框但**手机快照里无边框**。

任一不过：记录现象。WGC 起不来 → 看日志 `WGC 启动失败`，确认回退到 BitBlt；
帧错位/花屏 → 检查 `as_nopadding_buffer` 紧排与 `ColorFormat::Bgra8`。
