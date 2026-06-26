//! Director 端 Relay 客户端:WS 控制隧道 + WS 二进制推流 + 自动重连 + 把现有 router 当 Service 用。

use crate::app_state::AppState;
use axum::body::Body;
use axum::http::Request;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use protocol::{ControlMessage, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMsg;
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
    pub control_join: tokio::task::JoinHandle<()>,
    pub stream_join: tokio::task::JoinHandle<()>,
}

pub fn spawn(state: Arc<AppState>, router: Arc<axum::Router>, cfg: RelayConfig) -> RelayHandle {
    let control_join = tokio::spawn(control_loop(state.clone(), router.clone(), cfg.clone()));
    let stream_join = tokio::spawn(stream_loop(state, cfg));
    RelayHandle { control_join, stream_join }
}

async fn control_loop(state: Arc<AppState>, router: Arc<axum::Router>, cfg: RelayConfig) {
    let mut backoff_ms: u64 = 500;
    loop {
        *state.relay_status.lock().unwrap() = RelayStatus::Connecting;
        let url = format!("{}/director/control", cfg.base_url);
        let conn = tokio_tungstenite::connect_async(&url).await;
        let (ws, _) = match conn {
            Ok(v) => v,
            Err(e) => {
                *state.relay_status.lock().unwrap() = RelayStatus::Error(format!("connect: {e}"));
                tokio::time::sleep(Duration::from_millis(backoff_ms + rand_jitter_ms())).await;
                backoff_ms = (backoff_ms * 2).min(5000);
                continue;
            }
        };
        backoff_ms = 500;
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
        // 读者:收 ServerHello / TunnelReq → router.call → 推 TunnelResp
        while let Some(Ok(msg)) = stream.next().await {
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
                            *state.relay_status.lock().unwrap() = RelayStatus::Connected;
                        }
                        ControlMessage::ServerHello { ok: false, reason } => {
                            *state.relay_status.lock().unwrap() =
                                RelayStatus::Error(reason.unwrap_or_default());
                            break;
                        }
                        _ => {}
                    }
                }
                WsMsg::Close(_) => break,
                _ => {}
            }
        }
        *state.relay_notify.lock().unwrap() = None;
        *state.relay_status.lock().unwrap() = RelayStatus::Connecting;
        writer.abort();
        // 退避重连
        tokio::time::sleep(Duration::from_millis(backoff_ms + rand_jitter_ms())).await;
        backoff_ms = (backoff_ms * 2).min(5000);
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
    let mut backoff_ms: u64 = 500;
    // 任意"早期失败 → 退避后重连"路径都走这个 macro,免得忘了 sleep 跌进热循环。
    macro_rules! backoff_continue {
        () => {{
            tokio::time::sleep(Duration::from_millis(backoff_ms + rand_jitter_ms())).await;
            backoff_ms = (backoff_ms * 2).min(5000);
            continue;
        }};
    }
    loop {
        let url = format!("{}/director/stream", cfg.base_url);
        let Ok((ws, _)) = tokio_tungstenite::connect_async(&url).await else {
            backoff_continue!();
        };
        backoff_ms = 500;
        let (mut sink, mut stream) = ws.split();
        let token = state.lock().token.clone();
        let hello = ControlMessage::ClientHello {
            session_id: cfg.session_id.clone(),
            token,
            secret: cfg.secret.clone(),
            version: PROTOCOL_VERSION,
        };
        if sink.send(WsMsg::Text(serde_json::to_string(&hello).unwrap())).await.is_err() {
            backoff_continue!();
        }
        // 等 ServerHello — 严格解析。ok=false 或不是 ServerHello 都重连。
        let ok = matches!(
            stream.next().await,
            Some(Ok(WsMsg::Text(t)))
                if matches!(
                    serde_json::from_str::<ControlMessage>(&t),
                    Ok(ControlMessage::ServerHello { ok: true, .. })
                )
        );
        if !ok {
            backoff_continue!();
        }
        // 订阅 broker 推帧
        let mut rx = state.capture.subscribe();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let bytes = rx.borrow().jpeg.clone();
            if bytes.is_empty() {
                continue;
            }
            if sink.send(WsMsg::Binary(bytes)).await.is_err() {
                break;
            }
        }
        backoff_continue!();
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
}
