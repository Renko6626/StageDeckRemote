//! StageDeck Relay 控制通道 wire types + 共享小工具(纯函数,无 axum/tokio 依赖)。

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;

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
    fn mjpeg_part_frames_with_headers() {
        let part = mjpeg_part(&[0xAA, 0xBB, 0xCC]);
        let text = String::from_utf8_lossy(&part);
        assert!(text.starts_with("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: 3\r\n\r\n"));
        assert_eq!(&part[part.len() - 5..], &[0xAA, 0xBB, 0xCC, b'\r', b'\n']);
    }
}
