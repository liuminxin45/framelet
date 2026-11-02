//! Per-analysis-process LRU. Every lookup hashes leased input bytes; mtime is never trusted.
use anyhow::{Result, ensure};
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
const BUDGET: usize = 64 * 1024 * 1024;
struct Entry {
    key: PathBuf,
    kind: TypeId,
    tag: String,
    stamp: Vec<Option<String>>,
    bytes: usize,
    touched: Instant,
    value: Arc<dyn Any + Send + Sync>,
}
#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
    hits: u64,
    misses: u64,
}
thread_local! {static CACHE:RefCell<Cache>=RefCell::new(Cache::default());}
pub fn get<T: Any + Send + Sync>(
    path: &Path,
    files: &[PathBuf],
    load: impl FnOnce() -> Result<(T, usize)>,
) -> Result<Arc<T>> {
    get_tag(path, files, "", load)
}
pub fn get_tag<T: Any + Send + Sync>(
    path: &Path,
    files: &[PathBuf],
    tag: &str,
    load: impl FnOnce() -> Result<(T, usize)>,
) -> Result<Arc<T>> {
    let key = path.canonicalize()?;
    let mut leases = Vec::new();
    let mut stamp = Vec::new();
    for file in files {
        if file.try_exists()? {
            let limit = if file.extension().is_some_and(|s| s == "png") {
                32 * 1024 * 1024
            } else if file
                .file_name()
                .is_some_and(|s| s.to_string_lossy().ends_with(".timeline.json"))
            {
                64 * 1024 * 1024
            } else if file
                .file_name()
                .is_some_and(|s| s.to_string_lossy().ends_with(".capture.json"))
            {
                8192
            } else {
                512 * 1024 * 1024
            };
            ensure!(
                file.metadata()?.len() <= limit,
                "review input exceeds byte budget"
            );
            let lease = crate::file_guard::Lease::input(file)?;
            stamp.push(Some(crate::identity::sha256_reader(lease.reader()?)?));
            leases.push(lease);
        } else {
            stamp.push(None);
        }
    }
    let found = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(index) = cache.entries.iter().position(|e| {
            e.key == key
                && e.kind == TypeId::of::<T>()
                && e.tag == tag
                && e.stamp == stamp
                && e.touched.elapsed() < Duration::from_secs(30)
        }) {
            let mut entry = cache.entries.remove(index)?;
            entry.touched = Instant::now();
            let value = entry.value.clone().downcast::<T>().ok();
            cache.entries.push_back(entry);
            cache.hits += 1;
            value
        } else {
            cache.misses += 1;
            None
        }
    });
    if let Some(value) = found {
        return Ok(value);
    }
    let (value, bytes) = load()?;
    // Prove that optional sidecars did not appear/disappear while loading.
    for (file, expected) in files.iter().zip(&stamp) {
        ensure!(
            file.try_exists()? == expected.is_some(),
            "asset changed during validation"
        );
    }
    let value = Arc::new(value);
    if bytes <= BUDGET {
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let mut i = 0;
            while i < cache.entries.len() {
                if cache.entries[i].key == key
                    && cache.entries[i].kind == TypeId::of::<T>()
                    && cache.entries[i].tag == tag
                {
                    if let Some(e) = cache.entries.remove(i) {
                        cache.bytes = cache.bytes.saturating_sub(e.bytes);
                    }
                } else {
                    i += 1;
                }
            }
            while cache.entries.len() >= 8 || cache.bytes.saturating_add(bytes) > BUDGET {
                if let Some(e) = cache.entries.pop_front() {
                    cache.bytes = cache.bytes.saturating_sub(e.bytes);
                } else {
                    break;
                }
            }
            cache.bytes += bytes;
            cache.entries.push_back(Entry {
                key,
                kind: TypeId::of::<T>(),
                tag: tag.to_owned(),
                stamp,
                bytes,
                touched: Instant::now(),
                value: value.clone(),
            });
        });
    }
    drop(leases);
    Ok(value)
}
pub fn stats() -> serde_json::Value {
    CACHE.with(|c|{let c=c.borrow();serde_json::json!({"hits":c.hits,"misses":c.misses,"entries":c.entries.len(),"estimated_bytes":c.bytes,"budget_bytes":BUDGET,"validation":"SHA-256 of leased inputs on every lookup"})})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_does_not_trust_path_or_length() {
        let p = std::env::temp_dir().join(format!(
            "framelet-cache-{}",
            crate::identity::new_id().unwrap()
        ));
        std::fs::write(&p, b"first").unwrap();
        let load = || Ok((std::fs::read(&p)?, 5));
        assert_eq!(*get(&p, std::slice::from_ref(&p), load).unwrap(), b"first");
        assert_eq!(
            *get::<Vec<u8>>(&p, std::slice::from_ref(&p), || panic!("expected hit")).unwrap(),
            b"first"
        );
        std::fs::write(&p, b"other").unwrap();
        assert_eq!(*get(&p, std::slice::from_ref(&p), load).unwrap(), b"other");
        std::fs::remove_file(p).unwrap();
    }
}
