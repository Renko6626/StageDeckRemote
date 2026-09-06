//! StageDeck Relay 控制通道 wire types + 共享小工具(纯函数,无 axum/tokio 依赖)。

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;

use std::time::{Duration, Instant};

/// Relay → 导播心跳间隔(控制 WS 发 JSON Ping,推流 WS 发协议层 Ping)。
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// 读侧存活超时:超过这么久没有任何入站消息即判半开,主动断开重连。
pub const LIVENESS_TIMEOUT: Duration = Duration::from_secs(25);
/// WS 握手(含 TCP 连接)超时,防黑洞网络下挂一两分钟。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// 距最近一次入站消息是否已超过存活超时。纯函数,两端共用。
pub fn liveness_expired(last_inbound: Instant, now: Instant) -> bool {
    now.saturating_duration_since(last_inbound) > LIVENESS_TIMEOUT
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum ControlMessage {
    ClientHello { session_id: String, token: String, secret: String, version: u8 },
    ServerHello { ok: bool, reason: Option<String> },
    TunnelReq {
        req_id: u64,
        method: String,
        path: String,
        query: String,
        headers: Vec<(String, String)>,
        body_b64: String,
    },
    TunnelResp {
        req_id: u64,
        status: u16,
        headers: Vec<(String, String)>,
        body_b64: String,
    },
    TokenUpdate { token: String },
    Ping { nonce: u64 },
    Pong { nonce: u64 },
}

/// 把一段 JPEG 包成一个 multipart/x-mixed-replace 帧段。
pub fn mjpeg_part(jpeg: &[u8]) -> Vec<u8> {
    let header = format!(
        "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
        jpeg.len()
    );
    let mut out = Vec::with_capacity(header.len() + jpeg.len() + 2);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(jpeg);
    out.extend_from_slice(b"\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_message_roundtrips_json() {
        let msg = ControlMessage::ClientHello {
            session_id: "PROM-2026".into(),
            token: "abc".into(),
            secret: "xyz".into(),
            version: PROTOCOL_VERSION,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""type":"ClientHello""#));
        let back: ControlMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn tunnel_req_resp_pair_roundtrips() {
        let req = ControlMessage::TunnelReq {
            req_id: 42, method: "POST".into(), path: "/api/next".into(),
            query: "".into(), headers: vec![("content-type".into(), "application/json".into())],
            body_b64: "e30=".into(),
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: ControlMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, req);
        let resp = ControlMessage::TunnelResp {
            req_id: 42, status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body_b64: "eyJvayI6dHJ1ZX0=".into(),
        };
        let s = serde_json::to_string(&resp).unwrap();
        let back: ControlMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn liveness_expires_only_past_timeout() {
        let t0 = Instant::now();
        assert!(!liveness_expired(t0, t0));
        assert!(!liveness_expired(t0, t0 + LIVENESS_TIMEOUT));
        assert!(liveness_expired(t0, t0 + LIVENESS_TIMEOUT + Duration::from_millis(1)));
        // 时钟倒退(now < last)不算过期
        assert!(!liveness_expired(t0 + Duration::from_secs(5), t0));
    }

    #[test]
    fn mjpeg_part_frames_with_headers() {
        let part = mjpeg_part(&[0xAA, 0xBB, 0xCC]);
        let text = String::from_utf8_lossy(&part);
        assert!(text.starts_with("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: 3\r\n\r\n"));
        assert_eq!(&part[part.len() - 5..], &[0xAA, 0xBB, 0xCC, b'\r', b'\n']);
    }
}
