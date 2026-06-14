# 最短闭环 · Windows 手动验收

前置：`pnpm install`；`cd src-tauri && cargo test` 全绿。

1. `pnpm tauri dev` 启动；导播 GUI 点"枚举窗口" → 出现真实窗口列表。
2. 打开记事本/画图，回到 GUI 选中它 → ControlStatus 显示"已选择"。
3. 手机扫码打开主持人页 → 预览图显示该窗口**真实画面**（非占位图）。
4. 导播 GUI 开启远程控制；手机点"释放控制"。
5. 手机点击预览图上某个可见控件（如画图工具按钮）→ 该控件被真实点击。
6. 手机点"下一页/上一页"（用 PowerPoint 放映验证）→ 翻页生效。
7. 点击后导播电脑鼠标回到原位。
8. 导播 GUI 关闭远程控制 → 手机点击/翻页失效。

任一不过：记录现象，回到对应 Task 修正
（截图黑屏 → 见 spec §8 风险，可升级 WGC；翻页不生效 → `SetForegroundWindow` 受限，考虑 `PostMessage` 兜底）。
