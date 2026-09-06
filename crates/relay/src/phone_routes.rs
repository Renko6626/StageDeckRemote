#[cfg(test)]
use crate::session::Session;
use crate::AppCtx;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::Router;
use base64::Engine;
use protocol::ControlMessage;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio_stream::StreamExt as _;

const PRESENTER_HTML: &str = include_str!("../../../src/presenter/presenter.html");
const PRESENTER_CSS: &str = include_str!("../../../src/presenter/presenter.css");
const PRESENTER_JS: &str = include_str!("../../../src/presenter/presenter.js");
const PRESENTER_FAVICON: &str = include_str!("../../../src/presenter/favicon.svg");

pub fn router() -> Router<Arc<AppCtx>> {
    Router::new()
        // 隧道:除 stream.mjpg 外的 /api/* 全转发
        .route("/r/:sid/api/status", get(tunnel_get))
        .route("/r/:sid/api/slide_state", get(tunnel_get))
        .route("/r/:sid/api/messages", get(tunnel_get))
        .route("/r/:sid/api/snapshot.jpg", get(tunnel_get))
        .route("/r/:sid/api/next", any(tunnel_post))
        .route("/r/:sid/api/prev", any(tunnel_post))
        .route("/r/:sid/api/black", any(tunnel_post))
        .route("/r/:sid/api/restore", any(tunnel_post))
        .route("/r/:sid/api/action", any(tunnel_post))
        .route("/r/:sid/api/click", any(tunnel_post))
        .route("/r/:sid/api/lock", any(tunnel_post))
        .route("/r/:sid/api/unlock", any(tunnel_post))
        .route("/r/:sid/api/message", any(tunnel_post))
        .route("/r/:sid/api/stream.mjpg", get(stream_mjpeg))
        // /r/:sid/presenter/api/* aliases — relay-served presenter uses
        // <base href="/r/<sid>/presenter/">, so relative "api/foo" resolves
        // to "/r/<sid>/presenter/api/foo". These mirror the /r/:sid/api/*
        // tunneled routes; the tunnel strips "/r/<sid>" leaving "/presenter/api/foo"
        // which the director serves via Task 6's LAN aliases.
        .route("/r/:sid/presenter/api/status", get(tunnel_get))
        .route("/r/:sid/presenter/api/slide_state", get(tunnel_get))
        .route("/r/:sid/presenter/api/messages", get(tunnel_get))
        .route("/r/:sid/presenter/api/snapshot.jpg", get(tunnel_get))
        .route("/r/:sid/presenter/api/next", any(tunnel_post))
        .route("/r/:sid/presenter/api/prev", any(tunnel_post))
        .route("/r/:sid/presenter/api/black", any(tunnel_post))
        .route("/r/:sid/presenter/api/restore", any(tunnel_post))
        .route("/r/:sid/presenter/api/action", any(tunnel_post))
        .route("/r/:sid/presenter/api/click", any(tunnel_post))
        .route("/r/:sid/presenter/api/lock", any(tunnel_post))
        .route("/r/:sid/presenter/api/unlock", any(tunnel_post))
        .route("/r/:sid/presenter/api/message", any(tunnel_post))
        .route("/r/:sid/presenter/api/stream.mjpg", get(stream_mjpeg))
        // Presenter 静态资源 + <base href> 注入
        .route("/r/:sid/presenter", get(presenter_html))
        .route("/r/:sid/presenter/presenter.css", get(presenter_css))
        .route("/r/:sid/presenter/presenter.js", get(presenter_js))
        .route("/r/:sid/presenter/favicon.svg", get(presenter_favicon))
}

async fn presenter_html(Path(sid): Path<String>) -> Response {
    let base = format!("/r/{}/presenter/", sid);
    let html = PRESENTER_HTML.replace("__BASE__", &base);
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

async fn presenter_css() -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], PRESENTER_CSS).into_response()
}

async fn presenter_js() -> Response {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], PRESENTER_JS).into_response()
}

async fn presenter_favicon() -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml")], PRESENTER_FAVICON).into_response()
}

