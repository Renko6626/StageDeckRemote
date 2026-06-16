//! 主持人端 HTTP API（spec §9）。
//!
//! 这一层是**面向局域网**的受限接口：只暴露主持人需要的能力
//! （看画面、点击、翻页、锁定）。导播的特权操作不在这里，而是走 Tauri IPC
//! （commands.rs），天然不可从网络访问。
//!
//! 所有接口都要求正确 token；所有动作类接口（翻页/黑屏/恢复/宏/坐标点击）额外要求
//! 三层锁全部放行（含主持人 ARM）；`/api/unlock` 要求导播层放行（button_allowed），
//! 是主持人能够 ARM 的前置条件（spec §13.2）。

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::app_state::AppState;
use crate::{auth, capture, input, log};

// 主持人端静态资源在编译期内嵌，运行时由 Axum 直接吐出（开发/发布行为一致）。
const PRESENTER_HTML: &str = include_str!("../../src/presenter/presenter.html");
const PRESENTER_CSS: &str = include_str!("../../src/presenter/presenter.css");
const PRESENTER_JS: &str = include_str!("../../src/presenter/presenter.js");

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/presenter", get(presenter_page))
        .route("/presenter/presenter.css", get(presenter_css))
        .route("/presenter/presenter.js", get(presenter_js))
        .route("/api/status", get(status))
        .route("/api/snapshot.jpg", get(snapshot))
        .route("/api/click", post(click))
        .route("/api/next", post(next))
        .route("/api/prev", post(prev))
        .route("/api/lock", post(lock))
        .route("/api/unlock", post(unlock))
        .route("/api/black", post(black))
        .route("/api/restore", post(restore))
        .route("/api/slide_state", get(slide_state))
        .route("/api/action", post(action))
        .route("/api/messages", get(messages))
        .route("/api/message", post(message))
        .with_state(state)
}

// ---- 公共辅助 ----------------------------------------------------------------

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
    cid: Option<String>,
}

fn token_ok(state: &AppState, token: &Option<String>) -> bool {
    let expected = {
        let i = state.lock();
        i.token.clone()
    };
    matches!(token, Some(t) if auth::verify(t, &expected))
}

/// 请求体 client_id 是否为当前控制端(单控制端门控)。
fn is_controller(state: &AppState, cid: &Option<String>) -> bool {
    let c = cid.as_deref();
    let i = state.lock();
    c.is_some() && i.active_client_id.as_deref() == c
}

fn json_err(code: StatusCode, reason: &str) -> Response {
    (code, Json(json!({ "ok": false, "reason": reason }))).into_response()
}

fn ok_json() -> Response {
    Json(json!({ "ok": true })).into_response()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---- 页面与状态 --------------------------------------------------------------

async fn presenter_page(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TokenQuery>,
) -> Response {
    if !token_ok(&state, &q.token) {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    Html(PRESENTER_HTML).into_response()
}

async fn presenter_css() -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], PRESENTER_CSS).into_response()
}

async fn presenter_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        PRESENTER_JS,
    )
        .into_response()
}

async fn status(State(state): State<Arc<AppState>>, Query(q): Query<TokenQuery>) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    let body = {
        let mut i = state.lock();
        let is_controller = match &q.cid {
            Some(cid) => crate::app_state::note_presence_and_claim(&mut i, cid, crate::app_state::now_ms()),
            None => false,
        };
        json!({
            "ok": true,
            "ppt_window_found": i.selected_window.is_some(),
            "remote_enabled": i.remote_enabled,
            "control_locked": i.control_locked,
            "presenter_unlocked": i.presenter_unlocked,
            "active_client_id": i.active_client_id,
            "is_controller": is_controller,
            "director_message": i.director_message.clone(),
            "timer": i.timer.as_ref().map(|t| crate::app_state::timer_view_of(t, crate::app_state::now_ms())),
            "server_time": now_secs(),
        })
    };
    Json(body).into_response()
}

async fn snapshot(State(state): State<Arc<AppState>>, Query(q): Query<TokenQuery>) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    match capture::snapshot_jpeg(&state, 1280, 720, 70) {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("capture error: {e}"))
            .into_response(),
    }
}

// ---- 远程点击 ----------------------------------------------------------------

#[derive(Deserialize)]
struct ClickReq {
    token: Option<String>,
    #[allow(dead_code)]
    client_id: Option<String>,
    x: f64,
    y: f64,
}

async fn click(State(state): State<Arc<AppState>>, Json(req): Json<ClickReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    // 坐标必须是 [0,1] 归一化值（spec §13.3）。
    if !(0.0..=1.0).contains(&req.x) || !(0.0..=1.0).contains(&req.y) {
        return json_err(StatusCode::BAD_REQUEST, "coordinate_out_of_range");
    }
    // 三层锁。
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    // 取目标窗口 handle，再实时读取矩形（窗口可能已移动）。
    let handle = {
        let i = state.lock();
        i.selected_window.as_ref().map(|w| w.handle)
    };
    let Some(handle) = handle else {
        return json_err(StatusCode::CONFLICT, "ppt_window_not_found");
    };
    let Some(rect) = crate::window_manager::window_rect(handle) else {
        return json_err(StatusCode::CONFLICT, "ppt_window_not_found");
    };
    let (screen_x, screen_y) = crate::window_manager::map_normalized_to_screen(
        rect.left, rect.top, rect.width, rect.height, req.x, req.y,
    );

    match input::click_screen(screen_x, screen_y) {
        Ok(()) => {
            log::event(
                &state,
                format!(
                    "点击 ({:.3},{:.3}) → 屏幕 ({},{})",
                    req.x, req.y, screen_x, screen_y
                ),
            );
            Json(json!({ "ok": true, "screen_x": screen_x, "screen_y": screen_y })).into_response()
        }
        Err(e) => {
            log::event(&state, format!("点击失败：{e}"));
            json_err(StatusCode::INTERNAL_SERVER_ERROR, "input_failed")
        }
    }
}

