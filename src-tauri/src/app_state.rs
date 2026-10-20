//! 全局运行状态（spec §11.2）。
//!
//! `AppState` 同时被两条通道共享：
//!   * 导播端 GUI —— 通过 Tauri IPC（commands.rs）访问，拥有最高特权；
//!   * 主持人端浏览器 —— 通过 Axum HTTP/WS（routes.rs）访问，受限。
//! 因此它被包成 `Arc<AppState>`，内部用 `Mutex` 保护可变字段。

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::log::LogEntry;

use tokio::sync::mpsc::Sender;

/// 当前被控制的目标窗口（PPT 放映窗口）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetWindow {
    pub title: String,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    /// 平台原生句柄（Windows HWND 以 isize 存储）；0 表示无效。
    pub handle: isize,
}

/// 预制快捷消息(导播拥有、可编辑、同步给主持人)。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresetMessage {
    pub id: String,
    pub text: String,
}

/// 一条要弹给导播的消息;`from` 标注来源("主持人" / "系统")。
#[derive(Clone, Debug, Serialize)]
pub struct HostMessage {
    pub from: String,
    pub text: String,
    pub at_ms: u64,
}

/// 导播→主持人的即时通知(临时,不持久化)。seq 单调自增,供主持人检测新消息。
#[derive(Clone, Debug, Serialize)]
pub struct DirectorMessage {
    pub seq: u64,
    pub text: String,
    pub at_ms: u64,
}

/// 演出计时器状态(临时,不持久化)。
#[derive(Clone, Debug)]
pub struct TimerState {
    pub duration_ms: u64,
    pub running: bool,
    pub started_at_ms: u64,
    pub elapsed_before_ms: u64,
}

/// 计时展示视图(remaining 可为负 = 超时)。
#[derive(Clone, Debug, Serialize)]
pub struct TimerView {
    pub remaining_ms: i64,
    pub running: bool,
    pub duration_ms: u64,
}

// 挂钟时间(非单调)。计时器可容忍偶发回拨:导播 GUI 与主持人端每次轮询都用服务端算好的 remaining_ms 重新锚定,
// 最坏只是一个轮询周期(≤1.5s)的视觉抖动,故不引入 Instant。也用于日志/消息时间戳。
/// 服务端当前毫秒时间。
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 计时剩余毫秒(可为负=超时)。纯函数,便于单测。
pub fn timer_remaining_ms(
    duration_ms: u64,
    running: bool,
    started_at_ms: u64,
    elapsed_before_ms: u64,
    now_ms: u64,
) -> i64 {
    let live = elapsed_before_ms + if running { now_ms.saturating_sub(started_at_ms) } else { 0 };
    duration_ms as i64 - live as i64
}

/// 由计时状态 + 当前时刻算出展示视图。纯函数。
pub fn timer_view_of(t: &TimerState, now_ms: u64) -> TimerView {
    TimerView {
        remaining_ms: timer_remaining_ms(
            t.duration_ms,
            t.running,
            t.started_at_ms,
            t.elapsed_before_ms,
            now_ms,
        ),
        running: t.running,
        duration_ms: t.duration_ms,
    }
}

/// 受 `Mutex` 保护的可变运行状态。
pub struct Inner {
    pub selected_window: Option<TargetWindow>,
    /// 导播端总开关：是否允许远程控制（spec §13.2 第 1 层）。
    pub remote_enabled: bool,
    /// 导播端强制锁（spec §13.2 第 2 层）。
    pub control_locked: bool,
    /// 主持人端本地解锁（spec §13.2 第 3 层）。
    pub presenter_unlocked: bool,
    pub active_client_id: Option<String>,
    /// 当前控制端最后心跳(服务端时钟毫秒);配合 active_client_id 做在线判定。
    pub active_seen_ms: u64,
    pub token: String,
    pub server_port: u16,
    pub logs: Vec<LogEntry>,
    /// 导播可编辑的预制快捷消息(同步给主持人)。
    pub presets: Vec<PresetMessage>,
    /// 导播→主持人的最新通知(临时;主持人轮询 status 取走显示)。
    pub director_message: Option<DirectorMessage>,
    /// 演出计时器(临时;两端轮询 status 取走显示)。
    pub timer: Option<TimerState>,
    /// 动作请求幂等缓存(手机重发同 op_id 只执行一次)。
    pub ops: crate::op_cache::OpCache,
}

