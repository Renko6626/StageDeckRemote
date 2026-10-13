//! 只读 admin 状态页 —— `GET /admin/status?token=<admin_token>`。
//!
//! - 只在 `config.admin_token` 存在时启用;否则路由返回 404(不暴露存在性)。
//! - 不显示 token / 完整 secret;secret 只露前 3 字符 + ***。
//! - 5 秒 `<meta refresh>` 自动刷新;无 JS。
//! - 美术风格沿用导播 / presenter 的暗底等宽。

use crate::AppCtx;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

pub fn router() -> Router<Arc<AppCtx>> {
    Router::new().route("/admin/status", get(status_page))
}

async fn status_page(
    State(ctx): State<Arc<AppCtx>>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    // 配置里没设 admin_token => 状态页不可达(返 404,不暴露存在性)。
    let expected = match ctx.config.admin_token.as_deref() {
        Some(t) if !t.is_empty() => t,
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let supplied = q.get("token").map(|s| s.as_str()).unwrap_or("");
    if supplied != expected {
        return (StatusCode::UNAUTHORIZED, "invalid admin token").into_response();
    }

    let now = Instant::now();
    let uptime_secs = now.duration_since(ctx.started_at).as_secs();

    // 先取 Arc<Session> 的快照,再在表读锁外做每会话的 async lock(避免长时间持表锁)。
    let snapshot: Vec<(String, Arc<crate::session::Session>)> = {
        let tbl = ctx.sessions.0.read().await;
        tbl.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };

    let mut rows: Vec<Row> = Vec::with_capacity(snapshot.len());
    for (sid, sess) in &snapshot {
        let control_connected = !sess.control_tx.lock().await.is_closed();
        let stream_connected = sess.stream_connected.load(Ordering::SeqCst);
        let frames_pushed = sess.frames_pushed.load(Ordering::Relaxed);
        let pending = sess.pending.lock().await.len();
        let last_seen_ago = now
            .checked_duration_since(*sess.last_seen.lock().await)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let registered_ago = now
            .checked_duration_since(sess.registered_at)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let frame_last_ago = sess
            .frame_last_at
            .lock()
            .await
            .and_then(|t| now.checked_duration_since(t).map(|d| d.as_secs()));
        // secret 露前 3 字符 + ***(便于现场快速对照配置,不直接暴露)。
        let secret_hint = {
            let n = sess.secret.chars().count();
            let take = n.min(3);
            let prefix: String = sess.secret.chars().take(take).collect();
            format!("{prefix}***")
        };

        rows.push(Row {
            sid: sid.clone(),
            control_connected,
            stream_connected,
            frames_pushed,
            pending,
            last_seen_ago,
            registered_ago,
            frame_last_ago,
            secret_hint,
        });
    }

    rows.sort_by(|a, b| a.sid.cmp(&b.sid));
    let html = render_html(uptime_secs, &rows);
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer")],
        html,
    )
        .into_response()
}

struct Row {
    sid: String,
    control_connected: bool,
    stream_connected: bool,
    frames_pushed: u64,
    pending: usize,
    last_seen_ago: u64,
    registered_ago: u64,
    frame_last_ago: Option<u64>,
    secret_hint: String,
}

fn render_html(uptime_secs: u64, rows: &[Row]) -> String {
    let mut out = String::with_capacity(2048 + rows.len() * 256);
    out.push_str(HEAD);
    out.push_str("<body><main>\n");
    out.push_str(&format!(
        "<h1>STAGEDECK<span class=\"sub\"> Relay 状态</span></h1>\n\
         <p class=\"meta\">已运行 <b>{}</b>　会话 <b>{}</b>　每 5 秒刷新</p>\n",
        html_escape(&fmt_dur(uptime_secs)),
        rows.len(),
    ));

    if rows.is_empty() {
        out.push_str("<p class=\"empty\">暂无会话，请在导播端配置并连接 Relay</p>\n");
    } else {
        out.push_str(
            "<table>\n<thead><tr>\
             <th>会话</th>\
             <th>控制</th>\
             <th>推流</th>\
             <th>接收帧数</th>\
             <th>等待请求</th>\
             <th>最近心跳</th>\
             <th>最近画面</th>\
             <th>注册时长</th>\
             <th>密钥提示</th>\
             </tr></thead>\n<tbody>\n",
        );
        for r in rows {
            let frame_last = match r.frame_last_ago {
                Some(s) => fmt_ago(s),
                None => "—".to_string(),
            };
            out.push_str(&format!(
                "<tr>\
                 <td class=\"sid\" data-label=\"会话\">{}</td>\
                 <td data-label=\"控制\">{}</td>\
                 <td data-label=\"推流\">{}</td>\
                 <td data-label=\"接收帧数\">{}</td>\
                 <td data-label=\"等待请求\">{}</td>\
                 <td data-label=\"最近心跳\">{}</td>\
                 <td data-label=\"最近画面\">{}</td>\
                 <td data-label=\"注册时长\">{}</td>\
                 <td class=\"sec\" data-label=\"密钥提示\">{}</td>\
                 </tr>\n",
                html_escape(&r.sid),
                indicator(r.control_connected),
                indicator(r.stream_connected),
                r.frames_pushed,
                r.pending,
                html_escape(&fmt_ago(r.last_seen_ago)),
                html_escape(&frame_last),
                html_escape(&fmt_dur(r.registered_ago)),
                html_escape(&r.secret_hint),
            ));
        }
        out.push_str("</tbody></table>\n");
    }

    out.push_str("<p class=\"help\">控制和推流分别显示连接状态。PPT 画面静止时不发送新帧，最近画面时间较久不代表断线。已断开的会话会在宽限期后移除。</p></main></body></html>");
    out
}

