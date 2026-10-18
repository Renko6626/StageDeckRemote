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

/// Log before entering COM. Timeout never cancels or retries the blocking call.
pub async fn run_operation<T: Send + 'static>(
    state: std::sync::Arc<AppState>,
    label: String,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    run_operation_timeout(state, label, std::time::Duration::from_secs(8), work).await
}

async fn run_operation_timeout<T: Send + 'static>(
    state: std::sync::Arc<AppState>,
    label: String,
    timeout: std::time::Duration,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let label = format!("操作#{id} {label}");
    // Let a short status probe finish before rejecting an explicit operation.
    // Waiting on the existing semaphore also prevents new probes from overtaking it.
    let permit = match tokio::time::timeout(
        std::time::Duration::from_millis(500),
        state.ppt_gate.clone().acquire_owned(),
    ).await {
        Ok(Ok(permit)) => permit,
        _ => {
            let error = "PowerPoint 忙，通道占用超过 500ms，本次操作未执行；请稍后重试，持续忙碌请检查弹窗".to_string();
            event(&state, format!("{label} 拒绝：{error}"));
            return Err(error);
        }
    };
    event(&state, format!("{label} 开始"));
    let worker_state = state.clone();
    let worker_label = label.clone();
    let started = std::time::Instant::now();
    let task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
            .unwrap_or_else(|_| Err("执行线程 panic".into()));
        let outcome = match &result {
            Ok(_) => "成功".to_string(),
            Err(error) => format!("失败：{error}"),
        };
        event(&worker_state, format!("{worker_label} {outcome}，耗时 {}ms", started.elapsed().as_millis()));
        result
    });
    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            event(&state, format!("{label} 任务异常：{error}"));
            Err(format!("任务异常：{error}"))
        }
        Err(_) => {
            let error = "执行超时，结果未知；调用仍在运行，请检查 PowerPoint/VBA 弹窗，勿重复执行".to_string();
            event(&state, format!("{label} {error}"));
            Err(error)
        }
    }
}

/// Polling errors are recorded on transition, including recovery, without flooding logs.
pub fn ppt_probe(state: &AppState, error: Option<String>) {
    let changed = {
        let mut previous = state.ppt_probe_error.lock().unwrap_or_else(|p| p.into_inner());
        if *previous == error { false } else { *previous = error.clone(); true }
    };
    if changed {
        event(state, match error {
            Some(e) => format!("PPT 状态读取异常：{e}"),
            None => "PPT 状态读取恢复".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};

    #[tokio::test]
    async fn operation_waits_for_a_short_status_probe() {
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let state = Arc::new(AppState::new("secret".into(), 8787, tx));
        let probe = state.ppt_gate.clone().acquire_owned().await.unwrap();
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            drop(probe);
        });
        let result = run_operation(state, "宏 Answer".into(), || Ok("executed")).await;
        release.await.unwrap();
        assert_eq!(result.unwrap(), "executed");
    }

    #[tokio::test]
    async fn timed_out_macro_stays_busy_and_logs_its_late_result() {
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let state = Arc::new(AppState::new("secret".into(), 8787, tx));
        let (release, wait) = std::sync::mpsc::channel();
        let result = run_operation_timeout(state.clone(), "宏 Answer".into(), Duration::from_millis(20), move || {
            wait.recv().unwrap();
            Err::<(), _>("VBA 91".to_string())
        }).await;
        assert!(result.unwrap_err().contains("超时"));
        assert!(state.ppt_gate.clone().try_acquire_owned().is_err());
        let entries = recent(&state, 100);
        assert!(entries.iter().any(|e| e.message.contains("开始") && e.message.contains("Answer")));
        assert!(entries.iter().any(|e| e.message.contains("超时")));
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if recent(&state, 100).iter().any(|e| e.message.contains("VBA 91")) { break; }
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
    }
}
