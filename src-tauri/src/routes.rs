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
    extract::{DefaultBodyLimit, Query, State},
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
const PRESENTER_FAVICON: &str = include_str!("../../src/presenter/favicon.svg");

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/presenter", get(presenter_page))
        .route("/presenter/presenter.css", get(presenter_css))
        .route("/presenter/presenter.js", get(presenter_js))
        .route("/presenter/favicon.svg", get(presenter_favicon))
        .route("/api/status", get(status))
        .route("/api/snapshot.jpg", get(snapshot))
        .route("/api/stream.mjpg", get(stream_mjpeg))
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
        // /presenter/api/* aliases — same handlers, different prefix.
        // presenter.html sets <base href="/presenter/"> so relative "api/foo"
        // resolves to "/presenter/api/foo"; old /api/* routes kept for back-compat.
        .route("/presenter/api/status", get(status))
        .route("/presenter/api/slide_state", get(slide_state))
        .route("/presenter/api/snapshot.jpg", get(snapshot))
        .route("/presenter/api/stream.mjpg", get(stream_mjpeg))
        .route("/presenter/api/messages", get(messages))
        .route("/presenter/api/next", post(next))
        .route("/presenter/api/prev", post(prev))
        .route("/presenter/api/black", post(black))
        .route("/presenter/api/restore", post(restore))
        .route("/presenter/api/action", post(action))
        .route("/presenter/api/click", post(click))
        .route("/presenter/api/lock", post(lock))
        .route("/presenter/api/unlock", post(unlock))
        .route("/presenter/api/message", post(message))
        .layer(DefaultBodyLimit::max(64 * 1024))
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

/// 动作执行结果:状态码 + JSON body。可缓存回放,所以不直接构造 Response。
type Outcome = (StatusCode, serde_json::Value);

fn err_body(reason: &str) -> serde_json::Value {
    json!({ "ok": false, "reason": reason })
}

fn ok_body() -> serde_json::Value {
    json!({ "ok": true })
}

/// 幂等包装:带 `op_id` 的请求先查缓存,命中 Done 回放、命中 InFlight 回 409,
/// 否则执行 `work` 并把结果写进缓存。不带 `op_id` 的旧请求直接执行(兼容)。
/// 调用方必须已经做完 token / 控制权 / 三层锁校验 —— 门控失败不进缓存。
async fn with_op<F, Fut>(state: &Arc<AppState>, op_id: &Option<String>, work: F) -> Response
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Outcome>,
{
    use crate::op_cache::{CachedResp, Reserve};
    let Some(id) = op_id.as_deref().filter(|s| !s.is_empty()) else {
        let (code, body) = work().await;
        return (code, Json(body)).into_response();
    };
    let reserved = state.lock().ops.reserve(id, now_ms());
    match reserved {
        Reserve::Hit(r) => {
            log::event(state, format!("重复请求 op={id} 已去重（回放 HTTP {}）", r.status));
            let code = StatusCode::from_u16(r.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            return (code, Json(r.body)).into_response();
        }
        Reserve::InFlight => {
            log::event(state, format!("重复请求 op={id} 已去重（仍在执行）"));
            return json_err(StatusCode::CONFLICT, "op_in_flight");
        }
        Reserve::Reserved => {}
    }
    let (code, body) = work().await;
    state.lock().ops.complete(id, CachedResp { status: code.as_u16(), body: body.clone() });
    (code, Json(body)).into_response()
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
    Html(PRESENTER_HTML.replace("__BASE__", "/presenter/")).into_response()
}

async fn presenter_css() -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], PRESENTER_CSS).into_response()
}

async fn presenter_favicon() -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml")], PRESENTER_FAVICON).into_response()
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
    // 截图/JPEG 编码是阻塞型 GDI 调用,放到 blocking 线程池。
    let state2 = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        capture::snapshot_jpeg(&state2, 1280, 720, 70)
    })
    .await;
    match result {
        Ok(Ok(bytes)) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, format!("capture error: {e}"))
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "capture error: panicked".to_string())
            .into_response(),
    }
}

