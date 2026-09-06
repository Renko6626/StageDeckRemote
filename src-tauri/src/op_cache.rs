//! 动作请求幂等缓存(spec 2026-09-07 §二)。
//!
//! 手机端为每个动作生成 `op_id`,坏网下短窗口内重发同一 id;服务端凭这张表保证
//! 同一 op 只执行一次:命中 `Done` 原样回放,命中 `InFlight` 回 409 让手机停止重发。
//! 纯数据结构,无 IO,Linux 可单测。

use serde_json::Value;

/// 缓存的 HTTP 结果(状态码 + JSON body)。
#[derive(Clone, Debug, PartialEq)]
pub struct CachedResp {
    pub status: u16,
    pub body: Value,
}

#[derive(Clone, Debug)]
enum Entry {
    InFlight,
    Done(CachedResp),
}

/// `reserve()` 的三种结局。
#[derive(Debug, PartialEq)]
pub enum Reserve {
    /// 同 op 已执行完,回放这个结果。
    Hit(CachedResp),
    /// 同 op 正在执行,别再执行。
    InFlight,
    /// 首次见到,调用方负责执行并 `complete()`。
    Reserved,
}

pub struct OpCache {
    /// 按 reserve 先后排序;淘汰最老的只需 remove(0)。
    entries: Vec<(String, u64, Entry)>,
}

impl OpCache {
    pub const CAP: usize = 64;
    pub const TTL_MS: u64 = 60_000;

    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// 清掉超过 TTL 的条目(含仍 InFlight 的:60s 还没完成早已超出手机重试预算)。
    pub fn evict(&mut self, now_ms: u64) {
        self.entries.retain(|(_, at, _)| now_ms.saturating_sub(*at) <= Self::TTL_MS);
    }

    pub fn reserve(&mut self, op_id: &str, now_ms: u64) -> Reserve {
        self.evict(now_ms);
        if let Some((_, _, e)) = self.entries.iter().find(|(id, _, _)| id == op_id) {
            return match e {
                Entry::InFlight => Reserve::InFlight,
                Entry::Done(r) => Reserve::Hit(r.clone()),
            };
        }
        if self.entries.len() >= Self::CAP {
            self.entries.remove(0);
        }
        self.entries.push((op_id.to_string(), now_ms, Entry::InFlight));
        Reserve::Reserved
    }

    /// 把 InFlight 标成 Done。已 Done 的不覆盖;未 reserve 过(已被淘汰)的忽略。
    pub fn complete(&mut self, op_id: &str, resp: CachedResp) {
        if let Some((_, _, e)) = self.entries.iter_mut().find(|(id, _, _)| id == op_id) {
            if matches!(e, Entry::InFlight) {
                *e = Entry::Done(resp);
            }
        }
    }
}

impl Default for OpCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok() -> CachedResp {
        CachedResp { status: 200, body: json!({"ok": true}) }
    }

    #[test]
    fn first_reserve_then_complete_then_hit() {
        let mut c = OpCache::new();
        assert_eq!(c.reserve("a", 0), Reserve::Reserved);
        assert_eq!(c.reserve("a", 10), Reserve::InFlight);
        c.complete("a", ok());
        assert_eq!(c.reserve("a", 20), Reserve::Hit(ok()));
    }

    #[test]
    fn done_result_is_not_overwritten() {
        let mut c = OpCache::new();
        c.reserve("a", 0);
        c.complete("a", ok());
        c.complete("a", CachedResp { status: 500, body: json!({"ok": false}) });
        assert_eq!(c.reserve("a", 1), Reserve::Hit(ok()));
    }

    #[test]
    fn entries_expire_after_ttl() {
        let mut c = OpCache::new();
        c.reserve("a", 0);
        c.complete("a", ok());
        assert_eq!(c.reserve("a", OpCache::TTL_MS), Reserve::Hit(ok()));
        assert_eq!(c.reserve("a", OpCache::TTL_MS + 1), Reserve::Reserved);
    }

    #[test]
    fn cap_evicts_oldest() {
        let mut c = OpCache::new();
        for i in 0..OpCache::CAP {
            assert_eq!(c.reserve(&format!("op{i}"), i as u64), Reserve::Reserved);
        }
        // 第 65 个把 op0 挤掉;op1 还在
        assert_eq!(c.reserve("new", 100), Reserve::Reserved);
        assert_eq!(c.reserve("op1", 101), Reserve::InFlight);
        assert_eq!(c.reserve("op0", 102), Reserve::Reserved);
    }

    #[test]
    fn complete_unknown_is_noop() {
        let mut c = OpCache::new();
        c.complete("ghost", ok());
        assert_eq!(c.reserve("ghost", 0), Reserve::Reserved);
    }
}
