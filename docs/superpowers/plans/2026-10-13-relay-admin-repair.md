# Relay / admin 修复实施计划

> 执行：主 agent 按 executing-plans 原地实施，用户指定的 VBA subagent 负责独立产物。测试范围遵循用户约定。

**Goal:** 修复审查复现的 Relay 画面、连接接管、页面输入和设置一致性问题。

**Architecture:** 保留现有模块和协议；用现有控制 channel 身份判定连接归属，推流连接在 session 内记录当前归属。持久化设置与启动时运行配置使用现有磁盘/内存路径各自承担职责。

**Tech Stack:** Rust/Axum/Tokio、Vue 3、内嵌 HTML/CSS；不新增依赖。

**Spec:** ../specs/2026-10-13-relay-admin-repair-design.md

## 约束与审查重点

- 保留用户的 CLAUDE.md 改动、原始 PPTM，不提交 target/。
- 同步锁的顺序一致；接管、断线、请求注册不得跨连接误清数据。
- 静止画面在无订阅者和重连时仍可见；旧推流不得覆盖新画面。
- 保存失败保持当前运行和磁盘配置；界面不能宣称保存成功。
- SID 校验覆盖 presenter HTML 入口；状态页面展示可读文字和移动布局。

## Task 1: Relay 数据与连接可靠性

Files: crates/relay/src/{session,director_ws,phone_routes}.rs、crates/relay/tests/end_to_end.rs、src-tauri/src/relay_client.rs。

- [x] 在现有 loopback 测试中替换被忽略的空推流测试，补晚加入/接管和非法 SID 回归；先确认失败。
- [x] 修复最新帧保留、归属校验与退出清理，导播发送初始帧。
- [x] 运行相关 Relay 和导播测试。

## Task 2: 设置保存和二维码一致性

Files: src-tauri/src/{commands,config,relay_client}.rs、src/components/SettingsModal.vue。

- [x] 让读取待生效设置走磁盘、保存不覆盖运行配置；返回校验/写盘错误。
- [x] 补必要真实配置行为回归；更新设置反馈文案。

## Task 3: admin 可读性

Files: crates/relay/src/admin.rs、docs/network.md。

- [x] 添加 viewport、中文状态文字和手机布局，保留只读语义；补充访问入口说明。
- [x] 使用现有渲染检查和桌面/手机截图；不新增纯样式测试。

## Task 4: VBA 交付与综合验证

- [x] 接收 subagent 的独立脚本/报告/候选包，验证原件不变和候选差异。
- [x] workspace Rust 测试、pnpm test/build、Windows 交叉编译。
- [x] 独立审查实际 diff，解决重要问题，记录验证与未决项；保留修复分支。

## 完成记录（2026-10-13）

- Relay 新增4个回归先失败后通过：晚加入首帧、推流接管、控制接管、非法SID。占位推流用例已替换为真实 loopback 路径，当前无忽略用例。
- 审查补充：删除过期会话前重查当前连接；配置编辑使用可返回错误的读取，损坏文件不会被默认值覆盖；现有测试扩展对应断言。
- `tauri-env linux cargo test --workspace --offline`：80项通过，0失败、0忽略；最后调整配置测试调用后单独重跑config测试。
- `pnpm test`：56项通过；`pnpm build`：类型检查和构建通过。此环境用 `/data/sunyunbo/.nvm/versions/node/v22.22.1/bin/pnpm`。
- `cd src-tauri && tauri-env win cargo xwin build --offline --target x86_64-pc-windows-msvc`：通过，既有 RelayHandle 未使用字段警告1项。
- admin 使用真实 Relay 输出截图：1280桌面、390手机均无横向溢出；页面未执行自动点击或表单操作。
- `python3 scripts/prepare_quiz_template.py --output /tmp/sunyunbo/quiz-template-fixed-root-verification`：原件及VBA未变、候选仅slide5.xml改变、音效及ZIP完整性通过；正式候选包位于out/quiz-template-fixed.zip。
- 独立代码审查完成，两项Important均修复后复核通过。
- 未验证 Windows PowerPoint 运行和真实场馆网络；未制作安装包。第4页back含义、跨页重置/多重连线规则及模块合并待用户确认。
