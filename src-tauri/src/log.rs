//! 关键事件日志（spec §11.12）。内存环形缓冲 + stdout。

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::app_state::AppState;

const MAX_LOGS: usize = 500;

#[derive(Clone, Debug, Serialize)]
pub struct LogEntry {
    /// Unix 毫秒时间戳。
    pub time_ms: u64,
    pub message: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 记录一条事件：写入内存缓冲并打印到 stdout。
pub fn event(state: &AppState, message: impl Into<String>) {
    let message = message.into();
    let entry = LogEntry {
        time_ms: now_ms(),
        message: message.clone(),
    };
    {
        let mut inner = state.lock();
        inner.logs.push(entry);
        let len = inner.logs.len();
        if len > MAX_LOGS {
            inner.logs.drain(0..len - MAX_LOGS);
        }
    }
    println!("[stagedeck] {message}");
}

/// 取最近 `max` 条日志（导播端 GUI 展示用）。
pub fn recent(state: &AppState, max: usize) -> Vec<LogEntry> {
    let inner = state.lock();
    let start = inner.logs.len().saturating_sub(max);
    inner.logs[start..].to_vec()
}
