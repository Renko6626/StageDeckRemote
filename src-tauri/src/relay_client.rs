//! Director 端 Relay 客户端:WS 控制隧道 + WS 二进制推流 + 自动重连 + 把现有 router 当 Service 用。
//!
//! 坏网纪律(spec 2026-09-07 §一):握手 8s 超时;读侧 25s 没有任何入站消息即判半开、主动断开重连;
//! 生命周期(连上 / 断开原因 / 退避)写导播日志,导播能从日志面板看出是哪一段断了。

use crate::app_state::AppState;
use crate::log;
use axum::body::Body;
use axum::http::Request;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use protocol::{ControlMessage, CONNECT_TIMEOUT, LIVENESS_TIMEOUT, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tower::ServiceExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayConfig {
    pub base_url: String,    // e.g. "ws://stagedeck.clab.pku.edu.cn:8080"
    pub session_id: String,
    pub secret: String,
}

#[derive(Clone, Debug)]
pub enum RelayStatus {
    Disabled,
    Connecting,
    Connected,
    Error(String),
}

pub struct RelayHandle {
    pub control_join: tauri::async_runtime::JoinHandle<()>,
    pub stream_join: tauri::async_runtime::JoinHandle<()>,
}

pub fn spawn(state: Arc<AppState>, router: Arc<axum::Router>, cfg: RelayConfig) -> RelayHandle {
    // setup 在同步线程调用此入口，必须显式使用 Tauri runtime。
    let control_join = tauri::async_runtime::spawn(control_loop(state.clone(), router.clone(), cfg.clone()));
    let stream_join = tauri::async_runtime::spawn(stream_loop(state, cfg));
    RelayHandle { control_join, stream_join }
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 带握手超时的 connect:黑洞网络下 TCP 连接可能挂一两分钟,这里 8s 就放弃走退避。
pub(crate) async fn connect_with_timeout(url: &str) -> Result<Ws, String> {
    match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await {
        Ok(Ok((ws, _))) => Ok(ws),
        Ok(Err(e)) => Err(format!("connect: {e}")),
        Err(_) => Err(format!("握手超时({}s)", CONNECT_TIMEOUT.as_secs())),
    }
}

/// 指数退避:0.5s 起,每次翻倍,5s 封顶,外加 0–300ms 抖动。
struct Backoff {
    ms: u64,
}

impl Backoff {
    fn new() -> Self {
        Self { ms: 500 }
    }
    fn reset(&mut self) {
        self.ms = 500;
    }
    /// 返回这次要睡的时长并推进到下一档。
    fn next_delay(&mut self) -> Duration {
        let d = Duration::from_millis(self.ms + rand_jitter_ms());
        self.ms = (self.ms * 2).min(5000);
        d
    }
}

pub(crate) async fn control_loop(state: Arc<AppState>, router: Arc<axum::Router>, cfg: RelayConfig) {
    let mut backoff = Backoff::new();
    let mut attempts: u64 = 0; // 自上次成功以来的尝试次数
    let mut ever_connected = false;
    loop {
        attempts += 1;
        *state.relay_status.lock().unwrap() = RelayStatus::Connecting;
        let url = format!("{}/director/control", cfg.base_url);
        let ws = match connect_with_timeout(&url).await {
            Ok(v) => v,
            Err(e) => {
                *state.relay_status.lock().unwrap() = RelayStatus::Error(e.clone());
                let d = backoff.next_delay();
                log::event(&state, format!("Relay 控制通道连接失败：{e}；{:.1}s 后重试", d.as_secs_f32()));
                tokio::time::sleep(d).await;
                continue;
            }
        };
        let (mut sink, mut stream) = ws.split();
        // 首帧 ClientHello
        let token = state.lock().token.clone();
        let hello = ControlMessage::ClientHello {
            session_id: cfg.session_id.clone(),
            token,
            secret: cfg.secret.clone(),
            version: PROTOCOL_VERSION,
        };
        if sink.send(WsMsg::Text(serde_json::to_string(&hello).unwrap())).await.is_err() {
            *state.relay_status.lock().unwrap() = RelayStatus::Error("send hello".into());
            let d = backoff.next_delay();
            log::event(&state, format!("Relay 控制通道发送 ClientHello 失败；{:.1}s 后重试", d.as_secs_f32()));
            tokio::time::sleep(d).await;
            continue;
        }
        // 写者
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ControlMessage>();
        *state.relay_notify.lock().unwrap() = Some(out_tx.clone());
        let writer = tokio::spawn(async move {
            while let Some(msg) = out_rx.recv().await {
                let s = match serde_json::to_string(&msg) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if sink.send(WsMsg::Text(s)).await.is_err() {
                    break;
                }
            }
        });
        // 状态仍是 Connecting,等到 ServerHello{ok:true} 才置 Connected,避免半 RTT 的假绿。
        // 读者:收 ServerHello / TunnelReq → router.call → 推 TunnelResp;Ping → Pong。
        // 任何入站帧都算存活;25s 沉默 = 半开,主动断开重连(Relay 每 10s 必发 Ping)。
        let mut hello_ok = false;
        let reason: String = loop {
            let next = tokio::time::timeout(LIVENESS_TIMEOUT, stream.next()).await;
            let msg = match next {
                Err(_) => break format!("{}s 无心跳（半开）", LIVENESS_TIMEOUT.as_secs()),
                Ok(None) => break "连接被对端关闭".into(),
                Ok(Some(Err(e))) => break format!("WS 错误：{e}"),
                Ok(Some(Ok(m))) => m,
            };
            match msg {
                WsMsg::Text(t) => {
                    let Ok(m): Result<ControlMessage, _> = serde_json::from_str(&t) else {
                        continue;
                    };
                    match m {
                        ControlMessage::TunnelReq {
                            req_id,
                            method,
                            path,
                            query,
                            headers,
                            body_b64,
                        } => {
                            let router = router.clone();
                            let out_tx = out_tx.clone();
                            tokio::spawn(async move {
                                let resp =
                                    dispatch(&router, &method, &path, &query, &headers, &body_b64)
                                        .await;
                                let _ = out_tx.send(ControlMessage::TunnelResp {
                                    req_id,
                                    status: resp.0,
                                    headers: resp.1,
                                    body_b64: resp.2,
                                });
                            });
                        }
                        ControlMessage::Ping { nonce } => {
                            let _ = out_tx.send(ControlMessage::Pong { nonce });
                        }
                        ControlMessage::ServerHello { ok: true, .. } => {
                            hello_ok = true;
                            backoff.reset();
                            *state.relay_status.lock().unwrap() = RelayStatus::Connected;
                            log::event(
                                &state,
                                if ever_connected {
                                    format!("Relay 控制通道已重连（第 {} 次尝试）", attempts)
                                } else {
                                    "Relay 控制通道已连接".to_string()
                                },
                            );
                            ever_connected = true;
                            attempts = 0;
                        }
                        ControlMessage::ServerHello { ok: false, reason } => {
                            let r = reason.unwrap_or_default();
                            *state.relay_status.lock().unwrap() = RelayStatus::Error(r.clone());
                            break format!("被 Relay 拒绝：{r}");
                        }
                        _ => {}
                    }
                }
                WsMsg::Close(_) => break "收到 Close 帧".into(),
                _ => {}
            }
        };
        *state.relay_notify.lock().unwrap() = None;
        if hello_ok {
            *state.relay_status.lock().unwrap() = RelayStatus::Connecting;
        }
        writer.abort();
        // 退避重连
        let d = backoff.next_delay();
        log::event(&state, format!("Relay 控制通道断开：{reason}；{:.1}s 后重连", d.as_secs_f32()));
        tokio::time::sleep(d).await;
    }
}

/// 把 TunnelReq 喂给 router,拿到 (status, headers, body_b64)。
async fn dispatch(
    router: &Arc<axum::Router>,
    method: &str,
    path: &str,
    query: &str,
    headers: &[(String, String)],
    body_b64: &str,
) -> (u16, Vec<(String, String)>, String) {
    let body_bytes =
        base64::engine::general_purpose::STANDARD.decode(body_b64).unwrap_or_default();
    let uri = if query.is_empty() {
        path.to_string()
    } else {
        format!("{}?{}", path, query)
    };
    let mut builder = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let req = match builder.body(Body::from(body_bytes)) {
        Ok(r) => r,
        Err(_) => return (500, vec![], String::new()),
    };
    let router_clone = (**router).clone();
    let resp = match router_clone.oneshot(req).await {
        Ok(r) => r,
        Err(_) => return (500, vec![], String::new()),
    };
    let status = resp.status().as_u16();
    let resp_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
        .collect();
    let body = match resp.into_body().collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => return (500, resp_headers, String::new()),
    };
    let body_b64 = base64::engine::general_purpose::STANDARD.encode(&body);
    (status, resp_headers, body_b64)
}

async fn stream_loop(state: Arc<AppState>, cfg: RelayConfig) {
    let mut backoff = Backoff::new();
    loop {
        let url = format!("{}/director/stream", cfg.base_url);
        let ws = match connect_with_timeout(&url).await {
            Ok(v) => v,
            Err(e) => {
                let d = backoff.next_delay();
                log::event(&state, format!("Relay 推流通道连接失败：{e}；{:.1}s 后重试", d.as_secs_f32()));
                tokio::time::sleep(d).await;
                continue;
            }
        };
        let (mut sink, mut stream) = ws.split();
        let token = state.lock().token.clone();
        let hello = ControlMessage::ClientHello {
            session_id: cfg.session_id.clone(),
            token,
            secret: cfg.secret.clone(),
            version: PROTOCOL_VERSION,
        };
        let reason: String = 'conn: {
            if sink.send(WsMsg::Text(serde_json::to_string(&hello).unwrap())).await.is_err() {
                break 'conn "发送 ClientHello 失败".into();
            }
            // 等 ServerHello — 严格解析。ok=false 或不是 ServerHello 都重连。
            let ok = matches!(
                tokio::time::timeout(LIVENESS_TIMEOUT, stream.next()).await,
                Ok(Some(Ok(WsMsg::Text(t))))
                    if matches!(
                        serde_json::from_str::<ControlMessage>(&t),
                        Ok(ControlMessage::ServerHello { ok: true, .. })
                    )
            );
            if !ok {
                break 'conn "未收到 ServerHello".into();
            }
            backoff.reset();
            log::event(&state, "Relay 推流通道已连接");
            // 订阅 broker 推帧;同时必须 poll 读半边:Relay 每 10s 发协议层 Ping,
            // tungstenite 只有在读侧被 poll 时才会把自动 Pong 写出去,也只有这样才收得到 Close。
            let mut rx = state.capture.subscribe();
            // 新订阅默认认为现值已读；重连时静止幻灯片也必须立即补发。
            rx.mark_changed();
            loop {
                tokio::select! {
                    changed = rx.changed() => {
                        if changed.is_err() {
                            break 'conn "帧 broker 已关闭".into();
                        }
                        let bytes = rx.borrow().jpeg.clone();
                        if bytes.is_empty() {
                            continue;
                        }
                        if sink.send(WsMsg::Binary(bytes)).await.is_err() {
                            break 'conn "发送帧失败".into();
                        }
                    }
                    inbound = tokio::time::timeout(LIVENESS_TIMEOUT, stream.next()) => {
                        match inbound {
                            Err(_) => break 'conn format!("{}s 无心跳（半开）", LIVENESS_TIMEOUT.as_secs()),
                            Ok(None) => break 'conn "连接被对端关闭".into(),
                            Ok(Some(Err(e))) => break 'conn format!("WS 错误：{e}"),
                            Ok(Some(Ok(WsMsg::Close(_)))) => break 'conn "收到 Close 帧".into(),
                            Ok(Some(Ok(_))) => {} // Ping/Pong 等:算存活,继续
                        }
                    }
                }
            }
        };
        let d = backoff.next_delay();
        log::event(&state, format!("Relay 推流通道断开：{reason}；{:.1}s 后重连", d.as_secs_f32()));
        tokio::time::sleep(d).await;
    }
}