/// 主持人端实时画面：MJPEG（multipart/x-mixed-replace）长连流，由帧 broker 扇出。
/// 订阅 watch；静止时不产帧、变化才推一帧；慢消费者自动只看最新（无积压）。
async fn stream_mjpeg(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TokenQuery>,
) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    use tokio_stream::StreamExt;
    let rx = state.capture.subscribe();
    // WatchStream 立即产出当前最新帧，之后每次变化产出一帧；跳过空占位（seq 0 / 空 jpeg）。
    let body = tokio_stream::wrappers::WatchStream::new(rx).filter_map(|frame| {
        if frame.jpeg.is_empty() {
            None
        } else {
            Some(Ok::<Vec<u8>, std::io::Error>(crate::frame_broker::mjpeg_part(
                &frame.jpeg,
            )))
        }
    });
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "multipart/x-mixed-replace; boundary=frame",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::from_stream(body))
        .unwrap()
}

// ---- 远程点击 ----------------------------------------------------------------

#[derive(Deserialize)]
struct ClickReq {
    token: Option<String>,
    client_id: Option<String>,
    #[serde(default)]
    op_id: Option<String>,
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
    let (x, y) = (req.x, req.y);
    with_op(&state, &req.op_id, || async { do_click(&state, x, y) }).await
}

fn do_click(state: &Arc<AppState>, x: f64, y: f64) -> Outcome {
    // 取目标窗口 handle，再实时读取矩形（窗口可能已移动）。
    let handle = {
        let i = state.lock();
        i.selected_window.as_ref().map(|w| w.handle)
    };
    let Some(handle) = handle else {
        return (StatusCode::CONFLICT, err_body("ppt_window_not_found"));
    };
    let Some(rect) = crate::window_manager::window_rect(handle) else {
        return (StatusCode::CONFLICT, err_body("ppt_window_not_found"));
    };
    let (screen_x, screen_y) = crate::window_manager::map_normalized_to_screen(
        rect.left, rect.top, rect.width, rect.height, x, y,
    );

    match input::click_screen(screen_x, screen_y) {
        Ok(()) => {
            log::event(
                state,
                format!("点击 ({:.3},{:.3}) → 屏幕 ({},{})", x, y, screen_x, screen_y),
            );
            (StatusCode::OK, json!({ "ok": true, "screen_x": screen_x, "screen_y": screen_y }))
        }
        Err(e) => {
            log::event(state, format!("点击失败：{e}"));
            (StatusCode::INTERNAL_SERVER_ERROR, err_body("input_failed"))
        }
    }
}

// ---- 翻页 --------------------------------------------------------------------

#[derive(Deserialize)]
struct ActionReq {
    token: Option<String>,
    client_id: Option<String>,
    #[serde(default)]
    op_id: Option<String>,
}

async fn next(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    page_turn(&state, &req, true).await
}

async fn prev(State(state): State<Arc<AppState>>, Json(req): Json<ActionReq>) -> Response {
    page_turn(&state, &req, false).await
}

