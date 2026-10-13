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
        admin_token: None,
    };
    let listener = tokio::net::TcpListener::bind(cfg.listen).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ctx = Arc::new(relay::AppCtx {
        sessions: relay::session::SessionTable::new(),
        started_at: std::time::Instant::now(),
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

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect_stream(ws_base: &str, sid: &str) -> Ws {
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("{ws_base}/director/stream")).await.unwrap();
    ws.send(Message::Text(serde_json::to_string(&ControlMessage::ClientHello {
        session_id: sid.into(), token: "TOK".into(), secret: "SEC".into(), version: PROTOCOL_VERSION,
    }).unwrap())).await.unwrap();
    let hello = ws.next().await.unwrap().unwrap();
    assert!(matches!(hello, Message::Text(t) if matches!(serde_json::from_str::<ControlMessage>(&t), Ok(ControlMessage::ServerHello { ok: true, .. }))));
    ws
}

async fn wait_frames(ctx: &relay::AppCtx, sid: &str, count: u64) {
    tokio::time::timeout(Duration::from_secs(2), async {
        let sess = ctx.sessions.get(sid).await.unwrap();
        while sess.frames_pushed.load(std::sync::atomic::Ordering::Relaxed) < count {
            tokio::task::yield_now().await;
        }
    }).await.expect("relay did not receive the frame");
}

#[tokio::test]
async fn end_to_end_stream_frames_through() {
    let (base, ws_base, ctx) = start_relay(2).await;
    let (_control, _reader) = connect_director(&ws_base, "LATE-VIEWER", "TOK").await;
    let mut stream = connect_stream(&ws_base, "LATE-VIEWER").await;
    stream.send(Message::Binary(b"first-frame".to_vec())).await.unwrap();
    wait_frames(&ctx, "LATE-VIEWER", 1).await;
    let mut response = reqwest::get(format!("{base}/r/LATE-VIEWER/api/stream.mjpg?token=TOK")).await.unwrap();
    let chunk = tokio::time::timeout(Duration::from_secs(1), response.chunk()).await
        .expect("late viewer did not receive the static frame").unwrap().unwrap();
    assert!(chunk.windows(b"first-frame".len()).any(|w| w == b"first-frame"));
}

#[tokio::test]
async fn stream_takeover_keeps_new_connection_online() {
    let (base, ws_base, ctx) = start_relay(2).await;
    let (_control, _reader) = connect_director(&ws_base, "STREAM-SWAP", "TOK").await;
    let mut old = connect_stream(&ws_base, "STREAM-SWAP").await;
    let mut new = connect_stream(&ws_base, "STREAM-SWAP").await;
    old.send(Message::Binary(b"stale-frame".to_vec())).await.unwrap();
    old.close(None).await.unwrap();
    // Observe the old connection closing before checking the new connection's status.
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(Ok(msg)) = old.next().await {
            if matches!(msg, Message::Close(_)) { break; }
        }
    }).await.unwrap();
    let sess = ctx.sessions.get("STREAM-SWAP").await.unwrap();
    assert_eq!(sess.frames_pushed.load(std::sync::atomic::Ordering::Relaxed), 0, "old stream must not publish after takeover");
    new.send(Message::Binary(b"current-frame".to_vec())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while sess.frame_last_at.lock().await.is_none() { tokio::task::yield_now().await; }
    }).await.unwrap();
    // A fresh viewer must see only the new stream, including after old cleanup.
    let mut response = reqwest::get(format!("{base}/r/STREAM-SWAP/api/stream.mjpg?token=TOK")).await.unwrap();
    let chunk = tokio::time::timeout(Duration::from_secs(1), response.chunk()).await.unwrap().unwrap().unwrap();
    assert!(chunk.windows(b"current-frame".len()).any(|w| w == b"current-frame"));
    assert!(sess.stream_connected.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn old_control_cannot_replace_new_token() {
    let (_base, ws_base, ctx) = start_relay(2).await;
    let (mut old, mut old_reader) = connect_director(&ws_base, "CTRL-SWAP", "OLD").await;
    let (_new, _new_reader) = connect_director(&ws_base, "CTRL-SWAP", "NEW").await;
    old.send(Message::Text(serde_json::to_string(&ControlMessage::TokenUpdate { token: "STALE".into() }).unwrap())).await.unwrap();
    old.close().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(Ok(msg)) = old_reader.next().await {
            if matches!(msg, Message::Close(_)) { break; }
        }
    }).await.unwrap();
    let sess = ctx.sessions.get("CTRL-SWAP").await.unwrap();
    assert_eq!(&*sess.token.lock().await, "NEW");
    assert!(!sess.control_tx.lock().await.is_closed());
}