fn rand_jitter_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (n % 300) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatch_runs_router_and_returns_body() {
        // 构造一个最小 router:对 GET /ping 返回 "pong"
        let router: Arc<axum::Router> = Arc::new(axum::Router::new().route(
            "/ping",
            axum::routing::get(|| async { "pong" }),
        ));
        let (status, _h, b64) = dispatch(&router, "GET", "/ping", "", &[], "").await;
        assert_eq!(status, 200);
        let bytes = base64::engine::general_purpose::STANDARD.decode(&b64).unwrap();
        assert_eq!(&bytes[..], b"pong");
    }

    #[test]
    fn backoff_doubles_and_caps_at_5s() {
        let mut b = Backoff::new();
        let d1 = b.next_delay().as_millis() as u64;
        let d2 = b.next_delay().as_millis() as u64;
        let d3 = b.next_delay().as_millis() as u64;
        let d4 = b.next_delay().as_millis() as u64;
        let d5 = b.next_delay().as_millis() as u64;
        assert!((500..800).contains(&d1), "{d1}");
        assert!((1000..1300).contains(&d2), "{d2}");
        assert!((2000..2300).contains(&d3), "{d3}");
        assert!((4000..4300).contains(&d4), "{d4}");
        assert!((5000..5300).contains(&d5), "{d5}");
        b.reset();
        assert!((500..800).contains(&(b.next_delay().as_millis() as u64)));
    }

    #[tokio::test(start_paused = true)]
    async fn connect_gives_up_on_black_hole_within_timeout() {
        // 只 accept 不做 WS 握手的 listener = 黑洞。
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hold = tokio::spawn(async move {
            let (_sock, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let t0 = tokio::time::Instant::now();
        let r = connect_with_timeout(&format!("ws://{addr}/director/control")).await;
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("握手超时"));
        // 暂停时钟:超时定时器就是唯一推进时间的东西,elapsed 必须恰好被 CONNECT_TIMEOUT 封住。
        assert!(t0.elapsed() >= CONNECT_TIMEOUT);
        assert!(t0.elapsed() < CONNECT_TIMEOUT + Duration::from_secs(1));
        hold.abort();
    }

    fn test_state() -> Arc<AppState> {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        Arc::new(AppState::new("tok".into(), 0, tx))
    }

    #[test]
    fn spawn_from_sync_setup_connects_both_channels() {
        // Tauri setup 在同步线程上执行，不存在当前 Tokio runtime。
        assert!(tokio::runtime::Handle::try_current().is_err());
        let (addr, _) = tauri::async_runtime::block_on(mock_relay(true));
        let state = test_state();
        let cfg = RelayConfig {
            base_url: format!("ws://{addr}"),
            session_id: "SHOW-2026".into(),
            secret: "secret".into(),
        };
        let handle = spawn(state.clone(), Arc::new(axum::Router::new()), cfg);
        let connected = tauri::async_runtime::block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let logs = log::recent(&state, 100);
                    if logs.iter().any(|e| e.message == "Relay 控制通道已连接")
                        && logs.iter().any(|e| e.message == "Relay 推流通道已连接")
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await
        });
        handle.control_join.abort();
        handle.stream_join.abort();
        assert!(connected.is_ok(), "Relay channels did not connect from synchronous setup");
    }

    /// mock relay:接受连接、完成 WS 握手、读 ClientHello、回 ServerHello ok,然后按 `silent` 决定是否沉默。
    async fn mock_relay(silent: bool) -> (std::net::SocketAddr, Arc<std::sync::atomic::AtomicU64>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let conns = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let conns2 = conns.clone();
        tokio::spawn(async move {
            loop {
                let (sock, _) = listener.accept().await.unwrap();
                conns2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::spawn(async move {
                    let Ok(ws) = tokio_tungstenite::accept_async(sock).await else { return };
                    let (mut sink, mut stream) = ws.split();
                    let Some(Ok(WsMsg::Text(_hello))) = stream.next().await else { return };
                    let hello = ControlMessage::ServerHello { ok: true, reason: None };
                    let _ = sink.send(WsMsg::Text(serde_json::to_string(&hello).unwrap())).await;
                    if silent {
                        // 保持连接但什么都不发,也不回 Pong —— 模拟半开
                        while let Some(Ok(_)) = stream.next().await {}
                    }
                });
            }
        });
        (addr, conns)
    }

    #[tokio::test(start_paused = true)]
    async fn control_loop_logs_reconnect_after_relay_closes() {
        // mock relay 握手后立刻关连接(silent=false 分支:任务结束 → socket drop)。
        let (addr, conns) = mock_relay(false).await;
        let state = test_state();
        let cfg = RelayConfig { base_url: format!("ws://{addr}"), session_id: "T-2".into(), secret: "s".into() };
        let task = tokio::spawn(control_loop(state.clone(), Arc::new(axum::Router::new()), cfg));
        // 暂停时钟下 sleep 会瞬间跳过,所以不能"睡一会再看",要轮询日志直到出现重连记录。
        let has_reconnect = |state: &AppState| {
            log::recent(state, 100).iter().any(|e| e.message.starts_with("Relay 控制通道已重连（第"))
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while !has_reconnect(&state) {
            assert!(tokio::time::Instant::now() < deadline, "director never reconnected");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        task.abort();
        assert!(conns.load(std::sync::atomic::Ordering::SeqCst) >= 2);
        let logs: Vec<String> = log::recent(&state, 100).into_iter().map(|e| e.message).collect();
        assert!(logs.iter().any(|m| m == "Relay 控制通道已连接"), "{logs:?}");
        assert!(logs.iter().any(|m| m.contains("后重连")), "{logs:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn control_loop_drops_silent_relay_and_reconnects() {
        let (addr, conns) = mock_relay(true).await;
        let state = test_state();
        let router = Arc::new(axum::Router::new());
        let cfg = RelayConfig {
            base_url: format!("ws://{addr}"),
            session_id: "T-1".into(),
            secret: "s".into(),
        };
        let task = tokio::spawn(control_loop(state.clone(), router, cfg));
        // 等到第二次连接出现(= 第一次被判半开后重连)。暂停时钟自动推进,不会真等 25s。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while conns.load(std::sync::atomic::Ordering::SeqCst) < 2 {
            assert!(tokio::time::Instant::now() < deadline, "director never reconnected");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        task.abort();
        let logs: Vec<String> = log::recent(&state, 100).into_iter().map(|e| e.message).collect();
        assert!(logs.iter().any(|m| m.contains("Relay 控制通道已连接")), "{logs:?}");
        assert!(logs.iter().any(|m| m.contains("无心跳")), "{logs:?}");
        assert!(logs.iter().any(|m| m.contains("后重连")), "{logs:?}");
    }
}
