use crate::AppCtx;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use protocol::{ControlMessage, HEARTBEAT_INTERVAL, LIVENESS_TIMEOUT};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

pub fn router() -> Router<Arc<AppCtx>> {
    Router::new()
        .route("/director/control", get(control_upgrade))
        .route("/director/stream", get(stream_upgrade))
}

async fn control_upgrade(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<AppCtx>>,
) -> Response {
    ws.on_upgrade(move |s| control_loop(s, ctx))
}

async fn stream_upgrade(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<AppCtx>>,
) -> Response {
    ws.on_upgrade(move |s| stream_loop(s, ctx))
}

async fn control_loop(mut socket: WebSocket, ctx: Arc<AppCtx>) {
    // 1. Wait for the first ClientHello frame.
    let hello = match socket.next().await {
        Some(Ok(Message::Text(t))) => serde_json::from_str::<ControlMessage>(&t).ok(),
        _ => None,
    };
    let (sid, token, secret) = match hello {
        Some(ControlMessage::ClientHello { session_id, token, secret, .. }) => {
            (session_id, token, secret)
        }
        _ => {
            tracing::warn!("control ws closed: missing or malformed ClientHello");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    // 2. Validate session_id format.
    if !valid_sid(&sid) {
        tracing::warn!(sid = %sid, "control ws rejected: bad_sid");
        let _ = send_json(
            &mut socket,
            &ControlMessage::ServerHello { ok: false, reason: Some("bad_sid".into()) },
        )
        .await;
        return;
    }
    // 3. Register or take over existing session.
    let (tx, mut rx) = mpsc::unbounded_channel::<ControlMessage>();
    let sess = match ctx.sessions.register_or_attach(sid.clone(), secret, token, tx.clone()).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(sid = %sid, reason = %e, "control ws rejected by session table");
            let _ = send_json(
                &mut socket,
                &ControlMessage::ServerHello { ok: false, reason: Some(e.into()) },
            )
            .await;
            return;
        }
    };
    let _ = send_json(&mut socket, &ControlMessage::ServerHello { ok: true, reason: None }).await;
    // 4. Writer: drain mpsc and forward to WebSocket.
    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let txt = match serde_json::to_string(&msg) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if sink.send(Message::Text(txt)).await.is_err() {
                break;
            }
        }
    });
    // 4b. Pinger: every HEARTBEAT_INTERVAL push a JSON Ping through the same writer,
    //     so the director always has something inbound to prove the link alive.
    let pinger = {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut nonce: u64 = 0;
            loop {
                tokio::time::sleep(HEARTBEAT_INTERVAL).await;
                nonce += 1;
                if tx.send(ControlMessage::Ping { nonce }).is_err() {
                    break;
                }
            }
        })
    };
    // 5. Reader: receive TunnelResp / TokenUpdate / Pong from the director.
    //    Any inbound frame counts as liveness; LIVENESS_TIMEOUT of silence = half-open → drop.
    let disconnect_reason = loop {
        match tokio::time::timeout(LIVENESS_TIMEOUT, stream.next()).await {
            Err(_) => break "liveness_timeout",
            Ok(Some(Ok(Message::Text(t)))) => {
                let Ok(m): Result<ControlMessage, _> = serde_json::from_str(&t) else {
                    continue;
                };
                if !handle_director_msg(&sess, &tx, m).await {
                    break "replaced";
                }
            }
            Ok(Some(Ok(Message::Close(_)))) => break "close_frame",
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(_))) => break "ws_error",
            Ok(None) => break "eof",
        }
    };
    // 6. Director disconnected: stop writer/pinger, fail requests still waiting on THIS
    //    connection, update last_seen and let the grace-period reaper handle teardown.
    writer.abort();
    let _ = writer.await; // 等 rx 真正释放，离线请求不再排入无人消费的队列。
    pinger.abort();
    // 只有当 session 的 control_tx 仍指向本连接(已随 writer 退出而关闭)时才清 pending;
    // 若新连接已接管,pending 里是新连接的请求,不能动。
    let current_control = sess.control_tx.lock().await;
    let dropped = if current_control.same_channel(&tx) {
        *sess.last_seen.lock().await = Instant::now();
        sess.fail_pending().await
    } else {
        0
    };
    drop(current_control);
    tracing::info!(sid = %sid, reason = disconnect_reason, dropped_pending = dropped, "control ws disconnected");
}

