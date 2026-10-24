# PowerPoint 状态探测修正计划

设计：../specs/2026-10-24-ppt-probe-wait-design.md

- [x] ppt_com.rs：添加短暂占用的回归测试，确认原实现失败；状态探测改为在现有信号量上最多等待 1 秒。
- [x] DirectorPreview.vue：扫描间隔改为 2500ms。
- [x] presenter.js/spec：扫描与心跳频率分离，扫描最小间隔 2500ms；操作完成强制刷新。验证扫描间隔内权限仍及时更新。
- [x] 集中验证：Rust 导播库测试、手机页面测试、前端构建、Relay phone_routes 测试、Windows 交叉编译；检查最终 diff。

约束：不增加缓存、依赖或新接口，不调整 Relay 生命周期，不自动操作浏览器，不生成安装包。Windows 编译不能代替真实 PowerPoint 运行验收。

验证结果：`tauri-env linux cargo test -p stagedeck-remote --lib` 61 通过；`pnpm exec vitest run src/presenter/presenter.spec.ts` 18 通过；`pnpm build` 通过；`tauri-env linux cargo test -p relay phone_routes::tests` 5 通过；从 src-tauri 运行 `tauri-env win cargo xwin build --target x86_64-pc-windows-msvc` 通过，保留原有 RelayHandle 未使用字段警告。`git diff --check` 通过。独立只读审阅未发现阻断问题；未运行真实 Windows PowerPoint，未构建安装包或部署 Relay。
