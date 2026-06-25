use crate::session::Session;
use crate::AppCtx;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
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
        return (StatusCode::BAD_GATEWAY, "session_unavailable").into_response();
    };
    // 2. 边缘 token 校验(从 query 拿)
    let supplied = query_map.get("token").map(|s| s.as_str()).unwrap_or("");
    let expected = sess.token.lock().await.clone();
    if expected.is_empty() || supplied != expected {
        return (StatusCode::UNAUTHORIZED, "invalid_token").into_response();
    }
    // 3. body 大小 64 KiB 上限
    if body.len() > 64 * 1024 {
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
        Ok(_) => (StatusCode::BAD_GATEWAY, "bad_tunnel_resp").into_response(),
        Err(_) => {
            sess.pending.lock().await.remove(&req_id);
            (StatusCode::GATEWAY_TIMEOUT, "tunnel_timeout").into_response()
        }
    };
    resp
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
        });
        (sess, rx)
    }

    #[tokio::test]
    async fn tunnel_rejects_wrong_token() {
        let ctx = Arc::new(AppCtx {
            sessions: crate::session::SessionTable::new(),
            config: crate::config::Config {
                listen: "0.0.0.0:0".parse().unwrap(),
                session_grace_secs: 60,
                tunnel_timeout_secs: 30,
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
            config: crate::config::Config {
                listen: "0.0.0.0:0".parse().unwrap(),
                session_grace_secs: 60,
                tunnel_timeout_secs: 5,
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
}