async fn handle_director_msg(
    sess: &crate::session::Session,
    connection: &mpsc::UnboundedSender<ControlMessage>,
    m: ControlMessage,
) -> bool {
    // 与注册接管、请求入队使用同一锁，身份检查和状态更新不可被接管穿插。
    let current_control = sess.control_tx.lock().await;
    if !current_control.same_channel(connection) { return false; }
    // Inspect by reference first so we can forward the owned `m` whole.
    if let ControlMessage::TunnelResp { req_id, .. } = &m {
        let req_id = *req_id;
        let mut pend = sess.pending.lock().await;
        if let Some(tx) = pend.remove(&req_id) {
            let _ = tx.send(m);
        }
        return true;
    }
    match m {
        ControlMessage::TokenUpdate { token } => {
            *sess.token.lock().await = token;
        }
        ControlMessage::Pong { .. } => {
            *sess.last_seen.lock().await = Instant::now();
        }
        _ => {}
    }
    true
}

async fn stream_loop(mut socket: WebSocket, ctx: Arc<AppCtx>) {
    // First frame: ClientHello for sid + secret verification.
    let hello = match socket.next().await {
        Some(Ok(Message::Text(t))) => serde_json::from_str::<ControlMessage>(&t).ok(),
        _ => None,
    };
    let (sid, secret) = match hello {
        Some(ControlMessage::ClientHello { session_id, secret, .. }) => (session_id, secret),
        _ => {
            tracing::warn!("stream ws closed: missing or malformed ClientHello");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    let sess = match ctx.sessions.get(&sid).await {
        Some(s) if s.secret == secret => s,
        _ => {
            tracing::warn!(sid = %sid, "stream ws rejected: unknown sid or secret mismatch");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    let owner = Arc::new(());
    {
        let mut current = sess.stream_owner.lock().await;
        *current = Some(owner.clone());
        sess.stream_connected.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    let _ = send_json(&mut socket, &ControlMessage::ServerHello { ok: true, reason: None }).await;
    tracing::info!(sid = %sid, "stream ws connected");
    // Protocol-level Ping every HEARTBEAT_INTERVAL; tungstenite on the director auto-Pongs,
    // and that Pong (or any frame) resets the liveness clock below.
    let (mut sink, mut stream) = socket.split();
    let pinger = tokio::spawn(async move {
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            if sink.send(Message::Ping(Vec::new())).await.is_err() {
                break;
            }
        }
    });
    // Receive binary frames and broadcast via watch channel.
    let disconnect_reason = loop {
        let inbound = tokio::time::timeout(LIVENESS_TIMEOUT, stream.next()).await;
        let current = sess.stream_owner.lock().await;
        if !current.as_ref().is_some_and(|id| Arc::ptr_eq(id, &owner)) {
            break "replaced";
        }
        match inbound {
            Err(_) => break "liveness_timeout",
            Ok(Some(Ok(Message::Binary(b)))) => {
                sess.frame_tx.send_replace(Arc::new(b));
                sess.frames_pushed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                *sess.frame_last_at.lock().await = Some(std::time::Instant::now());
            }
            Ok(Some(Ok(Message::Close(_)))) => break "close_frame",
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(_))) => break "ws_error",
            Ok(None) => break "eof",
        }
    };
    pinger.abort();
    {
        let mut current = sess.stream_owner.lock().await;
        if current.as_ref().is_some_and(|id| Arc::ptr_eq(id, &owner)) {
            *current = None;
            sess.stream_connected.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    tracing::info!(
        sid = %sid,
        reason = disconnect_reason,
        total_frames = sess.frames_pushed.load(std::sync::atomic::Ordering::Relaxed),
        "stream ws disconnected"
    );
}

async fn send_json(socket: &mut WebSocket, msg: &ControlMessage) -> Result<(), axum::Error> {
    let s = serde_json::to_string(msg).map_err(axum::Error::new)?;
    socket.send(Message::Text(s)).await
}

pub(crate) fn valid_sid(s: &str) -> bool {
    let len = s.len();
    (4..=16).contains(&len)
        && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::valid_sid;

    #[test]
    fn sid_format_rules() {
        assert!(valid_sid("PROM-2026"));
        assert!(valid_sid("A1B2"));
        assert!(!valid_sid("ab12")); // lowercase
        assert!(!valid_sid("AB")); // too short
        assert!(!valid_sid("A".repeat(17).as_str())); // too long
    }
}
