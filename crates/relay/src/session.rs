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

impl Session {
    /// 丢弃所有等待中的隧道请求:oneshot sender 一 drop,`tunnel()` 那头立刻收到 RecvError → 502。
    /// 控制 WS 断开或被新连接接管时调用。返回丢弃条数(仅日志用)。
    pub async fn fail_pending(&self) -> usize {
        let mut pend = self.pending.lock().await;
        let n = pend.len();
        pend.clear();
        n
    }
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
                tracing::warn!(sid = %sid, "session register rejected: secret_mismatch");
                return Err("secret_mismatch");
            }
            // Same secret reconnect: replace control_tx + update token + refresh last_seen.
            *existing.control_tx.lock().await = control_tx;
            *existing.token.lock().await = token;
            *existing.last_seen.lock().await = Instant::now();
            // 旧连接上发出去的请求永远不会有回应了,立刻让手机拿到 502 而不是等满隧道超时。
            let dropped = existing.fail_pending().await;
            tracing::info!(sid = %sid, dropped_pending = dropped, "session reattached");
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
        tracing::info!(sid = %sid, "session registered");
        tbl.insert(sid, sess.clone());
        Ok(sess)
    }

    pub async fn drop_session(&self, sid: &str) {
        if self.0.write().await.remove(sid).is_some() {
            tracing::info!(sid = %sid, "session dropped");
        }
    }

    /// 把"控制 WS 已挂掉 + last_seen 超过 grace"的 sid 摘出来给 reaper。
    /// 不真删,调用方拿这个列表挨个 drop_session(),避免与活跃 WS 拿同把锁打架。
    pub async fn collect_expired(
        &self,
        grace: std::time::Duration,
        now: Instant,
    ) -> Vec<SessionId> {
        let tbl = self.0.read().await;
        let mut out = Vec::new();
        for (sid, sess) in tbl.iter() {
            // 检查控制 WS 是否还连着;writer 任务掉 rx → tx.is_closed() = true
            if !sess.control_tx.lock().await.is_closed() {
                continue;
            }
            let last_seen = *sess.last_seen.lock().await;
            if now.saturating_duration_since(last_seen) > grace {
                out.push(sid.clone());
            }
        }
        out
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

    #[tokio::test]
    async fn fail_pending_wakes_waiters_with_error() {
        let tbl = SessionTable::new();
        let sess = tbl
            .register_or_attach("PROM-2026".into(), "sec".into(), "t".into(), dummy_tx())
            .await
            .unwrap();
        let (tx, rx) = oneshot::channel();
        sess.pending.lock().await.insert(7, tx);
        assert_eq!(sess.fail_pending().await, 1);
        assert!(rx.await.is_err(), "等待方必须立刻拿到 RecvError");
        assert!(sess.pending.lock().await.is_empty());
    }

    #[tokio::test]
    async fn reattach_drops_stale_pending() {
        let tbl = SessionTable::new();
        let sess = tbl
            .register_or_attach("PROM-2026".into(), "sec".into(), "t".into(), dummy_tx())
            .await
            .unwrap();
        let (tx, rx) = oneshot::channel();
        sess.pending.lock().await.insert(1, tx);
        tbl.register_or_attach("PROM-2026".into(), "sec".into(), "t2".into(), dummy_tx())
            .await
            .unwrap();
        assert!(rx.await.is_err());
    }

    #[tokio::test]
    async fn collect_expired_picks_stale_disconnected_sessions() {
        let tbl = SessionTable::new();

        // 1) dummy_tx: rx 立刻被 drop → tx.is_closed() = true(=控制 WS 已掉线)
        let sess_stale = tbl
            .register_or_attach("STALE".into(), "s".into(), "t".into(), dummy_tx())
            .await
            .unwrap();
        // 把 last_seen 倒拨 100 秒,模拟"掉线很久了"
        *sess_stale.last_seen.lock().await =
            Instant::now().checked_sub(std::time::Duration::from_secs(100)).unwrap();

        // 2) 同样掉线但 last_seen 是刚刚 → 还在宽限期内,不该被摘
        tbl.register_or_attach("FRESH-DEAD".into(), "s".into(), "t".into(), dummy_tx())
            .await
            .unwrap();

        // 3) 控制 WS 还活着 → 不该被摘,即使 last_seen 已老
        let (live_tx, _live_rx) = mpsc::unbounded_channel::<ControlMessage>();
        let sess_live = tbl
            .register_or_attach("LIVE".into(), "s".into(), "t".into(), live_tx)
            .await
            .unwrap();
        *sess_live.last_seen.lock().await =
            Instant::now().checked_sub(std::time::Duration::from_secs(999)).unwrap();
        // _live_rx 一直被持有 → tx.is_closed() = false

        let expired =
            tbl.collect_expired(std::time::Duration::from_secs(60), Instant::now()).await;
        assert_eq!(expired, vec!["STALE".to_string()]);
    }
}
