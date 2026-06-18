# 主持人页:计时移入大字栏(方案 A)

**日期:** 2026-06-16
**状态:** 已批准(精简流程:spec → inline 实现)
**范围:** 仅 `src/presenter/{presenter.html,presenter.js,presenter.css}`(vanilla,经 Rust `include_str!` 嵌入,改后需重编)。不碰服务端/导播侧计时逻辑,不改 `m:ss`/超时语义,不动其它模式。

## 问题
计时现在是状态栏里一个很小的 chip `计时 [m:ss]`(`#timer-stat`/`#timer-val`),太小看不清。

## 方案 A —— 计时接管 `readout` 大字栏
`<section class="readout">`(现为 `SLIDE [大字] / [total] [IDLE/LIVE]`)按是否有计时切换:
- **有计时**:大字位 `#slide-big` 显示计时 `m:ss`(超时 `+m:ss` 转红);左标签 `SLIDE`→`计时`;隐藏 `/ total`;`#show-flag`(IDLE/LIVE)保留。幻灯片号仍在状态栏 `PG 03/12`。
- **无计时**:恢复成 `SLIDE 03 / 12`(今日行为)。
- 删掉状态栏冗余的 `#timer-stat` chip(计时只在大字位出现),不增加任何高度。

## 实现要点
**presenter.html**
- `readout` 行:给 `SLIDE` 标签加 `id="readout-label"`;把 `/ <span id="slide-total">` 外层包一个 `id="slide-frac"` 以便整体隐藏。
- 删除状态栏的 `<span class="stat" id="timer-stat" hidden>计时 <b id="timer-val">…`。

**presenter.js**
- `el` 映射:删 `timerStat`/`timerVal`;加 `readoutLabel`(#readout-label)、`slideFrac`(#slide-frac)。复用 `slideBig`(#slide-big)、`showFlag`。
- 新增状态 `timerActive`(模块级 `var`,初值 false)。
- `handleTimer(t)`:
  - `!t`:`timerBase=null`、停 tick、`timerActive=false`、把大字栏切回幻灯片模式(标签 `SLIDE`、显示 `slideFrac`、去 `.over`),并立刻按当前 `state.com` 重绘幻灯片号。
  - 有 `t`:`timerActive=true`、起 tick、切到计时模式(标签 `计时`、隐藏 `slideFrac`)。
- `tickTimer()`:写 `el.slideBig.textContent = fmtTimer(live)`,并 `el.readout.classList.toggle("over", live < 0)`(给大字位上红;`readout` 元素加进 `el`,或 toggle 在 `slideBig` 上)。
- **关键协调**:`render()` 第 ~94 行会把 `slide-big` 写成幻灯片号——用 `if (!timerActive)` 守卫,计时进行时不让 `render()` 覆盖大字位/标签/frac。

**presenter.css**
- 加大字位超时红:`.readout.over .readout-v { color: var(--danger); }`(复用现有 danger 变量)。
- 删冗余的 `#timer-stat.over b` 规则(及 `#timer-stat` 相关样式,若有)。

## 验证
- `node --check src/presenter/presenter.js`;`pnpm build`(presenter.js 不被 vite 处理,但确认导播端无破坏)。
- 运行时(Windows/真机):有计时→大字显示 `m:ss`、超时转红;清计时→恢复 `SLIDE x/y`;切页时大字栏不被幻灯片号覆盖。

## 不做(YAGNI)
不加新行/不增高度;不改计时格式与超时语义;不动 MAIN/MSG/SYNC 其它内容;不碰导播或服务端。