async fn tunnel_get(
    State(ctx): State<Arc<AppCtx>>,
    Path(sid): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    method: Method,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    tunnel(ctx, sid, q, method, headers, uri, Bytes::new()).await
}

async fn tunnel_post(
    State(ctx): State<Arc<AppCtx>>,
    Path(sid): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    method: Method,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    tunnel(ctx, sid, q, method, headers, uri, body).await
}

async fn tunnel(
    ctx: Arc<AppCtx>,
    sid: String,
    query_map: HashMap<String, String>,
    method: Method,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    // 1. 找 session
    let Some(sess) = ctx.sessions.get(&sid).await else {
        tracing::warn!(sid = %sid, path = %uri.path(), "tunnel: session_unavailable");
        return (StatusCode::BAD_GATEWAY, "session_unavailable").into_response();
    };
    // 2. 边缘 token 校验(从 query 拿)
    let supplied = query_map.get("token").map(|s| s.as_str()).unwrap_or("");
    let expected = sess.token.lock().await.clone();
    if expected.is_empty() || supplied != expected {
        tracing::warn!(sid = %sid, path = %uri.path(), "tunnel: invalid_token");
        return (StatusCode::UNAUTHORIZED, "invalid_token").into_response();
    }
    // 3. body 大小 64 KiB 上限
    if body.len() > 64 * 1024 {
        tracing::warn!(sid = %sid, bytes = body.len(), "tunnel: body_too_large");
        return (StatusCode::PAYLOAD_TOO_LARGE, "body_too_large").into_response();
    }
    // 4. 构造 TunnelReq
    let req_id = sess.next_req_id.fetch_add(1, Ordering::SeqCst);
    // 取掉前缀 /r/<sid> 还原导播侧的 path
    let original_path = uri.path();
    let stripped = original_path
        .strip_prefix(&format!("/r/{}", sid))
        .unwrap_or(original_path)
        .to_string();
    let query = uri.query().unwrap_or("").to_string();
    let body_b64 = base64::engine::general_purpose::STANDARD.encode(&body);
    let req = ControlMessage::TunnelReq {
        req_id,
        method: method.as_str().to_string(),
        path: stripped,
        query,
        headers: headers
            .iter()
            .filter_map(|(k, v)| {
                let n = k.as_str().to_lowercase();
                if ["host", "content-length", "connection", "upgrade"].contains(&n.as_str()) {
                    return None;
                }
                Some((n, v.to_str().ok()?.to_string()))
            })
            .collect(),
        body_b64,
    };
    // 5. 注册 oneshot
    let (tx, rx) = oneshot::channel();
    sess.pending.lock().await.insert(req_id, tx);
    // 6. 发到 director
    let send_res = sess.control_tx.lock().await.send(req);
    if send_res.is_err() {
        sess.pending.lock().await.remove(&req_id);
        tracing::warn!(sid = %sid, req_id, "tunnel: director_offline (control channel closed)");
        return (StatusCode::BAD_GATEWAY, "director_offline").into_response();
    }
    // 7. 等响应(超时)
    let timeout = Duration::from_secs(ctx.config.tunnel_timeout_secs);
    let resp = match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(ControlMessage::TunnelResp { status, headers, body_b64, .. })) => {
            let bytes =
                base64::engine::general_purpose::STANDARD.decode(&body_b64).unwrap_or_default();
            let mut builder = Response::builder().status(
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            );
            for (k, v) in headers {
                if !["transfer-encoding", "connection"].contains(&k.as_str()) {
                    builder = builder.header(k, v);
                }
            }
            builder
                .body(Body::from(bytes))
                .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "bad_resp").into_response())
        }
        // sender 被 fail_pending() 丢掉 = 导播控制通道在等待期间断开/被接管
        Ok(Err(_)) => {
            tracing::warn!(sid = %sid, req_id, "tunnel: director_offline (pending dropped)");
            (StatusCode::BAD_GATEWAY, "director_offline").into_response()
        }
        Ok(Ok(_)) => {
            tracing::warn!(sid = %sid, req_id, "tunnel: bad_tunnel_resp");
            (StatusCode::BAD_GATEWAY, "bad_tunnel_resp").into_response()
        }
        Err(_) => {
            sess.pending.lock().await.remove(&req_id);
            tracing::warn!(
                sid = %sid,
                req_id,
                timeout_secs = ctx.config.tunnel_timeout_secs,
                path = %uri.path(),
                "tunnel: tunnel_timeout"
            );
            (StatusCode::GATEWAY_TIMEOUT, "tunnel_timeout").into_response()
        }
    };
    resp
}