pub struct AppState {
    pub diagnostics: Arc<crate::diagnostics::Diagnostics>,
    pub inner: Mutex<Inner>,
    pub capture: crate::capture::CaptureManager,
    /// 收到主持人消息时投递给 Tauri 接收任务(显示软弹窗)。
    /// 有界通道:洪泛(token-gated /api/message)下满则丢弃,绝不阻塞。
    pub message_tx: Sender<HostMessage>,
    /// 导播预览推送任务句柄。subscribe_preview 每次被调用(组件重挂/页面重载)就 abort 上一个,
    /// 保证至多一个推送任务,避免 Tauri Channel 在窗口销毁后 send 不报错导致的任务堆积。
    pub preview_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// Relay 配置(无则不启动 relay 客户端)。
    pub relay_config: Mutex<Option<crate::relay_client::RelayConfig>>,
    /// 当前 relay 客户端连接状态。
    pub relay_status: Mutex<crate::relay_client::RelayStatus>,
    /// 当 relay_client 在跑时,这里有一个 sender;regenerate_token 时通过它推 TokenUpdate 给 relay。
    pub relay_notify: Mutex<Option<tokio::sync::mpsc::UnboundedSender<protocol::ControlMessage>>>,
    /// A timed-out COM call still owns this permit until it actually returns.
    pub ppt_gate: Arc<tokio::sync::Semaphore>,
    pub ppt_probe_error: Mutex<Option<String>>,
}

impl AppState {
    /// 中毒自恢复的锁访问：取走被 poison 的 guard 内部值，绝不传播 panic。
    /// 全仓统一用它取代 `self.inner.lock()`，消除 `.expect` 会 panic、`if let Ok` 会静默 no-op 的不一致。
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn new(token: String, server_port: u16, message_tx: Sender<HostMessage>) -> Self {
        let diagnostics = Arc::new(crate::diagnostics::Diagnostics::default());
        AppState {
            diagnostics: diagnostics.clone(),
            inner: Mutex::new(Inner {
                selected_window: None,
                // 默认:远程关闭(日常总闸)、强制锁释放(仅紧急用)、未解锁。
                // 导播开「远程控制」一步即放行,主持人再 ARM 即可全功能。
                remote_enabled: false,
                control_locked: false,
                presenter_unlocked: false,
                active_client_id: None,
                active_seen_ms: 0,
                token,
                server_port,
                logs: Vec::new(),
                presets: crate::presets::default_presets(),
                director_message: None,
                timer: None,
                ops: crate::op_cache::OpCache::new(),
            }),
            capture: crate::capture::CaptureManager::with_diagnostics(diagnostics),
            message_tx,
            preview_task: Mutex::new(None),
            relay_config: Mutex::new(None),
            relay_status: Mutex::new(crate::relay_client::RelayStatus::Disabled),
            relay_notify: Mutex::new(None),
            ppt_gate: Arc::new(tokio::sync::Semaphore::new(1)),
            ppt_probe_error: Mutex::new(None),
        }
    }

    /// 三层锁全部放行才允许"点击预览图"这类隐式操作（spec §13.2）。
    pub fn click_allowed(&self) -> bool {
        let i = self.lock();
        i.remote_enabled && !i.control_locked && i.presenter_unlocked
    }

    /// 显式按钮（翻页等）只要求远程开启且未被强制锁。
    pub fn button_allowed(&self) -> bool {
        let i = self.lock();
        i.remote_enabled && !i.control_locked
    }
}

/// 客户端在线判定阈值(>该值无心跳视为离线)。
/// 10s:与手机端"弱"档对称,一次 Wi‑Fi 抖动不丢控制权;代价是第二台手机接管要等 10s。
pub const PRESENCE_TIMEOUT_MS: u64 = 10_000;

/// 最近一次心跳是否仍算在线。纯函数。
pub fn client_online(last_seen_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_seen_ms) <= PRESENCE_TIMEOUT_MS
}

/// 记录 cid 心跳并按"先到为主 / 控制端离线可被接管"更新控制端;返回该 cid 是否为控制端。
pub fn note_presence_and_claim(inner: &mut Inner, cid: &str, now_ms: u64) -> bool {
    let controller_live =
        inner.active_client_id.is_some() && client_online(inner.active_seen_ms, now_ms);
    let is_me = inner.active_client_id.as_deref() == Some(cid);
    if is_me || !controller_live {
        inner.active_client_id = Some(cid.to_string());
        inner.active_seen_ms = now_ms;
        true
    } else {
        false
    }
}

/// 控制端当前是否在线。
pub fn controller_online(inner: &Inner, now_ms: u64) -> bool {
    inner.active_client_id.is_some() && client_online(inner.active_seen_ms, now_ms)
}