#[tokio::test]
async fn presenter_rejects_sid_with_html_attributes() {
    let (base, _, _) = start_relay(2).await;
    let response = reqwest::get(format!("{base}/r/TEST%22%20data-audit%3D%22injected/presenter")).await.unwrap();
    assert_eq!(response.status(), 400);
    assert!(!response.text().await.unwrap().contains("data-audit"));
}

/// 起一个 loopback relay,返回 (http base, ws base, ctx)。
async fn start_relay(tunnel_timeout_secs: u64) -> (String, String, Arc<relay::AppCtx>) {
    let cfg = relay::config::Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        session_grace_secs: 60,
        tunnel_timeout_secs,
        admin_token: None,
    };
    let listener = tokio::net::TcpListener::bind(cfg.listen).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ctx = Arc::new(relay::AppCtx {
        sessions: relay::session::SessionTable::new(),
        started_at: std::time::Instant::now(),
        config: cfg,
    });
    let app = relay::build_router(ctx.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{}", addr), format!("ws://{}", addr), ctx)
}

/// 模拟导播连上控制 WS 并完成 ClientHello/ServerHello,返回 (sink, stream)。
async fn connect_director(
    ws_base: &str,
    sid: &str,
    token: &str,
) -> (
    futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        Message,
    >,
    futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    >,
) {
    let (ws, _) = tokio_tungstenite::connect_async(format!("{}/director/control", ws_base))
        .await
        .unwrap();
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(
        serde_json::to_string(&ControlMessage::ClientHello {
            session_id: sid.into(),
            token: token.into(),
            secret: "SEC".into(),
            version: PROTOCOL_VERSION,
        })
        .unwrap(),
    ))
    .await
    .unwrap();
    match stream.next().await {
        Some(Ok(Message::Text(t))) => {
            let m: ControlMessage = serde_json::from_str(&t).unwrap();
            assert!(matches!(m, ControlMessage::ServerHello { ok: true, .. }), "{m:?}");
        }
        other => panic!("no ServerHello: {other:?}"),
    }
    (sink, stream)
}

#[tokio::test(start_paused = true)]
async fn relay_pings_director_within_heartbeat_interval() {
    let (_base, ws_base, _ctx) = start_relay(30).await;
    let (_sink, mut stream) = connect_director(&ws_base, "PING-TEST", "TOK").await;
    // 暂停时钟下 runtime 每次空闲都直接跳到下一个定时器,elapsed 没有意义,不能断言时间窗口;
    // 只断言顺序:在 relay 因沉默把我们踢掉(Close)之前,必须至少看到一条 Ping。
    let saw_ping = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(Ok(m)) = stream.next().await {
            match m {
                Message::Text(t) => {
                    if let Ok(ControlMessage::Ping { .. }) = serde_json::from_str::<ControlMessage>(&t) {
                        return true;
                    }
                }
                Message::Close(_) => return false,
                _ => {}
            }
        }
        false
    })
    .await
    .expect("relay neither pinged nor closed within 60s");
    assert!(saw_ping, "relay closed the socket without ever sending a Ping");
}

#[tokio::test(start_paused = true)]
async fn silent_director_is_dropped_and_pending_requests_get_502_fast() {
    let (base, ws_base, _ctx) = start_relay(30).await;
    // 导播连上后完全沉默:不回 Pong,也不回 TunnelResp(模拟半开)。
    let (_sink, mut stream) = connect_director(&ws_base, "DEAD-TEST", "TOK").await;
    let drain = tokio::spawn(async move {
        // 只读不回,直到 relay 关连接
        while let Some(Ok(m)) = stream.next().await {
            if matches!(m, Message::Close(_)) {
                break;
            }
        }
    });
    // 手机在导播沉默期间发一个隧道请求:必须由存活判死清 pending 得到 502 director_offline,
    // 而不是等满 30s 隧道超时得到 504 tunnel_timeout(暂停时钟下只能靠响应体区分路径)。
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/r/DEAD-TEST/api/next?token=TOK", base))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    assert_eq!(resp.text().await.unwrap(), "director_offline");
    // relay 已主动关掉这条 WS
    tokio::time::timeout(Duration::from_secs(5), drain).await.expect("relay never closed the ws").unwrap();
    // 之后的请求直接 502(控制通道已关)
    let resp = client
        .post(format!("{}/r/DEAD-TEST/api/next?token=TOK", base))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    assert_eq!(resp.text().await.unwrap(), "director_offline");
}
