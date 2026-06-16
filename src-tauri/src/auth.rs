//! Token 鉴权（spec §11.5 / §13.1）。

use rand::Rng;

const TOKEN_LEN: usize = 24;
const TOKEN_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// 生成随机访问 token，用于主持人端 URL 与所有 API 请求。
pub fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    (0..TOKEN_LEN)
        .map(|_| TOKEN_ALPHABET[rng.gen_range(0..TOKEN_ALPHABET.len())] as char)
        .collect()
}

/// 校验请求 token 是否与当前 token 一致。空 token 一律拒绝。
pub fn verify(provided: &str, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    constant_time_eq::constant_time_eq(provided.as_bytes(), expected.as_bytes())
}