// ---- 翻页 --------------------------------------------------------------------

#[derive(Deserialize)]
struct ActionReq {
    token: Option<String>,
    #[allow(dead_code)]
    client_id: Option<String>,
}

async fn next(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    page_turn(&state, &req.token, &req.client_id, true)
}

async fn prev(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    page_turn(&state, &req.token, &req.client_id, false)
}

fn page_turn(
    state: &AppState,
    token: &Option<String>,
    client_id: &Option<String>,
    forward: bool,
) -> Response {
    if !token_ok(state, token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(state, client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    // 翻页与宏/点坐标同级:需三层全通(含主持人 ARM)。
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    // COM 翻页优先；失败回退 SendInput（先置前窗口）。
    let com_result = if forward { crate::ppt_com::nav_next() } else { crate::ppt_com::nav_prev() };
    let result = match com_result {
        Ok(()) => Ok(()),
        Err(_) => {
            if let Some(handle) = {
                let i = state.lock();
                i.selected_window.as_ref().map(|w| w.handle)
            } {
                crate::window_manager::bring_to_front(handle);
            }
            if forward { input::key_next() } else { input::key_prev() }
        }
    };
    match result {
        Ok(()) => {
            log::event(state, if forward { "下一页" } else { "上一页" });
            ok_json()
        }
        Err(e) => {
            log::event(state, format!("翻页失败：{e}"));
            json_err(StatusCode::INTERNAL_SERVER_ERROR, "input_failed")
        }
    }
}

// ---- 控制锁（主持人端本地解锁，spec §6.2.4） --------------------------------

async fn lock(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    {
        let mut i = state.lock();
        i.presenter_unlocked = false;
    }
    log::event(&state, "主持人端：锁定");
    ok_json()
}

async fn unlock(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    // 只有导播放行（远程开启且未强制锁）时才允许主持人解锁。
    if !state.button_allowed() {
        return json_err(StatusCode::FORBIDDEN, "remote_disabled");
    }
    {
        let mut i = state.lock();
        i.presenter_unlocked = true;
    }
    log::event(&state, "主持人端：释放控制");
    ok_json()
}

// ---- 黑屏 / 恢复（COM：View.State） -----------------------------------------

async fn black(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::set_black(true) {
        Ok(()) => ok_json(),
        Err(_) => json_err(StatusCode::SERVICE_UNAVAILABLE, "black_failed"),
    }
}

async fn restore(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::set_black(false) {
        Ok(()) => ok_json(),
        Err(_) => json_err(StatusCode::SERVICE_UNAVAILABLE, "restore_failed"),
    }
}

async fn slide_state(State(state): State<Arc<AppState>>, Query(q): Query<TokenQuery>) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    let st = crate::ppt_com::slide_state().unwrap_or_else(|_| crate::ppt_com::SlideState::unavailable());
    Json(st).into_response()
}

#[derive(Deserialize)]
struct ActionReqBody {
    token: Option<String>,
    #[allow(dead_code)]
    client_id: Option<String>,
    shape: String,
}

async fn action(State(state): State<Arc<AppState>>, Json(req): Json<ActionReqBody>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    match crate::ppt_com::trigger_action(&req.shape) {
        Ok(macro_name) => {
            log::event(&state, format!("触发 {} → {}", req.shape, macro_name));
            ok_json()
        }
        Err(e) => {
            log::event(&state, format!("触发失败（{}）:{e}", req.shape));
            json_err(StatusCode::INTERNAL_SERVER_ERROR, "action_failed")
        }
    }
}

// ---- 快捷消息(主持人 → 导播,仅 token,不受三层锁限制) -------------------

async fn messages(State(state): State<Arc<AppState>>, Query(q): Query<TokenQuery>) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    let presets = {
        let i = state.lock();
        i.presets.clone()
    };
    Json(presets).into_response()
}

#[derive(Deserialize)]
struct MessageReq {
    token: Option<String>,
    #[allow(dead_code)]
    client_id: Option<String>,
    id: String,
}

async fn message(State(state): State<Arc<AppState>>, Json(req): Json<MessageReq>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    // 只允许发送预制列表里的消息:校验 id,取服务端当前文本。
    // 取完文本即释放 guard,避免后续 send/log 在持锁下执行(死锁纪律)。
    let text = {
        let i = state.lock();
        i.presets.iter().find(|p| p.id == req.id).map(|p| p.text.clone())
    };
    let Some(text) = text else {
        return json_err(StatusCode::BAD_REQUEST, "unknown_message");
    };
    let msg = crate::app_state::HostMessage {
        from: "主持人".to_string(),
        text: text.clone(),
        at_ms: now_ms(),
    };
    let _ = state.message_tx.send(msg);
    crate::log::event(&state, format!("收到主持人消息：{text}"));
    ok_json()
}
