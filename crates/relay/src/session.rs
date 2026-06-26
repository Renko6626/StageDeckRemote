use protocol::ControlMessage;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot, watch, Mutex, RwLock};

pub type SessionId = String;

pub struct Session {
    pub secret: String,
    pub token: Mutex<String>,
    /// Mutex-wrapped so reconnecting directors can replace the sender atomically.
    pub control_tx: Mutex<mpsc::UnboundedSender<ControlMessage>>,
    pub frame_tx: watch::Sender<Arc<Vec<u8>>>,
    pub pending: Mutex<HashMap<u64, oneshot::Sender<ControlMessage>>>,
    pub next_req_id: AtomicU64,
    pub last_seen: Mutex<Instant>,
    // ── 只读统计:仅 /admin/status 用,业务逻辑不读这些字段 ──
    /// 该 session 首次 register 的时刻;不随 reconnect 重置。
    pub registered_at: Instant,
    /// stream WS 推过来的二进制帧总数(累计;不随 reconnect 重置)。
    pub frames_pushed: AtomicU64,
    /// 最近一次接收到 stream 帧的时刻。
    pub frame_last_at: Mutex<Option<Instant>>,
    /// stream WS 当前是否连着(stream_loop 进出时 set true/false)。
    pub stream_connected: AtomicBool,
}

pub struct SessionTable(pub RwLock<HashMap<SessionId, Arc<Session>>>);

impl SessionTable {
    pub fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }

    pub async fn get(&self, sid: &str) -> Option<Arc<Session>> {
        self.0.read().await.get(sid).cloned()
    }

    /// First appearance: register new session.
    /// Already exists + same secret: take over (reconnect), replacing control_tx and updating token.
    /// Already exists + wrong secret: return Err("secret_mismatch").
    pub async fn register_or_attach(
        &self,
        sid: SessionId,
        secret: String,
        token: String,
        control_tx: mpsc::UnboundedSender<ControlMessage>,
    ) -> Result<Arc<Session>, &'static str> {
        let mut tbl = self.0.write().await;
        if let Some(existing) = tbl.get(&sid).cloned() {
            if existing.secret != secret {
                return Err("secret_mismatch");
            }
            // Same secret reconnect: replace control_tx + update token + refresh last_seen.
            *existing.control_tx.lock().await = control_tx;
            *existing.token.lock().await = token;
            *existing.last_seen.lock().await = Instant::now();
            return Ok(existing);
        }
        let (frame_tx, _) = watch::channel(Arc::new(Vec::new()));
        let now = Instant::now();
        let sess = Arc::new(Session {
            secret,
            token: Mutex::new(token),
            control_tx: Mutex::new(control_tx),
            frame_tx,
            pending: Mutex::new(HashMap::new()),
            next_req_id: AtomicU64::new(1),
            last_seen: Mutex::new(now),
            registered_at: now,
            frames_pushed: AtomicU64::new(0),
            frame_last_at: Mutex::new(None),
            stream_connected: AtomicBool::new(false),
        });
        tbl.insert(sid, sess.clone());
        Ok(sess)
    }

    pub async fn drop_session(&self, sid: &str) {
        self.0.write().await.remove(sid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_tx() -> mpsc::UnboundedSender<ControlMessage> {
        let (tx, _rx) = mpsc::unbounded_channel();
        tx
    }

    #[tokio::test]
    async fn register_first_then_reattach_same_secret() {
        let tbl = SessionTable::new();
        let s1 = tbl
            .register_or_attach("PROM-2026".into(), "sec".into(), "tok1".into(), dummy_tx())
            .await
            .unwrap();
        let s2 = tbl
            .register_or_attach("PROM-2026".into(), "sec".into(), "tok2".into(), dummy_tx())
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&s1, &s2));
        assert_eq!(&*s1.token.lock().await, "tok2");
    }

    #[tokio::test]
    async fn reattach_wrong_secret_rejected() {
        let tbl = SessionTable::new();
        tbl.register_or_attach("PROM-2026".into(), "sec".into(), "t".into(), dummy_tx())
            .await
            .unwrap();
        let err =
            tbl.register_or_attach("PROM-2026".into(), "WRONG".into(), "t".into(), dummy_tx())
                .await;
        assert_eq!(err.err(), Some("secret_mismatch"));
    }
}
