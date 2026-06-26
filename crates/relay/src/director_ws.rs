use crate::AppCtx;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use protocol::ControlMessage;
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
    let sess = match ctx.sessions.register_or_attach(sid.clone(), secret, token, tx).await {
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
    // 5. Reader: receive TunnelResp / TokenUpdate / Pong from the director.
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(t) => {
                let Ok(m): Result<ControlMessage, _> = serde_json::from_str(&t) else {
                    continue;
                };
                handle_director_msg(&sess, m).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    // 6. Director disconnected; update last_seen and let the grace-period cleaner handle teardown.
    *sess.last_seen.lock().await = Instant::now();
    tracing::info!(sid = %sid, "control ws disconnected");
    writer.abort();
}

async fn handle_director_msg(sess: &crate::session::Session, m: ControlMessage) {
    // Inspect by reference first so we can forward the owned `m` whole.
    if let ControlMessage::TunnelResp { req_id, .. } = &m {
        let req_id = *req_id;
        let mut pend = sess.pending.lock().await;
        if let Some(tx) = pend.remove(&req_id) {
            let _ = tx.send(m);
        }
        return;
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
    let _ = send_json(&mut socket, &ControlMessage::ServerHello { ok: true, reason: None }).await;
    // Mark stream connected for admin status; clear when this loop exits.
    sess.stream_connected.store(true, std::sync::atomic::Ordering::SeqCst);
    tracing::info!(sid = %sid, "stream ws connected");
    // Receive binary frames and broadcast via watch channel.
    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Binary(b) => {
                let _ = sess.frame_tx.send(Arc::new(b));
                sess.frames_pushed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                *sess.frame_last_at.lock().await = Some(std::time::Instant::now());
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    sess.stream_connected.store(false, std::sync::atomic::Ordering::SeqCst);
    tracing::info!(
        sid = %sid,
        total_frames = sess.frames_pushed.load(std::sync::atomic::Ordering::Relaxed),
        "stream ws disconnected"
    );
}

async fn send_json(socket: &mut WebSocket, msg: &ControlMessage) -> Result<(), axum::Error> {
    let s = serde_json::to_string(msg).map_err(axum::Error::new)?;
    socket.send(Message::Text(s)).await
}

fn valid_sid(s: &str) -> bool {
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