async fn page_turn(state: &Arc<AppState>, req: &ActionReq, forward: bool) -> Response {
    if !token_ok(state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(state, &req.client_id) {
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    // 翻页与宏/点坐标同级:需三层全通(含主持人 ARM)。
    if !state.click_allowed() {
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    with_op(state, &req.op_id, || do_page_turn(state, forward)).await
}

async fn do_page_turn(state: &Arc<AppState>, forward: bool) -> Outcome {
    // SendInput 回退需要的目标窗口 handle（COM 失败时置前窗口）。
    let handle = {
        let i = state.lock();
        i.selected_window.as_ref().map(|w| w.handle)
    };
    // COM 翻页 + SendInput 回退都是阻塞型调用,整体放到 blocking 线程池。
    let worker_state = state.clone();
    let result = log::run_operation(state.clone(), if forward { "主持人下一页" } else { "主持人上一页" }.into(), move || {
        if !worker_state.click_allowed() { return Err("control_locked".into()); }
        // COM 翻页优先；失败回退 SendInput（先置前窗口）。
        let com_result = if forward { crate::ppt_com::nav_next() } else { crate::ppt_com::nav_prev() };
        match com_result {
            Ok(()) => Ok(()),
            Err(error) => {
                log::event(&worker_state, format!("COM 翻页失败：{error}；尝试键盘回退"));
                if let Some(handle) = handle {
                    crate::window_manager::bring_to_front(handle);
                }
                let r = if forward { input::key_next() } else { input::key_prev() };
                r.map_err(|e| e.to_string())
            }
        }
    })
    .await;
    match result {
        Ok(()) => {
            log::event(state, if forward { "下一页" } else { "上一页" });
            (StatusCode::OK, ok_body())
        }
        Err(e) => {
            log::event(state, format!("翻页失败：{e}"));
            (StatusCode::INTERNAL_SERVER_ERROR, err_body("input_failed"))
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
    with_op(&state, &req.op_id, || async {
        match log::run_operation(state.clone(), "主持人黑屏".into(), move || crate::ppt_com::set_black(true)).await {
            Ok(()) => (StatusCode::OK, ok_body()),
            _ => (StatusCode::SERVICE_UNAVAILABLE, err_body("black_failed")),
        }
    })
    .await
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
    with_op(&state, &req.op_id, || async {
        match log::run_operation(state.clone(), "主持人恢复".into(), move || crate::ppt_com::set_black(false)).await {
            Ok(()) => (StatusCode::OK, ok_body()),
            _ => (StatusCode::SERVICE_UNAVAILABLE, err_body("restore_failed")),
        }
    })
    .await
}

async fn slide_state(State(state): State<Arc<AppState>>, Query(q): Query<TokenQuery>) -> Response {
    if !token_ok(&state, &q.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    let st = crate::ppt_com::read_slide_state(state).await;
    Json(st).into_response()
}

#[derive(Deserialize)]
struct ActionReqBody {
    token: Option<String>,
    client_id: Option<String>,
    #[serde(default)]
    op_id: Option<String>,
    shape: String,
}

async fn action(State(state): State<Arc<AppState>>, Json(req): Json<ActionReqBody>) -> Response {
    if !token_ok(&state, &req.token) {
        return json_err(StatusCode::UNAUTHORIZED, "invalid_token");
    }
    if !is_controller(&state, &req.client_id) {
        log::event(&state, format!("宏请求 {:?} 被拒绝：not_controller", req.shape));
        return json_err(StatusCode::FORBIDDEN, "not_controller");
    }
    if !state.click_allowed() {
        log::event(&state, format!("宏请求 {:?} 被拒绝：control_locked", req.shape));
        return json_err(StatusCode::FORBIDDEN, "control_locked");
    }
    let shape = req.shape.clone();
    with_op(&state, &req.op_id, || async {
        // 阻塞型 STA-COM 调用放到 blocking 线程池,避免占用 tokio worker。
        let worker_state = state.clone();
        let shape_for_worker = shape.clone();
        let result = log::run_operation(state.clone(), format!("主持人宏 shape={shape:?}"), move || {
            // Recheck after scheduling: the director may have locked control meanwhile.
            if !worker_state.click_allowed() { return Err("control_locked".into()); }
            crate::ppt_com::trigger_action(&shape_for_worker, |phase| log::event(&worker_state, phase))
        }).await;
        match result {
            Ok(_) => (StatusCode::OK, ok_body()),
            Err(e) if e.contains("超时") => (StatusCode::GATEWAY_TIMEOUT, err_body("action_timeout")),
            Err(e) if e.contains("PowerPoint 忙") => (StatusCode::CONFLICT, err_body("ppt_busy")),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, err_body("action_failed")),
        }
    })
    .await
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
    let _ = state.message_tx.try_send(msg);
    crate::log::event(&state, format!("收到主持人消息：{text}"));
    ok_json()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    #[tokio::test]
    async fn busy_powerpoint_keeps_heartbeat_alive_and_rejects_another_macro() {
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let state = Arc::new(AppState::new("test-secret".into(), 8787, tx));
        {
            let mut inner = state.lock();
            inner.remote_enabled = true;
            inner.presenter_unlocked = true;
            inner.active_client_id = Some("phone".into());
        }
        let _permit = state.ppt_gate.clone().acquire_owned().await.unwrap();
        assert!(crate::ppt_com::read_slide_state(state.clone()).await.busy);
        let app = router(state.clone());
        let status = app.clone().oneshot(axum::http::Request::builder()
            .uri("/api/status?token=test-secret&cid=phone")
            .body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(status.status(), StatusCode::OK);
        let response = app.oneshot(axum::http::Request::builder()
            .method("POST").uri("/api/action").header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"token":"test-secret","client_id":"phone","shape":"Answer"}"#)).unwrap())
            .await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let logs = log::recent(&state, 100);
        assert!(logs.iter().any(|entry| entry.message.contains("Answer") && entry.message.contains("忙")));
        assert!(logs.iter().all(|entry| !entry.message.contains("test-secret")));
    }

    fn armed_state() -> Arc<AppState> {
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let state = Arc::new(AppState::new("test-secret".into(), 8787, tx));
        {
            let mut inner = state.lock();
            inner.remote_enabled = true;
            inner.presenter_unlocked = true;
            inner.active_client_id = Some("phone".into());
            inner.active_seen_ms = crate::app_state::now_ms();
        }
        state
    }

    async fn post_json(app: &Router, path: &str, body: &str) -> (StatusCode, serde_json::Value) {
        let resp = app.clone().oneshot(axum::http::Request::builder()
            .method("POST").uri(path).header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_string())).unwrap()).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 16).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn same_op_id_is_executed_once_and_replayed() {
        let state = armed_state();
        let app = router(state.clone());
        // Linux 上 nav_next 是 stub → 走 SendInput stub;无论结果如何,两次必须一致且只执行一次。
        let body = r#"{"token":"test-secret","client_id":"phone","op_id":"phone-7"}"#;
        let first = post_json(&app, "/api/next", body).await;
        let logs_after_first = log::recent(&state, 100).len();
        let second = post_json(&app, "/api/next", body).await;
        assert_eq!(first, second, "重复 op 必须原样回放");
        let logs = log::recent(&state, 100);
        assert!(logs.iter().any(|e| e.message.contains("op=phone-7") && e.message.contains("已去重")));
        // 第二次只多了"已去重"这一条日志,没有再执行翻页
        assert_eq!(logs.len(), logs_after_first + 1);
    }

    #[tokio::test]
    async fn gated_request_is_not_cached() {
        let state = armed_state();
        state.lock().control_locked = true;
        let app = router(state.clone());
        let body = r#"{"token":"test-secret","client_id":"phone","op_id":"phone-8"}"#;
        let (code, _) = post_json(&app, "/api/next", body).await;
        assert_eq!(code, StatusCode::FORBIDDEN);
        // 解锁后同 op_id 应真正执行,而不是回放 403
        state.lock().control_locked = false;
        let (code, _) = post_json(&app, "/api/next", body).await;
        assert_ne!(code, StatusCode::FORBIDDEN);
        assert!(log::recent(&state, 100).iter().all(|e| !e.message.contains("已去重")));
    }

    #[tokio::test]
    async fn request_without_op_id_keeps_legacy_behavior() {
        let state = armed_state();
        let app = router(state.clone());
        let body = r#"{"token":"test-secret","client_id":"phone"}"#;
        let a = post_json(&app, "/api/next", body).await;
        let b = post_json(&app, "/api/next", body).await;
        assert_eq!(a.0, b.0);
        assert!(log::recent(&state, 100).iter().all(|e| !e.message.contains("已去重")));
    }
}
