# 主持人端台上 UX 改版 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把主持人页重排成「竖屏单手台上控制台」(方案 C):贴底全宽巨钮「下一页」、翻页上行、双列大宏按钮、细 ARM 安全条;MAIN/SYNC 共用同一底部翻页坞。

**Architecture:** 纯前端,只动 `presenter.{html,css,js}` 三个内嵌静态文件;后端/数据契约/三层锁逻辑不变。三个文件是**一次协同重写**(HTML 的 id ↔ CSS 的 class ↔ JS 的选择器必须对齐),应用只有三者齐备后才正常工作。详见 `docs/superpowers/specs/2026-06-15-presenter-stage-ux-design.md`。

**Tech Stack:** 原生 HTML/CSS/JS(无构建);经 `routes.rs` 的 `include_str!` 内嵌、Axum 直出。

> **实现顺序提示:** 三个任务紧耦合,按文件逐个全量重写、逐个提交(便于审计),但**功能验证在三者都改完后**统一做。`include_str!` 只内嵌字符串,任一文件改完都能编译,所以「交叉编译 0 错误」每步都成立,但运行时一致性需全改完。

**验证命令:**
- 交叉编译(确认内嵌路径未变、Rust 仍编译):
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote/src-tauri && XWIN_ACCEPT_LICENSE=1 \
    PATH=/data/sunyunbo/miniconda3/envs/xwin/bin:$PATH \
    cargo xwin build --target x86_64-pc-windows-msvc
  ```
  期望:`Finished`,0 error。
- 前端构建(不涉及该文件,确认未误伤):
  ```bash
  cd /data/sunyunbo/www/StageDeckRemote && npm run build
  ```

## 文件结构

| 文件 | 职责 | 改动 |
|---|---|---|
| `src/presenter/presenter.html` | 主持人页 DOM | 全量重写:共享读数 + MAIN(动作区)/SYNC(截图)页 + 共享 ARM 条 + 共享底部翻页坞 |
| `src/presenter/presenter.css` | 主持人页样式 | 全量重写:保留 `:root` 与基础;新版面(巨钮/翻页上行/双列大按钮/ARM 条) |
| `src/presenter/presenter.js` | 主持人页逻辑 | 全量重写:沿用状态机/轮询/快照链/三层锁门控,适配新 DOM;`renderActions` 去英文宏名 |

---

### Task 1: 重写 `presenter.html`

**Files:**
- Modify(整文件覆盖): `src/presenter/presenter.html`

- [ ] **Step 1: 用以下完整内容覆盖 `src/presenter/presenter.html`**

```html
<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta
      name="viewport"
      content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover"
    />
    <title>STAGEDECK — PRESENTER</title>
    <link rel="stylesheet" href="/presenter/presenter.css" />
  </head>
  <body>
    <div class="frame">
      <!-- ─── status bar ─── -->
      <header class="statusbar">
        <span class="brand">STAGEDECK<span class="brand-sub">·PRESENTER</span></span>
        <span class="status-grid">
          <span class="stat"><span class="dot" id="conn-dot"></span><b id="conn-text">LINK…</b></span>
          <span class="stat">LAT <b id="latency">--</b></span>
          <span class="stat">PG <b id="slide-pos">--/--</b></span>
        </span>
      </header>

      <!-- ─── mode switch ─── -->
      <nav class="modeswitch">
        <button class="mode active" data-mode="main">主控 · MAIN</button>
        <button class="mode" data-mode="sync">屏幕 · SYNC</button>
      </nav>

      <!-- ─── shared readout ─── -->
      <section class="readout">
        <span class="readout-k">SLIDE</span>
        <span class="readout-v" id="slide-big">--</span>
        <span class="readout-k">/ <span id="slide-total">--</span></span>
        <span class="show-flag" id="show-flag">IDLE</span>
      </section>

      <!-- ─── MAIN: COM macro actions (scrolls) ─── -->
      <main class="page page-main active" id="page-main">
        <div class="section-label">页内动作 · ACTIONS</div>
        <div class="actions" id="actions"></div>
        <div class="empty" id="actions-empty" hidden>无可触发动作 / 切到 SYNC 用坐标点击</div>
        <div class="unavail" id="com-unavail" hidden>
          COM 不可用<br /><span>PowerPoint 未连接 — 请切到 SYNC 屏幕同步</span>
        </div>
      </main>

      <!-- ─── SYNC: screenshot fallback ─── -->
      <main class="page page-sync" id="page-sync">
        <section class="stage">
          <img id="preview" alt="PPT" draggable="false" />
          <div class="overlay" id="lock-overlay">已锁定 · 点「释放控制」</div>
          <div class="reticle tl"></div><div class="reticle tr"></div>
          <div class="reticle bl"></div><div class="reticle br"></div>
        </section>
      </main>

      <!-- ─── ARM safety strip (shared) ─── -->
      <button class="arm" id="lock-btn" data-action="toggle-lock">● 已解锁 · 点此锁定</button>

      <!-- ─── transport dock (shared, pinned bottom) ─── -->
      <div class="dock">
        <div class="dock-row">
          <button class="d-btn d-prev" data-action="prev"><span class="gl">◄</span>上一页</button>
          <button class="d-btn d-black" data-action="black">黑屏</button>
          <button class="d-btn d-restore" data-action="restore">恢复</button>
        </div>
        <button class="d-next" data-action="next"><span class="gl">►</span><span class="tx">下一页</span></button>
      </div>
    </div>
    <script src="/presenter/presenter.js"></script>
  </body>
