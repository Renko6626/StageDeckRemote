// 主持人端控制逻辑（纯原生 JS，无构建步骤，Axum 直接吐出）。
// 流程：轮询 /api/status 与 /api/snapshot.jpg，点击预览图发送归一化坐标。
//
// 设计要点（防误触）：所有按钮/点击的可用性由统一的 render() 根据
// online + allowed(导播放行) + unlocked(本地解锁) 推导，任何状态变化都重绘，
// 不让"断线还能点""远程已关按钮却亮着"这类不一致出现。
(function () {
  "use strict";

  var token = new URLSearchParams(location.search).get("token") || "";
  var SNAPSHOT_MIN_INTERVAL = 300; // 两帧之间至少间隔（ms）
  var STATUS_INTERVAL = 1000;

  var el = {
    connDot: document.getElementById("conn-dot"),
    connText: document.getElementById("conn-text"),
    latency: document.getElementById("latency"),
    lockText: document.getElementById("lock-text"),
    preview: document.getElementById("preview"),
    overlay: document.getElementById("lock-overlay"),
    lockBtn: document.getElementById("lock-btn"),
    buttons: Array.prototype.slice.call(document.querySelectorAll(".btn")),
  };

  var state = {
    online: false,
    allowed: false, // 导播放行：remote_enabled && !control_locked
    unlocked: false, // 主持人本地解锁后才能点预览
    latency: null,
    connecting: true,
    tokenInvalid: false,
    statusTimer: null,
  };

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

  // ---- 统一渲染：唯一改动 DOM 的地方 ----
  function render() {
    el.connDot.classList.toggle("online", state.online);
    el.connText.textContent = state.tokenInvalid
      ? "token 无效"
      : state.online
        ? "已连接"
        : state.connecting
          ? "连接中…"
          : "已断开";
    el.latency.textContent = state.latency == null ? "-- ms" : state.latency + " ms";

    // 锁定状态 + 遮罩
    el.lockText.textContent = state.unlocked ? "可控制" : "已锁定";
    el.overlay.classList.toggle("hidden", state.unlocked);
    el.overlay.textContent = state.online
      ? '已锁定 · 点击下方"释放控制"'
      : state.tokenInvalid
        ? "token 无效，请用正确链接重新打开"
        : "连接已断开";
    el.lockBtn.textContent = state.unlocked ? "锁定" : "释放控制";
    el.lockBtn.classList.toggle("unlocked", state.unlocked);

    // 按钮可用性
    el.buttons.forEach(function (b) {
      var action = b.getAttribute("data-action");
      if (action === "toggle-lock") {
        // 已解锁时随时能锁；未解锁时需在线且导播放行才能解锁。
        b.disabled = !state.online || (!state.allowed && !state.unlocked);
      } else {
        // 翻页 / 黑屏 / 恢复：需在线且导播放行。
        b.disabled = !state.online || !state.allowed;
      }
    });
  }

  function setDisconnected(tokenInvalid) {
    state.online = false;
    state.connecting = false;
    state.unlocked = false; // 安全：掉线立即视为锁定
    state.allowed = false;
    state.latency = null;
    state.tokenInvalid = !!tokenInvalid;
    render();
  }

  // ---- 状态轮询 ----
  function pollStatus() {
    var t0 = Date.now();
    api("/api/status?token=" + encodeURIComponent(token))
      .then(function (r) {
        if (r.status === 401) {
          stopStatusPolling(); // token 无效，别再无限重试
          setDisconnected(true);
          return null;
        }
        if (!r.ok) throw new Error("status " + r.status);
        return r.json();
      })
      .then(function (s) {
        if (!s) return; // 401 已处理
        state.online = true;
        state.connecting = false;
        state.tokenInvalid = false;
        state.latency = Date.now() - t0;
        state.allowed = s.remote_enabled && !s.control_locked;
        state.unlocked = state.allowed && s.presenter_unlocked;
        render();
      })
      .catch(function () {
        setDisconnected(false);
      });
  }

  function startStatusPolling() {
    if (!state.statusTimer) state.statusTimer = setInterval(pollStatus, STATUS_INTERVAL);
  }
  function stopStatusPolling() {
    if (state.statusTimer) {
      clearInterval(state.statusTimer);
      state.statusTimer = null;
    }
  }

  // ---- 预览链式刷新：上一帧 load/error 后才排下一帧，自然限流 ----
  function loadFrame() {
    if (state.tokenInvalid) return;
    el.preview.src = "/api/snapshot.jpg?token=" + encodeURIComponent(token) + "&t=" + Date.now();
  }
  el.preview.addEventListener("load", function () {
    if (!state.tokenInvalid) setTimeout(loadFrame, SNAPSHOT_MIN_INTERVAL);
  });
  el.preview.addEventListener("error", function () {
    // 截图失败（含断线 / 401）：放慢重试，避免破图狂刷。
    if (!state.tokenInvalid) setTimeout(loadFrame, 1000);
  });

  // ---- 点击预览图 → 归一化坐标 → /api/click（spec §6.2.2 / §7） ----
  el.preview.addEventListener("click", function (ev) {
    if (!state.online || !state.unlocked) return;
    var rect = el.preview.getBoundingClientRect();
    if (!rect.width || !rect.height) return;
    var x = (ev.clientX - rect.left) / rect.width;
    var y = (ev.clientY - rect.top) / rect.height;
    if (x < 0 || x > 1 || y < 0 || y > 1) return;
    api("/api/click", { x: x, y: y }).catch(function () {});
  });

  // ---- 底部按钮 ----
  var ACTIONS = {
    prev: "/api/prev",
    next: "/api/next",
    black: "/api/black",
    restore: "/api/restore",
  };
  document.querySelector(".controls").addEventListener("click", function (ev) {
    var btn = ev.target.closest(".btn");
    if (!btn || btn.disabled) return;
    var action = btn.getAttribute("data-action");
    if (action === "toggle-lock") {
      api(state.unlocked ? "/api/lock" : "/api/unlock", {})
        .then(pollStatus)
        .catch(function () {});
      return;
    }
    if (ACTIONS[action]) api(ACTIONS[action], {}).catch(function () {});
  });

  // ---- 启动 ----
  render();
  loadFrame();
  pollStatus();
  startStatusPolling();
})();
