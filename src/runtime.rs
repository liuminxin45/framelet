use anyhow::{Result, anyhow};
use serde::Serialize;
use std::{
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::Mutex,
};
#[derive(Serialize)]
pub struct Failure {
    pub code: &'static str,
    pub stage: String,
    pub message: String,
    pub unix_ms: u128,
    pub retryable: bool,
    pub log_error: Option<String>,
}
static LOG: Mutex<()> = Mutex::new(());
pub fn log_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Framelet/Logs")
}
pub fn failure(stage: &str, error: &anyhow::Error) -> Failure {
    Failure {
        code: if error.downcast_ref::<RecoveredPanic>().is_some() {
            "internal_panic"
        } else {
            "operation_failed"
        },
        stage: stage.chars().take(128).collect(),
        message: format!("{error:#}").chars().take(4096).collect(),
        unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        retryable: false,
        log_error: None,
    }
}
pub fn report(stage: &str, error: &anyhow::Error) -> Failure {
    let mut f = failure(stage, error);
    if let Err(e) = write_log(&f) {
        f.log_error = Some(format!("diagnostic persistence failed: {e}"));
    }
    f
}
fn write_log(f: &Failure) -> Result<()> {
    let _guard = LOG
        .try_lock()
        .map_err(|_| anyhow!("diagnostic log busy or poisoned"))?;
    let dir = log_directory();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("failures-{}.jsonl", std::process::id()));
    if path.metadata().is_ok_and(|m| m.len() >= 1024 * 1024) {
        let backup = dir.join(format!("failures-{}.previous.jsonl", std::process::id()));
        match std::fs::remove_file(&backup) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        std::fs::rename(&path, &backup)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;
    serde_json::to_writer(&mut file, f)?;
    file.write_all(b"\n")?;
    file.flush()?;
    // Per-process files avoid cross-process rotation races; retain at most 64 owned logs.
    let mut owned: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix("failures-")
                .and_then(|s| s.strip_suffix(".jsonl"))
                .is_some_and(|s| {
                    s.strip_suffix(".previous")
                        .unwrap_or(s)
                        .bytes()
                        .all(|b| b.is_ascii_digit())
                })
        })
        .filter_map(|entry| {
            entry
                .metadata()
                .ok()
                .filter(|m| m.is_file())
                .map(|m| (m.modified().ok(), entry.path()))
        })
        .collect();
    owned.sort_by_key(|entry| entry.0);
    let excess = owned.len().saturating_sub(64);
    for (_, old) in owned.into_iter().take(excess) {
        if old != path {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}
#[derive(Debug)]
struct RecoveredPanic;
impl std::fmt::Display for RecoveredPanic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unexpected Rust panic; operation stopped")
    }
}
impl std::error::Error for RecoveredPanic {}
/// Catch at task boundaries, never resume the failed mutable operation itself.
pub fn guard<T>(stage: &str, work: impl FnOnce() -> Result<T>) -> Result<T> {
    match catch_unwind(AssertUnwindSafe(work)) {
        Ok(value) => value,
        Err(payload) => {
            // A hostile panic payload can itself panic on Drop.
            std::mem::forget(payload);
            let error = anyhow::Error::new(RecoveredPanic);
            report(stage, &error);
            Err(error)
        }
    }
}
pub fn install() {
    std::panic::set_hook(Box::new(|_| {}));
}
pub fn progress(value: serde_json::Value) {
    let _ = writeln!(std::io::stderr().lock(), "{value}");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_is_contained_and_following_task_runs() {
        let r: Result<()> = guard("test.panic", || panic!("controlled fault"));
        assert!(r.is_err());
        assert_eq!(guard("test.next", || Ok(42)).ok(), Some(42));
    }
}
