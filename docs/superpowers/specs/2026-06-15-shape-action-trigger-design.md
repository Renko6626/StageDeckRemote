# 形状级动作触发(Shape-Action Trigger)设计

**日期:** 2026-06-15
**状态:** 待审阅
**关联:** 修正 [`2026-06-15-com-toolchain-design.md`](./2026-06-15-com-toolchain-design.md) 里「按宏名触发」的两个缺陷。

---

## 1. 问题(Why)

运行时验证(用户在 Windows 上跑 `quiz_template.pptm` 第三页 = 多选题)发现:

- Reset、Check 两个按钮经我们的工具链可以正确触发;
- **选项按钮(8 个 `OptT_*` / `OptF_*`)无法触发**,报错:
  `触发宏失败:Run: 调用失败(Application.Run : Invalid request. Sub or function not defined. (0x80020009)）`
- 这些选项只有用鼠标手点才会亮。

### 1.1 根因:共享处理器需要「被点的形状」作参数

模块1(多选题)三类按钮的 VBA 签名差别就是全部原因:

| 按钮 | VBA 签名 | 我们当前调用 | 结果 |
|---|---|---|---|
| Reset | `Sub BtnReset()` | `Application.Run("BtnReset")` | ✅ |
| Check | `Sub CheckAnswer()` | `Application.Run("CheckAnswer")` | ✅ |
| 选项 | **`Sub ToggleOption(shp As Shape)`** | `Application.Run("ToggleOption")` | ❌ |

`ToggleOption` 必须收到被点的那个形状(它要读/写 `shp.Tags("Selected")`、改 `shp.Fill`/字体)。
**鼠标手点能亮**,是因为 PowerPoint「运行宏」动作的约定:当宏签名是单个 `As Shape` 参数时,
PPT 会把被点的形状自动塞进去。我们 `Application.Run("ToggleOption")` 没带这个参数,自动化层
无法把调用绑定到该签名,于是抛 `DISP_E_EXCEPTION (0x80020009)`。无参的 Reset/Check 因此一直正常。

### 1.2 连带缺陷:模型分不清「哪个选项」

第三页 8 个选项形状全部绑同一个宏 `ToggleOption`。我们现在以「宏名」为动作标识触发
(`trigger_action(macro_name)`),即便修好传参也说不出主持人点的是哪个选项 —— 同名宏的多个
按钮彼此不可区分。

### 1.3 通用性

「一个共享 `Sub Foo(shp As Shape)` + 多个按钮绑同一宏」是交互式 PPT 模板的常见写法(免去逐
按钮写宏)。本设计要让工具链对这种模式通用,而非只为本 pptm 打补丁。

---

## 2. 方案(What):按「形状」触发,复刻一次点击

核心:把动作的标识从「宏名」改成「形状」。主持人点按钮时传形状标识;后端据此重新定位形状,
**服务端自己读出该形状绑定的宏名**(不信任前端传的宏名),再 `Application.Run(宏名, 形状对象)`
把形状塞回去 —— 等价于复刻一次鼠标点击。对无参宏带形状会失败,**带参失败即退回无参重试**。

用户已拍板采用「COM 传形状」而非「模拟点击形状坐标」:不动鼠标、不依赖放映窗口在前台、投影
画面上不会出现光标乱跑。

### 2.1 形状标识 = 形状名(Shape.Name)

每个动作带上形状名(如 `OptT_1`)作为稳定标识:

- 形状名在一页内稳定、可读;PowerPoint 总会给形状分配名字。
- 用 `Shapes.Item(name)`(`Item` 接受名字字符串)重新定位 —— **对形状增删导致的索引漂移免疫**。
  这点对连线题页(模块2/3)很重要:那里 `Click_Right` 会新增连线形状、`Click_Back`/`ResetAllShapes`
  会删除形状,索引会变;按名字定位则不受影响。
- 万一同名(罕见):`Item(name)` 取第一个,可接受。

### 2.2 带形状调用,失败退回无参

```
Application.Run(macro, shape)   ← 先试带形状
  └─ 失败 → Application.Run(macro)   ← 退回无参
```

**为什么安全(不会双跑):** `Application.Run` 在进入宏体之前先做形参/实参个数校验。给无参宏多传
一个实参 → 在「绑定」阶段就报错 450(参数数目不符),宏体根本不执行 → 退回无参再跑一次,宏体恰好
执行一次。给需参宏(`ToggleOption`)带形状 → 正常绑定、执行一次,不触发退回。唯一的理论双跑边角是
「`Optional shp As Shape` 且宏体在带形状路径里中途抛错」—— 本模板不存在,列为已知局限。

### 2.3 安全不变量(强化)

`trigger_action` 仍只允许触发「当前页、且确为运行宏动作」的形状,且**宏名由服务端从形状现读**,
不采用前端传入的宏名:

1. 取当前页(放映视图优先,否则编辑视图);
2. 在当前页 `Shapes.Item(shape_id)` 定位;定位不到 → 拒绝(`shape_not_on_current_slide`);
3. 读 `shape.ActionSettings(ppMouseClick=1)`,校验 `Action == 8`(ppActionRunMacro);否则拒绝;
4. 宏名 = `ActionSettings.Run`(服务端现读);
5. 带形状调用、失败退回无参。

相比旧实现,攻击面更小:客户端最多能触发「当前页某个本就可点的形状所绑定的宏」。

---

## 3. 接口与数据结构变更

### 3.1 `SlideAction`(`src-tauri/src/ppt_com.rs`)

```rust
#[derive(Serialize, Clone, Debug)]
pub struct SlideAction {
    pub id: String,         // 新增:形状名(Shape.Name),触发标识
    pub label: String,      // 显示用:可见文字优先,空则形状名(不变)
    pub macro_name: String, // 显示/日志用:绑定的宏名(不变;触发时服务端重新读取)
}
```

`slide_actions()` 枚举形状时已有 `shape` 句柄,新增读取 `Name` 作为 `id`;`id` 为空则跳过该形状。

### 3.2 `trigger_action`(`src-tauri/src/ppt_com.rs`)

签名由 `trigger_action(macro_name: &str)` 改为 `trigger_action(shape_id: &str) -> Result<String, String>`,
返回实际触发的宏名供日志。在**单个 `ComGuard` 作用域**内完成 §2.3 的 1–5 步(不再二次
`slide_state()` 重连)。新增内部 `run_macro_with_shape(app, macro, shape)` 实现 §2.2。

`run_macro(name)`(无参 `Application.Run`)保留,供回退路径与潜在其它调用复用。

**FFI 注意点(实现时验证):** 需要把形状的 `IDispatch` 包成 `VT_DISPATCH` 的 `VARIANT` 作为
`Application.Run` 的第二个实参。优先用 `windows::core::VARIANT::from(idispatch)`;若 0.58 无此
`From` 实现,则手工构造 `VT_DISPATCH` 变体(设置 `Anonymous.Anonymous.vt = VT_DISPATCH`、
`pdispVal`,并保证 AddRef/Release 平衡)。现有 `invoke()` 的实参反序逻辑对两参 `[macro, shape]`
天然正确(COM 约定实参逆序)。此点以 `cargo xwin` 交叉编译 0 错误为准。

### 3.3 路由(`src-tauri/src/routes.rs`)

`ActionReqBody` 字段 `r#macro: String` → `shape: String`。`action` 处理器:token + `click_allowed`
门控不变;改调 `trigger_action(&req.shape)`;日志含形状名与返回的宏名(如 `触发 OptT_1 → ToggleOption`)。

### 3.4 前端(`src/presenter/presenter.js`)

- `renderActions`:按钮 `data-id = a.id`(形状名)、`data-macro = a.macro_name`(右上角小标注,不变);
  `lastActionsKey` 纳入 `a.id`(防同名宏按钮被并掉)。
- 动作点击处理:`api("/api/action", { shape: id })`(原 `{ macro }`)。

### 3.5 类型(`src/utils/api.ts`)

`SlideAction` 增加 `id: string` 字段。导播端 `ComStatusPanel` 不触发动作,无需改动。

---

## 4. 测试

- **纯逻辑(Linux):** 现有 `parse_macro_names` 单测保持;COM/FFI 部分跨平台不可单测。
  全量 `cargo test`(tauri-dev 环境)应维持既有 17/17 绿。
- **编译验证(Linux→Windows):** `cargo xwin build --target x86_64-pc-windows-msvc` 0 错误,
  确认 `VT_DISPATCH` 实参构造与两参 `Run` 调用编译通过。
- **运行时(用户 Windows):** 放映 `quiz_template.pptm` 第三页 → 主控页点选项按钮应能选中/取消选中
  (变黄加粗),Reset/Check 仍正常;第二/四页连线题按钮(`Click_L1`…`Click_Back`,均无参)仍正常。

## 5. 不做(YAGNI)

- 不引入 EXCEPINFO 精细判别错误号来决定是否回退(§2.2 已论证当前宏集合下安全);列为后续硬化项。
- 不支持非「运行宏」动作类型(超链接/跳页);本工具链只枚举并触发 RunMacro 动作。
- 不改导播端 IPC 行为与三层锁模型。

## 6. 影响文件

- `src-tauri/src/ppt_com.rs` — `SlideAction.id`、`trigger_action` 改形状触发、`run_macro_with_shape`。
- `src-tauri/src/routes.rs` — `ActionReqBody.shape`、`action` 处理器与日志。
- `src/presenter/presenter.js` — 按钮 `data-id`、动作请求体 `{shape}`、`lastActionsKey`。
- `src/utils/api.ts` — `SlideAction.id`。
