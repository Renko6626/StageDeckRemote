# PPT 动作识别与 VBA 兼容性待办

首次记录：2026-10-13；最近核对版本：0.2.7。用户决定先导出当前版本，以下通用兼容性优化暂缓。后续按通用能力设计；文件名、页码、形状前缀和哈希只用于样本定位，不作为产品分支条件。涉及动作标识、协议、模块边界或依赖的调整，实施前另行确认。

## 当前运行逻辑

1. 经 COM 读取 PowerPoint 第一个放映窗口的当前页；没有放映窗口时读取编辑页。截图窗口与受控文稿尚未严格关联。
2. 遍历顶层 Shapes，筛选 `Shape.ActionSettings(ppMouseClick).Action == ppActionRunMacro`；不递归组合，不读取文字片段动作。
3. 返回形状名称 `id`、自身文字（无文字则用形状名）`label`、绑定的 `macro_name`。不核实过程存在性、签名、遮挡或玩法语义。
4. 手机按枚举顺序排成两列，保留文字大小写，但不恢复 PPT 位置、图片和样式。
5. 点击请求携带形状名，服务端重新查当前页绑定，先传 Shape 调用；仅 `DISP_E_BADPARAMCOUNT` 允许无参回退。8 秒超时表示结果未知，后台调用与忙碌保护仍保留。
6. 0.2.7 的主动 COM 操作最多等待通道释放 500ms，手机发送操作期间暂停新探测。`409 ppt_busy` 表示本次宏未执行；`409 op_in_flight` 表示同一操作仍在执行。此修复未修改文稿、动画或宏绑定。

源码入口：`src-tauri/src/ppt_com.rs`、`src-tauri/src/macro_execution.rs`、`src-tauri/src/routes.rs`、`src/presenter/presenter.js`。

另有 `list_macros` 逐行提取公开 Sub/Function 名称的辅助逻辑，未参与主持人按钮列表生成；本轮离线解包/VBA 提取工具也没有接入产品运行链路。

## 已检查样本

| 项目 | quiz_template.pptm | 10.13测试.pptm |
| --- | --- | --- |
| 页数 | 6 | 51 |
| VBA | 3 个标准模块，483 行原始导出 | 2 个标准模块 + CMatchingEvents 事件类，939 行 |
| 动作 | 28 个形状宏动作 + 2 个文字宏动作 | 23 个形状宏动作，无文字宏动作 |

新样本 SHA-256：`e936991031374efda0e9b71c2921a0387553ebfa35a83b2eaedbafa3ab6a266e`。原始 PPT 未修改。

新样本的 23 个动作分为 12 个 Shape 参数动作（8 MQ_Choose + 4 ToggleOption）、4 个无参动作、7 个指向本工程缺失过程的动作。

第44页的 Bottom3、Top3、Bottom1、Top2、Bottom2、Top1、Top4 还绑定旧 Click_R3、Click_L3、Click_R1、Click_L2、Click_R2、Click_L1、Click_R4；同时存在 8 个有效 `MQ_Hit_* → MQ_Choose` 点击框。程序会把底层残留动作和透明点击框一起列出。底层显示 A/a 等文字，真正的点击框没有文字而显示内部名称。

MQ 实现通过事件类处理放映事件，并在首次 MQ 动作时懒初始化。初始化会删除并重建点击框；MQ_Choose 已先保存 clicked.Name，再初始化，避免继续访问被删对象。程序不应猜测初始化宏名或自动运行这类宏。

新样本有 5 个 MsgBox 调用点，错误处理关闭弹窗后可能正常返回；COM 返回正常不保证玩法操作成功。模态等待、错误码及 COM 参数封送尚未在 Windows PowerPoint 验证。

新样本唯一组合仅含图片，没有组内动作；20 类跨页重复形状名均无动作。不能把组合漏识别和跨页同名误点写成此文件已复现的问题。

旧样本证据更正：第4页 back 的**形状**绑定 ResetGame，但**文字片段**绑定模块2.Click_Back；两层不一致。首轮报告把二者都描述成 ResetGame 不准确。第5页形状与文字原本均为未限定 Click_Back。

### thfg18_quiz_VBA+动画.pptm

2026-10-18 离线核对：10 页、2 个标准模块及 `CMatchingEvents` 事件类。检查了 ZIP 完整性、XML 动作/动画绑定及导出的 VBA，原件未修改，未运行 Windows PowerPoint。

