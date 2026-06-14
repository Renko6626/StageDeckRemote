# VBA spike · Windows 手动验收

前置：
1. PowerPoint 信任中心 → 宏设置 → 勾选「信任对 VBA 工程对象模型的访问」。
2. 准备一个 `.pptm`，含几个公开命名 Sub（最好有可观察效果，如跳页/MsgBox）。（测试宏请放在标准模块 Module1 里；类模块 ThisPresentation/SlideN 里的宏需 "模块名.宏名" 限定名，本 spike 用非限定名。）
3. 出 debug 安装包：`tauri build --debug --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis`，安装。

验收：
1. 打开那个 `.pptm`（PowerPoint 保持运行）。
2. 启动 StageDeck Remote，导播页点「探测 VBA 宏」→ 列出宏名（= VBProject 访问通了）。
3. 点某个宏 → 该宏真的执行（跳页/弹框）（= Application.Run 通了）。
4. 反例：关掉信任设置再探测 → 弹「请开启信任设置」提示（错误映射正确）。
5. 反例：不开 PowerPoint 就探测 → 弹「PowerPoint 未运行」提示。

结论：2+3 通过 = 优雅路线成立，可进入完整 COM 工具链增量；否则记录卡点。