</html>
```

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.html
git commit -m "feat(presenter): layout C DOM — pinned transport dock, shared readout/arm"
```

---

### Task 2: 重写 `presenter.css`

**Files:**
- Modify(整文件覆盖): `src/presenter/presenter.css`

- [ ] **Step 1: 用以下完整内容覆盖 `src/presenter/presenter.css`**

```css
/* ───────────────────────────────────────────────────────────────
   STAGEDECK · PRESENTER — "CONTROL SURFACE" (台上单手 · 方案 C)
   竖屏单手:最高频动作贴底拇指区;巨钮「下一页」反白高亮。
   离线系统字体(场馆无网)。
   ─────────────────────────────────────────────────────────────── */
:root {
  --bg: #0a0a0b;
  --panel: #0e0e10;
  --panel-2: #131316;
  --line: #26262b;
  --line-hi: #3a3a42;
  --ink: #ededee;
  --ink-dim: #a2a2aa;
  --ink-faint: #82828b;
  --danger: #d23b3b;
  --sans: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
    "Microsoft YaHei", system-ui, sans-serif;
  --mono: ui-monospace, "Cascadia Mono", "JetBrains Mono", "SF Mono",
    "Segoe UI Mono", Menlo, Consolas, monospace;
  --tap: 56px;
}

* {
  box-sizing: border-box;
  -webkit-user-select: none;
  user-select: none;
  -webkit-tap-highlight-color: transparent;
}
html,
body {
  margin: 0;
  height: 100%;
  overflow: hidden;
  background: var(--bg);
  color: var(--ink);
  font-family: var(--sans);
}
body::before {
  content: "";
  position: fixed;
  inset: 0;
  background-image: linear-gradient(var(--line) 1px, transparent 1px),
    linear-gradient(90deg, var(--line) 1px, transparent 1px);
  background-size: 28px 28px;
  opacity: 0.16;
  pointer-events: none;
}
.frame {
  position: relative;
  height: 100%;
  display: flex;
  flex-direction: column;
  max-width: 980px;
  margin: 0 auto;
  border-inline: 1px solid var(--line);
}

/* ─── status bar ─── */
.statusbar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--line-hi);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}
.brand {
  font-weight: 700;
  letter-spacing: 0.18em;
}
.brand-sub {
  color: var(--ink-faint);
}
.status-grid {
  display: flex;
  gap: 14px;
  align-items: center;
  color: var(--ink-dim);
}
.stat {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.stat b {
  color: var(--ink);
  font-weight: 600;
}
.dot {
  width: 8px;
  height: 8px;
  background: var(--danger);
  box-shadow: 0 0 0 1px #000;
}
.dot.online {
  background: var(--ink);
}

/* ─── mode switch ─── */
.modeswitch {
  flex: 0 0 auto;
  display: grid;
  grid-template-columns: 1fr 1fr;
  border-bottom: 1px solid var(--line-hi);
}
.mode {
  appearance: none;
  background: transparent;
  border: none;
  border-right: 1px solid var(--line);
  color: var(--ink-faint);
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  padding: 13px 0;
  cursor: pointer;
  transition: color 0.08s linear, background 0.08s linear;
}
.mode:last-child {
  border-right: none;
}
.mode.active {
  background: var(--ink);
  color: #000;
  font-weight: 700;
}

/* ─── shared readout ─── */
.readout {
  flex: 0 0 auto;
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 12px 16px 9px;
  border-bottom: 1px solid var(--line);
  font-family: var(--mono);
}
.readout-k {
  font-size: 11px;
  letter-spacing: 0.16em;
  color: var(--ink-faint);
  text-transform: uppercase;
}
.readout-v {
  font-size: 40px;
  line-height: 0.85;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.show-flag {
  margin-left: auto;
  align-self: center;
  font-size: 10px;
  letter-spacing: 0.18em;
  color: var(--ink-faint);
  border: 1px solid var(--line-hi);
  padding: 3px 8px;
  text-transform: uppercase;
}
.show-flag.live {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}

/* ─── pages ─── */
.page {
  flex: 1 1 auto;
  min-height: 0;
  display: none;
  flex-direction: column;
}
.page.active {
  display: flex;
}
.page-main {
  overflow-y: auto;
}

/* ─── MAIN: actions (big, two-column) ─── */
.section-label {
  flex: 0 0 auto;
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.22em;
  color: var(--ink-faint);
  text-transform: uppercase;
  padding: 12px 16px 0;
}
.actions {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  padding: 10px 16px 14px;
}
.act {
  appearance: none;
  min-height: 62px;
  background: var(--panel-2);
  border: 1px solid var(--line-hi);
  color: var(--ink);
  font-family: var(--sans);
  font-size: 16px;
  font-weight: 600;
  letter-spacing: 0.01em;
  padding: 10px 12px;
  cursor: pointer;
  text-align: left;
  display: flex;
  align-items: center;
  transition: background 0.06s linear, color 0.06s linear, transform 0.04s linear;
}
.act:active:not(:disabled) {
  background: var(--ink);
  color: #000;
  transform: translateY(1px);
}
.act:disabled {
  opacity: 0.32;
  cursor: not-allowed;
}
.empty,
.unavail {
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.06em;
  color: var(--ink-faint);
  padding: 22px 16px;
  text-align: center;
}
.unavail {
  border: 1px dashed var(--line-hi);
  margin: 8px 16px 0;
  line-height: 1.7;
}
.unavail span {
  color: var(--ink-dim);
  font-size: 11px;
}

/* ─── SYNC: stage ─── */
.stage {
  flex: 1 1 auto;
  min-height: 0;
  position: relative;
  background: #000;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
#preview {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  display: block;
}
.overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(8, 8, 9, 0.66);
  font-family: var(--mono);
  font-size: 13px;
  letter-spacing: 0.1em;
  color: var(--ink);
  pointer-events: none;
}
.overlay.hidden {
  display: none;
}
.reticle {
  position: absolute;
  width: 14px;
  height: 14px;
  border: 1px solid var(--ink-dim);
  opacity: 0.7;
  pointer-events: none;
}
.reticle.tl { top: 8px; left: 8px; border-right: none; border-bottom: none; }
.reticle.tr { top: 8px; right: 8px; border-left: none; border-bottom: none; }
.reticle.bl { bottom: 8px; left: 8px; border-right: none; border-top: none; }
.reticle.br { bottom: 8px; right: 8px; border-left: none; border-top: none; }

/* ─── ARM safety strip (shared) ─── */
.arm {
  flex: 0 0 auto;
  appearance: none;
  width: 100%;
  min-height: 44px;
  border: none;
  border-top: 1px solid var(--line-hi);
  background: transparent;
  color: var(--ink-dim);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  cursor: pointer;
  transition: color 0.08s linear, background 0.08s linear, box-shadow 0.08s linear;
}
.arm.locked {
  color: var(--danger);
  background: rgba(210, 59, 59, 0.06);
  box-shadow: inset 0 0 0 1px var(--danger);
}
.arm:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* ─── transport dock (shared, pinned bottom) ─── */
.dock {
  flex: 0 0 auto;
}
.dock-row {
  display: grid;
  grid-template-columns: 1.2fr 0.9fr 0.9fr;
  border-top: 1px solid var(--line-hi);
}
.d-btn {
  appearance: none;
  min-height: var(--tap);
  background: transparent;
  border: none;
  border-right: 1px solid var(--line);
  color: var(--ink);
  font-family: var(--sans);
  font-size: 15px;
  font-weight: 600;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  transition: background 0.06s linear, color 0.06s linear;
}
.d-btn:last-child {
  border-right: none;
}
.d-btn .gl {
  font-family: var(--mono);
  font-size: 18px;
  color: var(--ink-faint);
}
.d-btn:active:not(:disabled) {
  background: var(--ink);
  color: #000;
}
.d-btn:active:not(:disabled) .gl {
  color: #000;
}
.d-black:active:not(:disabled) {
  background: var(--danger);
  color: #fff;
}
.d-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
.d-next {
  appearance: none;
  width: 100%;
  height: 80px;
  border: none;
  border-top: 1px solid var(--line-hi);
  background: var(--ink);
  color: #000;
  font-family: var(--sans);
  font-size: 17px;
  font-weight: 700;
  letter-spacing: 0.04em;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  transition: opacity 0.06s linear, background 0.06s linear;
}
.d-next .gl {
  font-family: var(--mono);
  font-size: 30px;
  line-height: 1;
}
.d-next:active:not(:disabled) {
  background: #cfcfd4;
}
.d-next:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
```