const HEAD: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>StageDeck Relay 状态</title>
<meta http-equiv="refresh" content="5">
<style>
:root {
  --bg:#0a0a0b; --panel:#0e0e10; --line:#26262b; --line-hi:#3a3a42;
  --ink:#ededee; --ink-dim:#a2a2aa; --ink-faint:#82828b;
  --danger:#d23b3b; --ok:#40c870;
  --mono: ui-monospace, "Cascadia Mono", "JetBrains Mono", "SF Mono", monospace;
}
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; background: var(--bg); color: var(--ink); font-family: var(--mono); font-size: 13px; }
main { max-width: 1040px; margin: 0 auto; padding: 28px 22px; }
h1 { font-size: 14px; letter-spacing: 0.25em; text-transform: uppercase; color: var(--ink); margin: 0 0 4px; font-weight: 700; }
h1 .sub { color: var(--ink-faint); font-weight: 400; }
.meta { color: var(--ink-dim); font-size: 11px; letter-spacing: 0.14em; text-transform: uppercase; margin: 0 0 22px; }
.meta b { color: var(--ink); }
.empty { color: var(--ink-faint); padding: 60px 0; text-align: center; letter-spacing: 0.18em; text-transform: uppercase; font-size: 11px; border: 1px dashed var(--line); }
table { width: 100%; border-collapse: collapse; }
thead th { font-size: 10px; letter-spacing: 0.18em; color: var(--ink-faint); text-transform: uppercase; font-weight: 400; padding: 10px 8px; text-align: left; border-bottom: 1px solid var(--line-hi); }
tbody td { padding: 9px 8px; border-bottom: 1px solid var(--line); font-size: 12px; }
td.sid { color: var(--ink); font-weight: 700; letter-spacing: 0.06em; }
td.sec { color: var(--ink-faint); }
.state { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
.help { color: var(--ink-dim); font-size: 12px; line-height: 1.7; margin-top: 22px; }
@media (max-width: 760px) {
  main { padding: 22px 16px; }
  h1 { letter-spacing: .08em; }
  .meta { letter-spacing: 0; line-height: 1.8; }
  table, tbody { display: block; }
  thead { display: none; }
  tbody tr { display: grid; grid-template-columns: 1fr 1fr; gap: 0 16px; margin-bottom: 16px; padding: 12px; border: 1px solid var(--line-hi); background: var(--panel); }
  tbody td { display: flex; flex-direction: column; gap: 6px; min-width: 0; overflow-wrap: anywhere; padding: 10px 0; }
  tbody td::before { content: attr(data-label); color: var(--ink-faint); font-size: 11px; font-weight: 400; }
  td.sid { grid-column: 1 / -1; font-size: 15px; }
}
.dot { display: inline-block; width: 9px; height: 9px; vertical-align: middle; }
.dot.on  { background: var(--ok); }
.dot.off { background: var(--danger); }
</style>
</head>
"#;

fn indicator(on: bool) -> &'static str {
    if on {
        r#"<span class="state"><span class="dot on" aria-hidden="true"></span>已连接</span>"#
    } else {
        r#"<span class="state"><span class="dot off" aria-hidden="true"></span>未连接</span>"#
    }
}

fn fmt_ago(s: u64) -> String {
    if s < 60 {
        format!("{}s", s)
    } else if s < 3600 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    }
}

fn fmt_dur(s: u64) -> String {
    if s < 60 {
        format!("{}s", s)
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86400 {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}d{:02}h", s / 86400, (s % 86400) / 3600)
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_handles_specials() {
        assert_eq!(html_escape("a<b>c"), "a&lt;b&gt;c");
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape(r#"a"b"#), "a&quot;b");
    }

    #[test]
    fn fmt_dur_picks_unit() {
        assert_eq!(fmt_dur(5), "5s");
        assert_eq!(fmt_dur(120), "2m");
        assert_eq!(fmt_dur(3725), "1h02m");
        assert_eq!(fmt_dur(90061), "1d01h");
    }

    #[test]
    fn fmt_ago_shows_seconds() {
        assert_eq!(fmt_ago(0), "0s");
        assert_eq!(fmt_ago(59), "59s");
        assert_eq!(fmt_ago(125), "2m05s");
    }

    #[test]
    fn render_empty_table_shows_placeholder() {
        let html = render_html(42, &[]);
        assert!(html.contains("暂无会话，请在导播端配置并连接 Relay"));
        assert!(html.contains("已运行 <b>42s</b>"));
    }

    #[test]
    fn render_row_includes_sid_and_dots() {
        let rows = vec![Row {
            sid: "PROM-2026".into(),
            control_connected: true,
            stream_connected: false,
            frames_pushed: 142,
            pending: 0,
            last_seen_ago: 3,
            registered_ago: 893,
            frame_last_ago: Some(7),
            secret_hint: "abc***".into(),
        }];
        let html = render_html(123, &rows);
        assert!(html.contains("PROM-2026"));
        assert!(html.contains("dot on"));   // control connected → green
        assert!(html.contains("dot off"));  // stream disconnected → red
        assert!(html.contains(">142<"));    // frames count
        assert!(html.contains("abc***"));
        assert!(html.contains("3s"));        // last seen
    }
}
