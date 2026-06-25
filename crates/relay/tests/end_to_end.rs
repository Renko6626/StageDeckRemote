use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use protocol::{ControlMessage, PROTOCOL_VERSION};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn end_to_end_tunnel_and_stream() {
    // 1. Start relay on loopback with OS-assigned port.
    let cfg = relay::config::Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        session_grace_secs: 60,
        tunnel_timeout_secs: 5,
    };
    let listener = tokio::net::TcpListener::bind(cfg.listen).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ctx = Arc::new(relay::AppCtx {
        sessions: relay::session::SessionTable::new(),
        config: cfg,
    });
    let app = relay::build_router(ctx.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = format!("http://{}", addr);
    let ws_base = format!("ws://{}", addr);

    // 2. Mock director connects to /director/control, sends ClientHello, receives ServerHello.
    let sid = "PROM-2026".to_string();
    let token = "TOK123".to_string();
    let secret = "SEC".to_string();
    let (control_ws, _) =
        tokio_tungstenite::connect_async(format!("{}/director/control", ws_base))
            .await
            .unwrap();
    let (mut c_sink, mut c_stream) = control_ws.split();
    c_sink
        .send(Message::Text(
            serde_json::to_string(&ControlMessage::ClientHello {
                session_id: sid.clone(),
                token: token.clone(),
                secret: secret.clone(),
                version: PROTOCOL_VERSION,
            })
            .unwrap(),
        ))
        .await
        .unwrap();
    if let Some(Ok(Message::Text(t))) = c_stream.next().await {
        let m: ControlMessage = serde_json::from_str(&t).unwrap();
        assert!(
            matches!(m, ControlMessage::ServerHello { ok: true, .. }),
            "expected ServerHello ok=true, got: {:?}",
            m
        );
    } else {
        panic!("no ServerHello received from relay");
    }

    // 3. Spawn mock director task that owns both WS halves.
    //    On the first TunnelReq it receives, reply with TunnelResp status=200, body="pong".
    let dir_handle = tokio::spawn(async move {
        while let Some(Ok(Message::Text(t))) = c_stream.next().await {
            let m: ControlMessage = serde_json::from_str(&t).unwrap();
            if let ControlMessage::TunnelReq { req_id, .. } = m {
                let resp = ControlMessage::TunnelResp {
                    req_id,
                    status: 200,
                    headers: vec![],
                    body_b64: base64::engine::general_purpose::STANDARD.encode(b"pong"),
                };
                c_sink
                    .send(Message::Text(serde_json::to_string(&resp).unwrap()))
                    .await
                    .unwrap();
                break;
            }
        }
    });

    // 4. Phone GET /r/<sid>/api/status?token=...; expect 200 "pong".
    let client = reqwest::Client::new();
    let phone_resp = tokio::time::timeout(
        Duration::from_secs(5),
        client
            .get(format!("{}/r/{}/api/status?token={}", base, sid, token))
            .send(),
    )
    .await
    .expect("phone request timed out")
    .expect("phone request errored");
    assert_eq!(phone_resp.status(), 200);
    assert_eq!(phone_resp.text().await.unwrap(), "pong");

    // 5. Ensure the mock director task finished after replying to the first TunnelReq.
    let _ = tokio::time::timeout(Duration::from_secs(2), dir_handle).await;
}

#[tokio::test]
#[ignore = "stream frames test enabled in Windows runtime verification"]
async fn end_to_end_stream_frames_through() {
    // enabled when relay stream broker side is exercised (Task 7 e2e on Windows)
    //
    // Outline:
    //   1. Start relay, mock director connects both /director/control and /director/stream.
    //   2. Director pushes a Binary frame over the stream WS.
    //   3. Phone connects to /r/<sid>/api/stream.mjpg?token=..., reads the first
    //      chunked response chunk, asserts it contains "--frame" and "Content-Length".
    //
    // The relay stream broker (watch channel + WatchStream) is exercised in the unit
    // tests in phone_routes.rs (stream_emits_multipart_after_frame_pushed). Full
    // end-to-end including the director stream_loop (Task 7) requires a Windows host.
}
