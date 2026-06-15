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
