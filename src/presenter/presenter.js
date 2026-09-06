// STAGEDECK · PRESENTER — 台上单手控制台(方案 C,vanilla,无构建;Axum 直出)。
// MAIN = COM 宏按钮(双列大钮)+ 共享底部翻页坞;SYNC = 截图 + 点坐标兜底。
// 所有 DOM 变更走 render();state 是唯一真源(防误触)。
(function () {
  "use strict";

  var token = new URLSearchParams(location.search).get("token") || "";
  var cid = localStorage.getItem("sd.presenter.cid");
  if (!cid) {
    cid = Date.now().toString(36) + Math.random().toString(36).slice(2, 10);
    localStorage.setItem("sd.presenter.cid", cid);
  }
  var POLL = 900; // ms status + slide_state cadence
  var SLOW_POLL = 4000; // ms back-off cadence after a 401 (token changed)
  // Relay waits up to 30s for its tunnel response. A status heartbeat must allow
  // that budget instead of being cancelled by the generic request timeout.
  var STATUS_TIMEOUT = 35000;

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
    pageMsg: document.getElementById("page-msg"),
    msgGrid: document.getElementById("msg-grid"),
    msgEmpty: document.getElementById("msg-empty"),
    msgSent: document.getElementById("msg-sent"),
    dirBanner: document.getElementById("dir-banner"),
    dirBannerText: document.getElementById("dir-banner-text"),
    dirBannerTime: document.getElementById("dir-banner-time"),
    dirBannerX: document.getElementById("dir-banner-x"),
    readout: document.getElementById("readout"),
    readoutLabel: document.getElementById("readout-label"),
    slideFrac: document.getElementById("slide-frac"),
    viewonly: document.getElementById("viewonly"),
  };

  var state = {
    online: false,
    connecting: true,
    tokenInvalid: false,
    allowed: false, // remote_enabled && !control_locked
    unlocked: false, // presenter_unlocked
    isController: false,
    latency: null,
    mode: "main",
    com: { available: false, in_show: false, current: 0, total: 0, actions: [] },
    statusTimer: null,
    pollRate: POLL,
    tokenBanner: false, // dir-banner currently shows the persistent "token changed" notice
    actionPending: false,
  };
  var lastActionsKey = ""; // avoid rebuilding the grid every tick

  function api(path, body, timeoutMs) {
    var opts = body
      ? {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(Object.assign({ token: token, client_id: cid }, body)),
        }
      : { method: "GET" };
    var controller = new AbortController();
    opts.signal = controller.signal;
    if (timeoutMs === undefined) timeoutMs = body ? 12000 : 5000;
    var timeout = setTimeout(function () { controller.abort(); }, timeoutMs);
    return fetch(path, opts).finally(function () { clearTimeout(timeout); });
  }

  // ── gating ──（主持人侧一切操作都需 ARM 解锁)
  function canAct() {
    return state.online && state.allowed && state.unlocked && state.isController && !state.actionPending && !state.com.busy;
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
    if (!timerActive) {
      // 计时进行时大字位归计时所有(handleTimer/tickTimer 写),别让幻灯片号覆盖。
      el.slideBig.textContent = cur ? String(cur).padStart(2, "0") : "--";
      el.slideTotal.textContent = tot || "--";
    }
    el.showFlag.textContent = state.com.busy ? "BUSY" : state.com.in_show ? "LIVE" : "IDLE";
    el.showFlag.classList.toggle("live", state.com.in_show);

    el.viewonly.hidden = !(state.online && !state.isController);

    // ARM strip: armed = presenter 已解锁(控制权放出)
    var armed = state.unlocked;
    el.lockBtn.textContent = armed ? "● 已解锁 · 点此锁定" : "○ 已锁定 · 点此释放控制";
    el.lockBtn.classList.toggle("locked", !armed);
    // Action-aware: ARM (→/api/unlock) needs button_allowed; LOCK (→/api/lock) needs only controllership.
    var canArm = state.online && state.isController && state.allowed; // → /api/unlock
    var canLock = state.online && state.isController; // → /api/lock
    el.lockBtn.disabled = state.unlocked ? !canLock : !canArm;

    // SYNC 锁定遮罩
    el.overlay.classList.toggle("hidden", armed);
    el.overlay.textContent = state.online
      ? "已锁定 · 点「释放控制」"
      : state.tokenInvalid
        ? "TOKEN 无效"
        : "连接已断开";

    // COM 可用性(MAIN)
    var comUp = state.com.available;
    el.comUnavail.textContent = state.com.busy
      ? "PowerPoint 忙，请等待当前操作完成；若长时间无响应，请导播检查 PPT / VBA 弹窗"
      : "PowerPoint 未连接 — 请导播检查 PowerPoint，或切到 SYNC 屏幕同步";
    el.comUnavail.hidden = comUp;
    el.actions.hidden = !comUp;
    el.actionsEmpty.hidden = !comUp || state.com.actions.length > 0;

    // 翻页坞:与宏按钮一致,需 ARM(canAct)。未解锁则全禁用 →「锁就全锁」。
    var t = !canAct();
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
  var statusPending = false;
  var slidePending = false;
  function pollSlide() {
    if (slidePending || state.tokenInvalid) return;
    slidePending = true;
    api("api/slide_state?token=" + encodeURIComponent(token)).then(handle401)
      .then(function (r) { if (r && r.ok) return r.json(); })
      .then(function (s) { if (s) { state.com = s; render(); } })
      .catch(function () {
        state.com = { available: false, in_show: false, current: 0, total: 0, actions: [] };
        render();
      })
      .finally(function () { slidePending = false; });
  }
  function poll() {
    pollSlide();
    if (statusPending) return;
    statusPending = true;
    var t0 = Date.now();
    api("api/status?token=" + encodeURIComponent(token) + "&cid=" + encodeURIComponent(cid), null, STATUS_TIMEOUT).then(handle401)
      .then(function (r) {
        if (!r) return;
        if (!r.ok) throw new Error("status HTTP " + r.status);
        return r.json();
      })
      .then(function (s) {
        if (!s) return;
        state.online = true;
        state.connecting = false;
        state.tokenInvalid = false;
        setPollRate(POLL); // recovered: restore fast cadence
        if (state.tokenBanner) {
          state.tokenBanner = false;
          hideDirBanner(); // clear the persistent "token changed" notice
        }
        state.latency = Date.now() - t0;
        state.allowed = s.remote_enabled && !s.control_locked;
        state.unlocked = state.allowed && s.presenter_unlocked;
        state.isController = !!s.is_controller;
        render();
        // token 恢复后,若仍停留在 SYNC 且流已断,自愈重连
        if (state.mode === "sync" && !el.preview.getAttribute("src")) startStream();
        handleDirectorMessage(s.director_message);
        handleTimer(s.timer);
      })
      .catch(function () {
        setDown(false);
      })
      .finally(function () { statusPending = false; });
  }
  function handle401(r) {
    if (r.status === 401) {
      setDown(true);
      setPollRate(SLOW_POLL); // back off, keep polling so a re-issued token self-heals
      state.tokenBanner = true;
      showDirBanner("访问码已更改,请重新扫码二维码", { persist: true });
      return null;
    }
    return r;
  }
  function setDown(tokenInvalid) {
    state.online = false;
    state.connecting = false;
    state.unlocked = false;
    state.isController = false;
    state.allowed = false;
    state.latency = null;
    state.tokenInvalid = !!tokenInvalid;
    if (tokenInvalid) stopStream(); // token 失效后别让 <img> 继续重试 MJPEG
    render();
  }
  function startPolling() {
    if (!state.statusTimer) state.statusTimer = setInterval(poll, state.pollRate);
  }
  function stopPolling() {
    if (state.statusTimer) {
      clearInterval(state.statusTimer);
      state.statusTimer = null;
    }
  }
  // Swap the single poll interval without stacking timers.
  function setPollRate(ms) {
    if (state.pollRate === ms) return;
    state.pollRate = ms;
    stopPolling();
    state.statusTimer = setInterval(poll, ms);
  }

  // ── live stream (SYNC):MJPEG 长连,浏览器原生换帧、无闪烁、空闲零流量 ──
  function startStream() {
    if (state.tokenInvalid) return;
    el.preview.src = "api/stream.mjpg?token=" + encodeURIComponent(token);
  }
  function stopStream() {
    el.preview.removeAttribute("src"); // 断开 MJPEG 连接
  }
  el.preview.addEventListener("error", function () {
    // 连接异常(如切走/网络):SYNC 下短延迟重连,其它模式不管
    if (state.mode === "sync" && !state.tokenInvalid) setTimeout(startStream, 1000);
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
      el.pageMsg.classList.toggle("active", mode === "msg");
      if (mode === "msg") loadMessages();
      if (mode === "sync") startStream();
      else stopStream();
    });
  });

  // ── action response handler: surface 403 (locked / lost control), always refresh gating ──
  function onActionResp(r) {
    if (r && r.status === 401) {
      handle401(r);
    } else if (r && r.status === 403) {
      flashNotice("已被锁定 / 已失去控制权");
    } else if (r && !r.ok) {
      flashNotice(r.status === 504
        ? "执行超时，结果未知；请导播检查 PPT 弹窗，勿重复点击"
        : "操作失败，请导播查看日志和 PPT 弹窗（HTTP " + r.status + "）");
    }
    poll(); // refresh gating immediately in all cases
  }
  // Brief mode-independent toast (dir-banner is a sibling of all pages → visible in MAIN/SYNC/MSG).
  function flashNotice(text) {
    state.tokenBanner = false; // 瞬时提示不是持久的"访问码已变"横幅
    showDirBanner(text, { persist: false });
  }

  // ── macro action buttons (delegated) ──
  el.actions.addEventListener("click", function (ev) {
    var b = ev.target.closest(".act");
    if (!b || b.disabled || !canAct()) return;
    var shape = b.getAttribute("data-id");
    state.actionPending = true;
    render();
    api("api/action", { shape: shape }).then(onActionResp).catch(function () {
      flashNotice("宏请求未完成，结果未知；请导播检查日志，勿重复点击");
    }).finally(function () { state.actionPending = false; render(); poll(); });
  });

  // ── transport dock (delegated on body) ──
  var ACT = { prev: "api/prev", next: "api/next", black: "api/black", restore: "api/restore" };
  document.body.addEventListener("click", function (ev) {
    var b = ev.target.closest(".d-btn, .d-next");
    if (!b || b.disabled) return;
    var a = b.getAttribute("data-action");
    if (ACT[a]) api(ACT[a], {}).then(onActionResp).catch(function () {});
  });

  // ── arm / lock ──
  el.lockBtn.addEventListener("click", function () {
    if (el.lockBtn.disabled) return;
    api(state.unlocked ? "api/lock" : "api/unlock", {}).then(poll).catch(function () {});
  });

  // ── preview tap → normalized click (SYNC) ──
  el.preview.addEventListener("click", function (ev) {
    if (!canAct()) return;
    var r = el.preview.getBoundingClientRect();
    if (!r.width || !r.height) return;
    var x = (ev.clientX - r.left) / r.width;
    var y = (ev.clientY - r.top) / r.height;
    if (x < 0 || x > 1 || y < 0 || y > 1) return;
    api("api/click", { x: x, y: y }).then(onActionResp).catch(function () {});
  });

  // ── quick messages (MSG) ──
  var sentTimer = null;
  function loadMessages() {
    api("api/messages?token=" + encodeURIComponent(token))
      .then(handle401)
      .then(function (r) { return r ? r.json() : null; })
      .then(function (list) {
        if (!list) return;
        el.msgEmpty.hidden = list.length > 0;
        el.msgGrid.innerHTML = "";
        list.forEach(function (m) {
          var b = document.createElement("button");
          b.className = "msg-btn";
          b.setAttribute("data-id", m.id);
          b.textContent = m.text;
          el.msgGrid.appendChild(b);
        });
      })
      .catch(function () {});
  }
  function flashSent(text) {
    el.msgSent.textContent = text;
    el.msgSent.hidden = false;
    if (sentTimer) clearTimeout(sentTimer);
    sentTimer = setTimeout(function () { el.msgSent.hidden = true; }, 1400);
  }
  el.msgGrid.addEventListener("click", function (ev) {
    var b = ev.target.closest(".msg-btn");
    if (!b) return;
    api("api/message", { id: b.getAttribute("data-id") })
      .then(function (r) { flashSent(r && r.ok ? "已发送 ✓" : "发送失败"); })
      .catch(function () { flashSent("发送失败"); });
  });

  // ── director → presenter banner ──
  var lastDirSeq = null;
  var dirBannerTimer = null;
  function fmtHM(ms) {
    var d = new Date(ms);
    function p(n) { return (n < 10 ? "0" : "") + n; }
    return p(d.getHours()) + ":" + p(d.getMinutes());
  }
  function handleDirectorMessage(dm) {
    var seq = dm ? dm.seq : 0;
    if (lastDirSeq === null) { lastDirSeq = seq; return; } // 首帧只记基线,不回放旧消息
    if (dm && dm.seq > lastDirSeq) {
      lastDirSeq = dm.seq;
      state.tokenBanner = false; // a fresh director message takes over the shared banner
      showDirBanner(dm.text, { persist: false, time: fmtHM(dm.at_ms) });
    }
  }
  // Shared banner surface for director messages, token-changed notice, and action toasts.
  // persist:true keeps it up (no auto-hide) until explicitly cleared (token-changed case).
  function showDirBanner(text, opts) {
    opts = opts || {};
    el.dirBannerText.textContent = text;
    el.dirBannerTime.textContent = opts.time || "";
    el.dirBanner.hidden = false;
    if (dirBannerTimer) { clearTimeout(dirBannerTimer); dirBannerTimer = null; }
    if (!opts.persist) dirBannerTimer = setTimeout(hideDirBanner, 10000);
  }
  function hideDirBanner() {
    if (dirBannerTimer) { clearTimeout(dirBannerTimer); dirBannerTimer = null; }
    state.tokenBanner = false;
    el.dirBanner.hidden = true;
  }
  el.dirBannerX.addEventListener("click", hideDirBanner);

  // ── show timer (occupies the big readout slot while active; local tick) ──
  var timerBase = null; // {remaining, running, at}
  var tickHandle = null; // setInterval id — runs ONLY while a timer is active
  var timerActive = false; // 大字位是否已让给计时(render() 据此不覆盖大字位)
  function handleTimer(t) {
    if (!t) {
      timerBase = null;
      if (tickHandle) { clearInterval(tickHandle); tickHandle = null; } // no timer → stop ticking
      if (timerActive) {
        // 退出计时模式:大字栏交还幻灯片号。
        timerActive = false;
        el.readoutLabel.textContent = "SLIDE";
        el.slideFrac.hidden = false;
        el.readout.classList.remove("over");
        render(); // 立刻用当前幻灯片号重绘大字位
      }
      return;
    }
    timerBase = { remaining: t.remaining_ms, running: t.running, at: Date.now() };
    if (!timerActive) {
      // 进入计时模式:大字栏让给计时(幻灯片号仍在状态栏 PG)。
      timerActive = true;
      el.readoutLabel.textContent = "计时";
      el.slideFrac.hidden = true;
    }
    if (!tickHandle) tickHandle = setInterval(tickTimer, 250); // active → start (guard double-start)
    tickTimer(); // paint immediately, don't wait up to 250ms
  }
  function fmtTimer(ms) {
    var neg = ms < 0;
    var s = Math.floor(Math.abs(ms) / 1000);
    var m = Math.floor(s / 60);
    s = s % 60;
    return (neg ? "+" : "") + m + ":" + (s < 10 ? "0" : "") + s;
  }
  function tickTimer() {
    if (!timerBase) return;
    var live = timerBase.running ? timerBase.remaining - (Date.now() - timerBase.at) : timerBase.remaining;
    el.slideBig.textContent = fmtTimer(live); // 写进大字位
    el.readout.classList.toggle("over", live < 0); // 超时整行转红(见 css .readout.over)
  }

  // ── boot ──
  render();
  poll();
  startPolling();
})();