- 第 3、4、6、7、8 页共 18 个形状绑定 `AB_RelayClick`，各自都能找到唯一的 `AB_Click_<形状名>` 目标，未发现这些桥接目标缺失或同名重复。
- `AB_RelayClick` 调用 `AB_Fire`，按目标形状的布局中心计算窗口坐标，再用 `PostMessage` 投递鼠标移动、按下和抬起消息。该路径仍依赖放映画面的点击命中，不是直接播放指定动画。
- 第 6、8 页的目标形状配置了退场动画，桥接宏未核验动画播放后的实际命中状态，手机仍会列出桥接按钮。重复点击原坐标可能命中背景或其他对象，进而触发翻页；文件机制已确认，用户所遇误翻页仍需 Windows 实机核验。
- 第 9 页保留 7 个旧绑定：`Bottom3 → Click_R3`、`Top3 → Click_L3`、`Bottom1 → Click_R1`、`Top2 → Click_L2`、`Bottom2 → Click_R2`、`Top1 → Click_L1`、`Top4 → Click_R4`。本文件 VBA 中没有这些过程，同时存在 8 个有效 `MQ_Hit_* → MQ_Choose` 点击框，当前软件会一起枚举。
- `AB_Fire` 捕获异常后弹出 `MsgBox` 并正常返回，未向调用方重新抛出错误。点击消息投递成功也未核验动画结果，因此软件日志中的调用成功不保证动画成功。
- 多选与连线音效从 PPT 同目录的 `sound/SE_Bingo.wav`、`sound/SE_Miss.wav` 读取，不直接使用文稿内嵌音频。当前工作区无同级 `sound` 目录；实际演示电脑是否配齐需另行核对。

第 5 页多选宏用 `"Yes"` 比较 `Selected` 标签值，模块未声明 `Option Compare Text`；微软 [Tags 文档](https://learn.microsoft.com/en-us/office/vba/api/powerpoint.shape.tags) 提示使用大写值比较。此处记录为大小写疑点，尚未通过 Windows 实机确认其是否影响取消选中和判题，不将其写成已复现故障。

## 后续目标与验收点

- [ ] **动作发现与可调用性诊断。** 保留形状、文字、组合路径及动作类型；可核实的宏缺失/歧义/不支持签名给出具体原因。源码访问失败只能标记未核实，不能误判宏缺失；外部已加载宏也需考虑。默认使用不能强制开放 VBProject 访问权限。
- [ ] **按钮与可见内容关联。** 文字保留大小写，图片考虑缩略图，透明覆盖层关联其可见内容并处理重复候选。不能按 MQ_、Top、Left 等特定前缀猜题意。关联不明确时提供可辨识的信息，避免静默猜测；排序考虑视觉位置而非形状创建顺序。
- [ ] **稳定定位与过期动作拒绝。** 绑定文稿、SlideID、形状/组合位置及动作版本，执行前重新读取；覆盖翻页、多文稿、同名形状、删除重建和绑定变化。不长期缓存 COM Shape 对象。动作标识及协议方案待确认。
- [ ] **通用调用契约。** Windows 上用小型独立样例验证无参、Shape、Object、Optional/ParamArray 及其他必需参数的支持边界，核验 Application.Run 的参数数组封送和实际 HRESULT；先确定签名或能力，不靠重复执行猜参数。保留幂等和运行错误不重试的约束。
- [ ] **可信的状态与错误反馈。** 区分收到请求、执行中、调用结束、失败、结果未知。验证主动弹窗、VBA 捕获错误后正常返回、超时后迟到结果；不得把“调用返回”宣称为业务成功，也不擅自关闭全部 Office 弹窗。
- [ ] **跨样本验证。** 旧模板、新模板及与其命名无关的最小样例共同覆盖，确保改名、换页或换文件后仍适用。页面验证遵循用户约定，以截图为限；Windows 宏运行验收另行安排。

推荐先处理发现/显示与目标定位，再根据 Windows 调用证据调整执行器。上述事项未在当前导出版本中实施。

## 文档自身问题与产品边界

第44页失效旧绑定、VBA 的事件/弹窗/玩法逻辑属于文稿问题；产品应提供诊断，不自动重写为某个已知题型。`scripts/prepare_quiz_template.py` 仅是旧样本的局部修复工具，不作为通用算法；不得直接套到新文件。

官方参考：[形状宏参数约定](https://learn.microsoft.com/en-us/office/vba/api/powerpoint.actionsetting.run)、[Application.Run](https://learn.microsoft.com/en-us/office/vba/api/powerpoint.application.run)、[SlideID](https://learn.microsoft.com/en-us/office/vba/api/powerpoint.slide.slideid)。
