//! Bounded, local JSONL diagnostics. A write failure is visible through IPC and
//! never prevents an operation; reopening/recovery is left to the next launch.
use serde::Serialize;
use std::{fs::{self, File, OpenOptions}, io::Write, path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Mutex}};

const FILE_LIMIT: u64 = 5 * 1024 * 1024;
const HISTORY: usize = 3;

#[derive(Serialize)]
pub struct Status {
    pub enabled: bool,
    pub directory: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
struct Storage {
    directory: Option<PathBuf>,
    file: Option<File>,
    size: u64,
    error: Option<String>,
}

pub struct Diagnostics {
    detailed: AtomicBool,
    storage: Mutex<Storage>,
    limit: u64,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self { detailed: AtomicBool::new(false), storage: Mutex::new(Storage::default()), limit: FILE_LIMIT }
    }
}

impl Diagnostics {
    pub fn initialize(&self, directory: Result<PathBuf, String>) {
        let mut storage = self.storage.lock().unwrap_or_else(|p| p.into_inner());
        let result = (|| -> Result<(), String> {
            let directory = directory?;
            storage.directory = Some(directory.clone());
            fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
            let file = OpenOptions::new().create(true).append(true).open(directory.join("diagnostics.jsonl"))
                .map_err(|e| e.to_string())?;
            storage.size = file.metadata().map_err(|e| e.to_string())?.len();
            storage.file = Some(file);
            Ok(())
        })();
        storage.error = result.err().map(|e| format!("诊断日志不可写：{e}"));
    }

    pub fn enabled(&self) -> bool { self.detailed.load(Ordering::Relaxed) }
    pub fn set_enabled(&self, enabled: bool) { self.detailed.store(enabled, Ordering::Relaxed); }

    pub fn status(&self) -> Status {
        let storage = self.storage.lock().unwrap_or_else(|p| p.into_inner());
        Status { enabled: self.enabled(), directory: storage.directory.as_ref().map(|p| p.to_string_lossy().into_owned()), error: storage.error.clone() }
    }

    pub fn write(&self, level: &str, component: &str, message: &str) {
        if level == "debug" && !self.enabled() { return; }
        let record = serde_json::json!({
            "time_ms": crate::app_state::now_ms(), "level": level,
            "component": component, "message": redact(message, &[]).chars().take(8000).collect::<String>(),
        });
        let Ok(mut line) = serde_json::to_vec(&record) else { return; };
        line.push(b'\n');
        let mut storage = self.storage.lock().unwrap_or_else(|p| p.into_inner());
        if storage.error.is_some() || storage.file.is_none() { return; }
        let result = (|| -> std::io::Result<()> {
            if storage.size + line.len() as u64 > self.limit {
                // Close before renaming: Windows cannot rename an open file.
                storage.file.take();
                let dir = storage.directory.as_ref().expect("open log has a directory");
                let oldest = dir.join(format!("diagnostics.{HISTORY}.jsonl"));
                if oldest.exists() { fs::remove_file(oldest)?; }
                for index in (1..HISTORY).rev() {
                    let from = dir.join(format!("diagnostics.{index}.jsonl"));
                    if from.exists() { fs::rename(from, dir.join(format!("diagnostics.{}.jsonl", index + 1)))?; }
                }
                fs::rename(dir.join("diagnostics.jsonl"), dir.join("diagnostics.1.jsonl"))?;
                storage.file = Some(OpenOptions::new().create(true).append(true).open(dir.join("diagnostics.jsonl"))?);
                storage.size = 0;
            }
            storage.file.as_mut().expect("log file open").write_all(&line)?;
            storage.size += line.len() as u64;
            Ok(())
        })();
        if let Err(e) = result {
            storage.file.take();
            storage.error = Some(format!("诊断日志写入失败：{e}。控制功能仍可使用，请检查磁盘后重启应用。"));
        }
    }
}

/// Remove configured credentials and credential query parameters, including
/// percent-encoded copies that can appear in browser/transport error strings.
pub fn redact(message: &str, secrets: &[String]) -> String {
    let mut result = message.to_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        let encoded: String = secret.bytes().map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() }
            else { format!("%{b:02X}") }
        }).collect();
        result = result.replace(secret, "[redacted]").replace(&encoded, "[redacted]");
    }
    for key in ["token=", "secret=", "access_token=", "authorization="] {
        let mut from = 0;
        while let Some(relative) = result[from..].to_ascii_lowercase().find(key) {
            let start = from + relative + key.len();
            let length = result[start..].find(|c: char| c.is_whitespace() || "&#\"')},;".contains(c)).unwrap_or(result.len() - start);
            result.replace_range(start..start + length, "[redacted]");
            from = start + "[redacted]".len();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!("stagedeck-diagnostics-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
    }

    #[test]
    fn records_persist_with_bounded_history_and_debug_opt_in() {
        let dir = directory();
        let logs = Diagnostics { limit: 250, ..Diagnostics::default() };
        logs.initialize(Ok(dir.clone()));
        logs.write("debug", "test", "hidden");
        assert_eq!(fs::metadata(dir.join("diagnostics.jsonl")).unwrap().len(), 0);
        logs.set_enabled(true);
        for i in 0..10 { logs.write("debug", "test", &format!("visible {i}")); }
        let files: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(files.len(), HISTORY + 1);
        let current = fs::read_to_string(dir.join("diagnostics.jsonl")).unwrap();
        assert!(current.contains("visible 9"));
        for file in files {
            assert!(fs::metadata(&file).unwrap().len() <= 250);
            for line in fs::read_to_string(file).unwrap().lines() {
                assert!(serde_json::from_str::<serde_json::Value>(line).is_ok());
            }
        }
        drop(logs);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn credentials_are_removed_from_plain_and_encoded_messages() {
        let out = redact("key=a+b key=a%2Bb https://host/?token=unknown&secret=other done", &["a+b".into()]);
        assert!(!out.contains("a+b") && !out.contains("a%2Bb") && !out.contains("unknown") && !out.contains("other"));
        assert!(out.ends_with(" done"));
    }

    #[test]
    fn initialization_failure_is_visible_without_panicking_on_write() {
        let logs = Diagnostics::default();
        logs.initialize(Err("no directory".into()));
        logs.write("error", "test", "still running");
        assert!(logs.status().error.unwrap().contains("no directory"));
    }
}
