# 题目模板 VBA 与局部修复

本目录保存 `quiz_template.pptm` 中导出的原始源码，便于阅读和后续维护。本轮只修第5页 back 的宏绑定并补齐外置音效，未修改 VBA 或玩法。原始 PPTM 不纳入版本管理。

## 生成修复包

在项目根目录执行（Python 3.9+，只用标准库）：

```bash
python3 scripts/prepare_quiz_template.py
```

输出为 `out/quiz-template-fixed/` 和 `out/quiz-template-fixed.zip`。目录内包含：

- `quiz_template-fixed.pptm`：第5页形状和文字的 back 点击动作均指向 `模块3.Click_Back`。
- `sound/SE_Bingo.wav`、`sound/SE_Miss.wav`：从第2页内嵌音频原样导出，供 VBA 按相对目录读取。
- `verification.json`：原件与候选哈希、变更条目和音频校验记录。
- `README.txt`：修复范围及尚未解决的问题。

脚本拒绝覆盖输入、已有输出目录或已有 ZIP。再次生成可指定新的目录：

```bash
python3 scripts/prepare_quiz_template.py quiz_template.pptm --output /tmp/quiz-template-review
```

输入必须是已审查的版本，SHA-256 为 `b44a19f91ac6910cff31246120fa0ecb98b4836991a86dba720d8a1258b6763f`。用户编辑过的模板需要重新核对绑定，脚本会拒绝直接套用。

脚本检查 ZIP CRC、候选条目、音效内容和原件哈希。候选仅 `ppt/slides/slide5.xml` 改变，其他80个条目（包括 `vbaProject.bin`）逐字节保持一致。ZIP 检查不能证明 PowerPoint 宏已运行通过。

## 原始源码与状态

全部3个标准模块均已导出，和 OLE 工程中的模块清单一致；共483行、36个过程，没有类模块或窗体。源码为 UTF-8 阅读副本，已去除行尾空白及文件末尾多余空行，不应直接替换模板中的 VBA。导出工具为 oletools 0.60.2；生成修复包不依赖它。

| 源码 | 题型与入口 | 状态 |
| --- | --- | --- |
| [模块1.bas](original/模块1.bas) | 多选；`ToggleOption(Shape)`、`CheckAnswer()`、`BtnReset()` | Shape 的 `Selected` 标签 |
| [模块2.bas](original/模块2.bas) | 左右连线；`Click_L1..4()`、`Click_R1..4()`、`Click_Back()` | 模块级 `selectedLeft`、`Line_LeftN_RightN` 形状和颜色 |
| [模块3.bas](original/模块3.bas) | 上下连线；`Click_T1..4()`、`Click_B1..4()`、`Click_Back()` | 模块级 `selectedTop`、`Line_TopN_BottomN` 形状和颜色 |

6页中共有28个形状级宏绑定。第3页8个 `ToggleOption` 接受 Shape，其余已有入口为无参宏；第4页另有一个缺失入口。微软允许形状动作绑定无参宏或一个 Shape/Object 参数的宏；`Application.Run` 支持模块限定名称。[形状动作](https://learn.microsoft.com/en-us/office/vba/api/powerpoint.actionsetting.run)、[Application.Run](https://learn.microsoft.com/en-us/office/vba/api/powerpoint.application.run)

## 未决项

新样本 `10.13测试.pptm` 与产品通用性优化统一记录在 [兼容性待办](compatibility-backlog.md)，本轮暂缓实施。

1. **第4页 back 的形状绑定不存在的 `ResetGame`，文字片段却绑定 `模块2.Click_Back`。** 两层绑定不一致，首轮报告把二者都写成 ResetGame 不准确。最终希望撤销一条还是清空本页仍待确认。模块2现有 `Click_Back`（153行）和 `ResetAllShapes`（53行）语义不同；后者参数为 Slide，不能直接当作 Shape 参数入口。
2. **跨页重置及选择状态未调整。** 三个模块都有 `OnSlideShowPageChange`；模块2/3还各有 `ResetAllShapes`，未确认目标 PowerPoint 版本实际调用哪个翻页入口。所选 Shape 引用没有检查是否属于当前页（模块2:100、模块3:103）。
3. **多重连线和撤销规则未调整。** 模块2/3的110行直接创建连接线，不查重；撤销无条件重置两端颜色。选择其他端点后再撤销，还可能出现高亮保留但选中引用已清空。需先明确玩法。
4. **错误处理与模块合并未调整。** 样式和撤销过程存在整段 `On Error Resume Next`，连线创建失败没有清理。需围绕具体失败做局部修复，模块合并另行确认。

## Windows 验收

本轮只做文件及源码静态检查，没有运行 PowerPoint。请单独打开候选文件验收，避免与原件同时加载造成同名全局宏解析混淆：

- 第5页连接后 back 应撤销上下连线，音效目录应随 PPT 一起移动。
- 分别验证原生点击、StageDeck 调用一个 Shape 参数宏和一个无参宏。StageDeck 只在 `DISP_E_BADPARAMCOUNT` 时无参回退；PowerPoint 实际返回的 HRESULT 仍须记录。
- 验证多选、判题、重置及两类连线；未完成选择时翻页再返回、重复连接后撤销应单独观察。

不要因文件检查通过就把此包视为全部玩法问题已修复的正式验收版。