- [ ] **Step 2: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.css
git commit -m "style(presenter): layout C styles — giant next, transport row, big 2-col actions, arm strip"
```

---

### Task 3: 重写 `presenter.js`

**Files:**
- Modify(整文件覆盖): `src/presenter/presenter.js`

> 保留原有逻辑(轮询 900ms、快照链、401/断线降级、三层锁门控 `canAct`/`canTransport`)。门控语义**不变**:翻页坞按钮(prev/next/black/restore)= `canTransport()`(在线且 remote 开、未被导播强锁,**不要求** ARM);宏按钮 = `canAct()`(还须 ARM 解锁);SYNC 点坐标 = `canAct()`。`renderActions` 不再渲染英文宏名小标,仍以 `data-id`(形状名)发 `/api/action {shape}`。

- [ ] **Step 1: 用以下完整内容覆盖 `src/presenter/presenter.js`**

```js
// STAGEDECK · PRESENTER — 台上单手控制台(方案 C,vanilla,无构建;Axum 直出)。
// MAIN = COM 宏按钮(双列大钮)+ 共享底部翻页坞;SYNC = 截图 + 点坐标兜底。
// 所有 DOM 变更走 render();state 是唯一真源(防误触)。
(function () {
  "use strict";

  var token = new URLSearchParams(location.search).get("token") || "";
  var SNAPSHOT_MIN = 300; // ms between snapshot frames
  var POLL = 900; // ms status + slide_state cadence

  var el = {
    connDot: document.getElementById("conn-dot"),
    connText: document.getElementById("conn-text"),
    latency: document.getElementById("latency"),
    slidePos: document.getElementById("slide-pos"),
    slideBig: document.getElementById("slide-big"),
    slideTotal: document.getElementById("slide-total"),
    showFlag: document.getElementById("show-flag"),
    actions: document.getElementById("actions"),
    actionsEmpty: document.getElementById("actions-empty"),
    comUnavail: document.getElementById("com-unavail"),
    preview: document.getElementById("preview"),
    overlay: document.getElementById("lock-overlay"),
    lockBtn: document.getElementById("lock-btn"),
    pageMain: document.getElementById("page-main"),
    pageSync: document.getElementById("page-sync"),
    modes: Array.prototype.slice.call(document.querySelectorAll(".mode")),
    dockBtns: Array.prototype.slice.call(document.querySelectorAll(".dock [data-action]")),
  };

  var state = {
    online: false,
    connecting: true,
    tokenInvalid: false,
    allowed: false, // remote_enabled && !control_locked
    unlocked: false, // presenter_unlocked
    latency: null,
    mode: "main",
    com: { available: false, in_show: false, current: 0, total: 0, actions: [] },
    statusTimer: null,
  };
  var lastActionsKey = ""; // avoid rebuilding the grid every tick

  function api(path, body) {
    var opts = body
      ? {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(Object.assign({ token: token, client_id: "presenter" }, body)),
        }
      : { method: "GET" };
    return fetch(path, opts);
  }

  // ── gating ──
  function canAct() {
    return state.online && state.allowed && state.unlocked;
  }
  function canTransport() {
    return state.online && state.allowed;
  }

  // ── render: single DOM update point ──
  function render() {
    el.connDot.classList.toggle("online", state.online);
    el.connText.textContent = state.tokenInvalid
      ? "TOKEN✗"
      : state.online
        ? "LINK"
        : state.connecting
          ? "LINK…"
          : "DOWN";
    el.latency.textContent = state.latency == null ? "--" : state.latency + "ms";

    var cur = state.com.current || 0;
    var tot = state.com.total || 0;
    el.slidePos.textContent = (cur || "--") + "/" + (tot || "--");
    el.slideBig.textContent = cur ? String(cur).padStart(2, "0") : "--";
    el.slideTotal.textContent = tot || "--";
    el.showFlag.textContent = state.com.in_show ? "LIVE" : "IDLE";
    el.showFlag.classList.toggle("live", state.com.in_show);

    // ARM strip: armed = presenter 已解锁(控制权放出)
    var armed = state.unlocked;
    el.lockBtn.textContent = armed ? "● 已解锁 · 点此锁定" : "○ 已锁定 · 点此释放控制";
    el.lockBtn.classList.toggle("locked", !armed);
    el.lockBtn.disabled = !state.online || (!state.allowed && !state.unlocked);

    // SYNC 锁定遮罩
    el.overlay.classList.toggle("hidden", armed);
    el.overlay.textContent = state.online
      ? "已锁定 · 点「释放控制」"
      : state.tokenInvalid
        ? "TOKEN 无效"
        : "连接已断开";

    // COM 可用性(MAIN)
    var comUp = state.com.available;
    el.comUnavail.hidden = comUp;
    el.actions.hidden = !comUp;
    el.actionsEmpty.hidden = !comUp || state.com.actions.length > 0;

    // 翻页坞:canTransport(不要求 ARM)
    var t = !canTransport();
    el.dockBtns.forEach(function (b) {
      b.disabled = t;
    });

    renderActions();
  }

  function renderActions() {
    var actions = state.com.actions || [];
    var key = actions.map(function (a) { return a.id + "|" + a.label; }).join("¦");
    if (key !== lastActionsKey) {
      lastActionsKey = key;
      el.actions.innerHTML = "";
      actions.forEach(function (a) {
        var b = document.createElement("button");
        b.className = "act";
        b.setAttribute("data-id", a.id);
        b.textContent = a.label || a.id;
        el.actions.appendChild(b);
      });
    }
    var ok = canAct();
    Array.prototype.forEach.call(el.actions.children, function (b) {
      b.disabled = !ok;
    });
  }

  // ── status + slide_state poll ──
  function poll() {
    var t0 = Date.now();
    Promise.all([
      api("/api/status?token=" + encodeURIComponent(token)).then(handle401),
      api("/api/slide_state?token=" + encodeURIComponent(token)).then(handle401),
    ])
      .then(function (rs) {
        if (!rs[0] || !rs[1]) return; // 401 handled
        return Promise.all([rs[0].json(), rs[1].json()]);
      })
      .then(function (data) {
        if (!data) return;
        var s = data[0];
        state.online = true;
        state.connecting = false;
        state.tokenInvalid = false;
        state.latency = Date.now() - t0;
        state.allowed = s.remote_enabled && !s.control_locked;
        state.unlocked = state.allowed && s.presenter_unlocked;
        state.com = data[1] || state.com;
        render();
      })
      .catch(function () {
        setDown(false);
      });
  }
  function handle401(r) {
    if (r.status === 401) {
      stopPolling();
      setDown(true);
      return null;
    }
    return r;
  }
  function setDown(tokenInvalid) {
    state.online = false;
    state.connecting = false;
    state.unlocked = false;
    state.allowed = false;
    state.latency = null;
    state.tokenInvalid = !!tokenInvalid;
    render();
  }
  function startPolling() {
    if (!state.statusTimer) state.statusTimer = setInterval(poll, POLL);
  }
  function stopPolling() {
    if (state.statusTimer) {
      clearInterval(state.statusTimer);
      state.statusTimer = null;
    }
  }

  // ── snapshot chain (SYNC) ──
  function loadFrame() {
    if (state.tokenInvalid) return;
    el.preview.src = "/api/snapshot.jpg?token=" + encodeURIComponent(token) + "&t=" + Date.now();
  }
  el.preview.addEventListener("load", function () {
    if (state.mode === "sync" && !state.tokenInvalid) setTimeout(loadFrame, SNAPSHOT_MIN);
  });
  el.preview.addEventListener("error", function () {
    if (state.mode === "sync" && !state.tokenInvalid) setTimeout(loadFrame, 1000);
  });

  // ── mode switch ──
  el.modes.forEach(function (m) {
    m.addEventListener("click", function () {
      var mode = m.getAttribute("data-mode");
      if (mode === state.mode) return;
      state.mode = mode;
      el.modes.forEach(function (x) { x.classList.toggle("active", x === m); });
      el.pageMain.classList.toggle("active", mode === "main");
      el.pageSync.classList.toggle("active", mode === "sync");
      if (mode === "sync") loadFrame(); // (re)start the snapshot chain
    });
  });

  // ── macro action buttons (delegated) ──
  el.actions.addEventListener("click", function (ev) {
    var b = ev.target.closest(".act");
    if (!b || b.disabled) return;
    var shape = b.getAttribute("data-id");
    api("/api/action", { shape: shape }).then(poll).catch(function () {});
  });

  // ── transport dock (delegated on body) ──
  var ACT = { prev: "/api/prev", next: "/api/next", black: "/api/black", restore: "/api/restore" };
  document.body.addEventListener("click", function (ev) {
    var b = ev.target.closest(".d-btn, .d-next");
    if (!b || b.disabled) return;
    var a = b.getAttribute("data-action");
    if (ACT[a]) api(ACT[a], {}).then(poll).catch(function () {});
  });

  // ── arm / lock ──
  el.lockBtn.addEventListener("click", function () {
    if (el.lockBtn.disabled) return;
    api(state.unlocked ? "/api/lock" : "/api/unlock", {}).then(poll).catch(function () {});
  });

  // ── preview tap → normalized click (SYNC) ──
  el.preview.addEventListener("click", function (ev) {
    if (!canAct()) return;
    var r = el.preview.getBoundingClientRect();
    if (!r.width || !r.height) return;
    var x = (ev.clientX - r.left) / r.width;
    var y = (ev.clientY - r.top) / r.height;
    if (x < 0 || x > 1 || y < 0 || y > 1) return;
    api("/api/click", { x: x, y: y }).catch(function () {});
  });

  // ── boot ──
  render();
  poll();
  startPolling();
})();
```

- [ ] **Step 2: 交叉编译 + 前端构建**

Run 交叉编译命令。Expected:`Finished`,0 error。
Run `npm run build`。Expected:构建成功(不涉及该文件)。

- [ ] **Step 3: 一致性自检(无运行环境,静态核对)**

确认 JS 里 `getElementById`/`querySelectorAll` 的每个选择器都能在 `presenter.html` 找到:
`conn-dot, conn-text, latency, slide-pos, slide-big, slide-total, show-flag, actions, actions-empty, com-unavail, preview, lock-overlay, lock-btn, page-main, page-sync, .mode, .dock [data-action]`;且 CSS 覆盖了 `.act/.arm/.arm.locked/.dock-row/.d-btn/.d-black/.d-next/.page-main`。任一缺失即修。

- [ ] **Step 4: Commit**

```bash
cd /data/sunyunbo/www/StageDeckRemote
git add src/presenter/presenter.js
git commit -m "feat(presenter): wire layout C — dock transport gating, drop macro-name caption"
```

---

## Self-Review

- **Spec 覆盖:** §2 版面结构 → Task 1(DOM)+ Task 2(样式);§3 组件规格 → Task 2;§4 门控/交互 → Task 3(沿用 `canAct`/`canTransport`,语义不变);§2 去英文宏名 → Task 3 `renderActions`。SYNC 共用底部坞 → DOM 中 dock 在 page 之外、两模式常驻。
- **类型/选择器一致:** HTML id ↔ JS `getElementById` ↔ CSS class 三处核对见 Task 3 Step 3。`data-id`(形状)→ `/api/action {shape}` 与后端 `ActionReqBody.shape` 一致(沿用 shape-action 改造)。
- **Placeholder:** 无 TBD;三个文件均给出完整内容。
- **门控未改:** 翻页 = `canTransport`(不要求 ANR/解锁,与后端 `button_allowed` 对应);宏/点坐标 = `canAct`(对应 `click_allowed`)。三层锁逻辑零改动。
- **回归:** 后端、`include_str!` 路径、数据契约不变 → 交叉编译与 18 个 Rust 测试不受影响。