/// 控制端最近一次心跳距今毫秒;从未有控制端则 None。导播 GUI 用它显示"弱 · Ns 前"。
pub fn controller_seen_ms_ago(inner: &Inner, now_ms: u64) -> Option<u64> {
    inner.active_client_id.as_ref().map(|_| now_ms.saturating_sub(inner.active_seen_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_not_force_locked() {
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let st = AppState::new("t".into(), 0, tx);
        let i = st.lock();
        assert!(!i.remote_enabled, "远程默认关");
        assert!(!i.control_locked, "强制锁默认释放(新默认)");
        assert!(!i.presenter_unlocked, "默认未解锁");
    }

    #[test]
    fn gating_state_machine() {
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let st = AppState::new("t".into(), 0, tx);
        // 远程关 → 全禁
        assert!(!st.button_allowed());
        assert!(!st.click_allowed());
        // 开远程、未强锁 → 导播层放行(button_allowed 真),但主持人未 ARM → 动作仍禁
        st.lock().remote_enabled = true;
        assert!(st.button_allowed());
        assert!(!st.click_allowed());
        // 主持人 ARM → 一切动作放行
        st.lock().presenter_unlocked = true;
        assert!(st.click_allowed());
        // 紧急强锁压制一切
        st.lock().control_locked = true;
        assert!(!st.button_allowed());
        assert!(!st.click_allowed());
    }

    #[test]
    fn client_online_threshold() {
        assert!(client_online(1000, 1000));
        assert!(client_online(1000, 1000 + PRESENCE_TIMEOUT_MS));
        assert!(!client_online(1000, 1000 + PRESENCE_TIMEOUT_MS + 1));
    }

    #[test]
    fn claim_first_come_takeover_viewonly() {
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let st = AppState::new("t".into(), 0, tx);
        let mut i = st.lock();
        // 空槽 → A 认领
        assert!(note_presence_and_claim(&mut i, "A", 1000));
        assert_eq!(i.active_client_id.as_deref(), Some("A"));
        // A 仍在线时 B 轮询 → 只读
        assert!(!note_presence_and_claim(&mut i, "B", 1500));
        assert_eq!(i.active_client_id.as_deref(), Some("A"));
        // A 在线时 A 自己轮询 → 保持 + 刷新心跳
        assert!(note_presence_and_claim(&mut i, "A", 2000));
        // A 超时离线后 B 轮询 → 接管
        assert!(note_presence_and_claim(&mut i, "B", 2000 + PRESENCE_TIMEOUT_MS + 1));
        assert_eq!(i.active_client_id.as_deref(), Some("B"));
    }

    #[test]
    fn presence_timeout_is_ten_seconds() {
        assert_eq!(PRESENCE_TIMEOUT_MS, 10_000);
    }

    #[test]
    fn controller_seen_ms_ago_tracks_last_heartbeat() {
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let st = AppState::new("t".into(), 0, tx);
        let mut i = st.lock();
        assert_eq!(controller_seen_ms_ago(&i, 5000), None);
        note_presence_and_claim(&mut i, "A", 1000);
        assert_eq!(controller_seen_ms_ago(&i, 4500), Some(3500));
        // 时钟回拨不出现负数
        assert_eq!(controller_seen_ms_ago(&i, 500), Some(0));
    }

    #[test]
    fn timer_remaining_running_paused_overrun() {
        // 运行中:5s 时长,已过 2s(started=1000,now=3000)→ 剩 3s
        assert_eq!(timer_remaining_ms(5000, true, 1000, 0, 3000), 3000);
        // 暂停:累计 2s,不随 now 变 → 剩 3s
        assert_eq!(timer_remaining_ms(5000, false, 0, 2000, 999_999), 3000);
        // 超时:运行中已过 7s → 剩 -2s
        assert_eq!(timer_remaining_ms(5000, true, 0, 0, 7000), -2000);
        // 未启动(running=false,elapsed=0)→ 满
        assert_eq!(timer_remaining_ms(5000, false, 0, 0, 123), 5000);
    }

    #[test]
    fn lock_recovers_from_poison() {
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let app = std::sync::Arc::new(AppState::new("tok".into(), 8787, tx));
        let a2 = app.clone();
        // 在持有 guard 时 panic → 毒化 mutex；join 吞掉子线程的 panic。
        let _ = std::thread::spawn(move || {
            let _g = a2.lock();
            panic!("poison it");
        })
        .join();
        // 中毒后仍必须可读可写，且不 panic。
        let mut g = app.lock();
        g.remote_enabled = true;
        assert!(g.remote_enabled);
    }
}