async fn stream_mjpeg(
    State(ctx): State<Arc<AppCtx>>,
    Path(sid): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(sess) = ctx.sessions.get(&sid).await else {
        return (StatusCode::BAD_GATEWAY, "session_unavailable").into_response();
    };
    let supplied = q.get("token").map(|s| s.as_str()).unwrap_or("");
    let expected = sess.token.lock().await.clone();
    if expected.is_empty() || supplied != expected {
        return (StatusCode::UNAUTHORIZED, "invalid_token").into_response();
    }
    let rx = sess.frame_tx.subscribe();
    let body = tokio_stream::wrappers::WatchStream::new(rx).filter_map(|frame| {
        if frame.is_empty() {
            None
        } else {
            Some(Ok::<Vec<u8>, std::io::Error>(protocol::mjpeg_part(&frame)))
        }
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "multipart/x-mixed-replace; boundary=frame")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(body))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::ControlMessage;
    use tokio::sync::mpsc;

    fn mk_session() -> (Arc<Session>, mpsc::UnboundedReceiver<ControlMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (frame_tx, _) = tokio::sync::watch::channel(Arc::new(Vec::new()));
        let sess = Arc::new(Session {
            secret: "s".into(),
            token: tokio::sync::Mutex::new("TOK".into()),
            control_tx: tokio::sync::Mutex::new(tx),
            frame_tx,
            pending: tokio::sync::Mutex::new(Default::default()),
            next_req_id: std::sync::atomic::AtomicU64::new(1),
            last_seen: tokio::sync::Mutex::new(std::time::Instant::now()),
            registered_at: std::time::Instant::now(),
            frames_pushed: std::sync::atomic::AtomicU64::new(0),
            frame_last_at: tokio::sync::Mutex::new(None),
            stream_connected: std::sync::atomic::AtomicBool::new(false),
        });
        (sess, rx)
    }

    #[tokio::test]
    async fn tunnel_rejects_wrong_token() {
        let ctx = Arc::new(AppCtx {
            sessions: crate::session::SessionTable::new(),
            started_at: std::time::Instant::now(),
            config: crate::config::Config {
                listen: "0.0.0.0:0".parse().unwrap(),
                session_grace_secs: 60,
                tunnel_timeout_secs: 30,
                admin_token: None,
            },
        });
        let (sess, _rx) = mk_session();
        ctx.sessions.0.write().await.insert("SID-1234".into(), sess);
        let mut q = HashMap::new();
        q.insert("token".into(), "WRONG".into());
        let resp = tunnel(
            ctx,
            "SID-1234".into(),
            q,
            Method::POST,
            HeaderMap::new(),
            "/r/SID-1234/api/next".parse().unwrap(),
            Bytes::new(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn tunnel_forwards_request_and_returns_response() {
        let ctx = Arc::new(AppCtx {
            sessions: crate::session::SessionTable::new(),
            started_at: std::time::Instant::now(),
            config: crate::config::Config {
                listen: "0.0.0.0:0".parse().unwrap(),
                session_grace_secs: 60,
                tunnel_timeout_secs: 5,
                admin_token: None,
            },
        });
        let (sess, mut rx) = mk_session();
        ctx.sessions.0.write().await.insert("SID-1234".into(), sess.clone());
        // 模拟 director:收到 TunnelReq 立刻回 TunnelResp 200 ok
        let sess_for_resp = sess.clone();
        tokio::spawn(async move {
            let msg = rx.recv().await.unwrap();
            if let ControlMessage::TunnelReq { req_id, .. } = msg {
                let resp = ControlMessage::TunnelResp {
                    req_id,
                    status: 200,
                    headers: vec![],
                    body_b64: base64::engine::general_purpose::STANDARD.encode(b"hello"),
                };
                let mut pend = sess_for_resp.pending.lock().await;
                if let Some(tx) = pend.remove(&req_id) {
                    let _ = tx.send(resp);
                }
            }
        });
        let mut q = HashMap::new();
        q.insert("token".into(), "TOK".into());
        let resp = tunnel(
            ctx,
            "SID-1234".into(),
            q,
            Method::GET,
            HeaderMap::new(),
            "/r/SID-1234/api/status?token=TOK".parse().unwrap(),
            Bytes::new(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"hello");
    }

    #[tokio::test]
    async fn tunnel_alias_forwards_with_presenter_prefix() {
        let ctx = Arc::new(AppCtx {
            sessions: crate::session::SessionTable::new(),
            started_at: std::time::Instant::now(),
            config: crate::config::Config {
                listen: "0.0.0.0:0".parse().unwrap(),
                session_grace_secs: 60,
                tunnel_timeout_secs: 5,
                admin_token: None,
            },
        });
        let (sess, mut rx) = mk_session();
        ctx.sessions.0.write().await.insert("SID-1234".into(), sess.clone());
        let sess_for_resp = sess.clone();
        tokio::spawn(async move {
            let msg = rx.recv().await.unwrap();
            if let ControlMessage::TunnelReq { req_id, path, .. } = msg {
                // verify path strip behavior: /r/SID-1234/presenter/api/status → /presenter/api/status
                assert_eq!(path, "/presenter/api/status");
                let resp = ControlMessage::TunnelResp {
                    req_id,
                    status: 200,
                    headers: vec![],
                    body_b64: base64::engine::general_purpose::STANDARD.encode(b"aliased"),
                };
                let mut pend = sess_for_resp.pending.lock().await;
                if let Some(tx) = pend.remove(&req_id) {
                    let _ = tx.send(resp);
                }
            }
        });
        let mut q = HashMap::new();
        q.insert("token".into(), "TOK".into());
        let resp = tunnel(
            ctx,
            "SID-1234".into(),
            q,
            Method::GET,
            HeaderMap::new(),
            "/r/SID-1234/presenter/api/status?token=TOK"
                .parse()
                .unwrap(),
            Bytes::new(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"aliased");
    }

    #[tokio::test]
    async fn stream_emits_multipart_after_frame_pushed() {
        let ctx = Arc::new(AppCtx {
            sessions: crate::session::SessionTable::new(),
            started_at: std::time::Instant::now(),
            config: crate::config::Config {
                listen: "0.0.0.0:0".parse().unwrap(),
                session_grace_secs: 60,
                tunnel_timeout_secs: 5,
                admin_token: None,
            },
        });
        let (sess, _rx) = mk_session();
        // send_replace succeeds even with no active receivers; the sent value
        // becomes the watch's current value and WatchStream emits it on first poll.
        sess.frame_tx.send_replace(Arc::new(vec![0xAA, 0xBB, 0xCC]));
        ctx.sessions.0.write().await.insert("SID-1234".into(), sess);
        let mut q = HashMap::new();
        q.insert("token".into(), "TOK".into());
        let resp = stream_mjpeg(State(ctx), Path("SID-1234".into()), Query(q)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(ct.starts_with("multipart/x-mixed-replace"));
        // take first chunk and verify boundary + payload size header
        let body = resp.into_body();
        let mut stream = http_body_util::BodyExt::into_data_stream(body);
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let text = String::from_utf8_lossy(&chunk);
        assert!(text.contains("--frame"));
        assert!(text.contains("Content-Length: 3"));
    }

    #[tokio::test]
    async fn presenter_html_injects_base_href() {
        let resp = presenter_html(Path("PROM-2026".into())).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains(r#"<base href="/r/PROM-2026/presenter/""#));
        assert!(!text.contains("__BASE__"));
    }
}
