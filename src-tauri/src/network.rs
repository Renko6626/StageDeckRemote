//! 局域网地址发现（spec §11.10）。

use std::net::{IpAddr, UdpSocket};

/// 获取本机对外的局域网 IPv4 地址。
///
/// 技巧：把一个 UDP socket "connect" 到一个外部地址（并不真正发包），
/// 内核会据此选择出口网卡，`local_addr()` 即得到该网卡的本地 IP。
/// 跨平台、无需第三方依赖。
pub fn local_ipv4() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(v4) => Some(v4.to_string()),
        IpAddr::V6(_) => None,
    }
}

/// 拼接主持人端访问 URL。
pub fn access_url(ip: &str, port: u16, token: &str) -> String {
    format!("http://{ip}:{port}/presenter?token={token}")
}
