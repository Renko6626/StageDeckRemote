//! 全局运行状态（spec §11.2）。
//!
//! `AppState` 同时被两条通道共享：
//!   * 导播端 GUI —— 通过 Tauri IPC（commands.rs）访问，拥有最高特权；
//!   * 主持人端浏览器 —— 通过 Axum HTTP/WS（routes.rs）访问，受限。
//! 因此它被包成 `Arc<AppState>`，内部用 `Mutex` 保护可变字段。

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::log::LogEntry;

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
    pub token: String,
    pub server_port: u16,
    pub logs: Vec<LogEntry>,
}

pub struct AppState {
    pub inner: Mutex<Inner>,
    pub capture: crate::capture::CaptureManager,
}

impl AppState {
    pub fn new(token: String, server_port: u16) -> Self {
        AppState {
            inner: Mutex::new(Inner {
                selected_window: None,
                // 默认:远程关闭(日常总闸)、强制锁释放(仅紧急用)、未解锁。
                // 导播开「远程控制」一步即放行,主持人再 ARM 即可全功能。
                remote_enabled: false,
                control_locked: false,
                presenter_unlocked: false,
                active_client_id: None,
                token,
                server_port,
                logs: Vec::new(),
            }),
            capture: crate::capture::CaptureManager::new(),
        }
    }

    /// 三层锁全部放行才允许"点击预览图"这类隐式操作（spec §13.2）。
    pub fn click_allowed(&self) -> bool {
        self.inner
            .lock()
            .map(|i| i.remote_enabled && !i.control_locked && i.presenter_unlocked)
            .unwrap_or(false)
    }

    /// 显式按钮（翻页等）只要求远程开启且未被强制锁。
    pub fn button_allowed(&self) -> bool {
        self.inner
            .lock()
            .map(|i| i.remote_enabled && !i.control_locked)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_not_force_locked() {
        let st = AppState::new("t".into(), 0);
        let i = st.inner.lock().unwrap();
        assert!(!i.remote_enabled, "远程默认关");
        assert!(!i.control_locked, "强制锁默认释放(新默认)");
        assert!(!i.presenter_unlocked, "默认未解锁");
    }

    #[test]
    fn gating_state_machine() {
        let st = AppState::new("t".into(), 0);
        // 远程关 → 全禁
        assert!(!st.button_allowed());
        assert!(!st.click_allowed());
        // 开远程、未强锁 → 导播层放行(button_allowed 真),但主持人未 ARM → 动作仍禁
        st.inner.lock().unwrap().remote_enabled = true;
        assert!(st.button_allowed());
        assert!(!st.click_allowed());
        // 主持人 ARM → 一切动作放行
        st.inner.lock().unwrap().presenter_unlocked = true;
        assert!(st.click_allowed());
        // 紧急强锁压制一切
        st.inner.lock().unwrap().control_locked = true;
        assert!(!st.button_allowed());
        assert!(!st.click_allowed());
    }
}
