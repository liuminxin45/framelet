use super::{SeriesArgs, model::*};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

// Serializes group creation/recovery across CLI and MCP processes without a resident service.
pub struct OperationLock(windows_sys::Win32::Foundation::HANDLE);
impl OperationLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        let mut hash = 14695981039346656037u64;
        for b in path.to_string_lossy().to_lowercase().bytes() {
            hash = (hash ^ b as u64).wrapping_mul(1099511628211);
        }
        let name: Vec<u16> = format!("Local\\Framelet-Series-{hash:016x}\0")
            .encode_utf16()
            .collect();
        unsafe {
            let h = windows_sys::Win32::System::Threading::CreateMutexW(
                std::ptr::null(),
                0,
                name.as_ptr(),
            );
            ensure!(!h.is_null(), "cannot create recording operation lock");
            let state = windows_sys::Win32::System::Threading::WaitForSingleObject(h, 5000);
            if state != 0 && state != 128 {
                windows_sys::Win32::Foundation::CloseHandle(h);
                anyhow::bail!("recording operation is busy; retry status, not user interactions")
            }
            Ok(Self(h))
        }
    }
}
impl Drop for OperationLock {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.0);
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

pub fn directory(input: &Path) -> Result<PathBuf> {
    let root = if input.is_dir() {
        input.to_path_buf()
    } else {
        ensure!(
            input.file_name().is_some_and(|n| n == "recording.json"),
            "expected .framelet directory or recording.json"
        );
        input
            .parent()
            .context("missing recording parent")?
            .to_path_buf()
    };
    ensure!(
        root.extension().is_some_and(|x| x == "framelet"),
        "expected .framelet group directory"
    );
    Ok(root.canonicalize()?)
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let _lease = crate::file_guard::Lease::input(path)?;
    let f = File::open(path)?;
    ensure!(f.metadata()?.len() <= limit, "JSON exceeds resource budget");
    Ok(serde_json::from_reader(f.take(limit + 1))?)
}
pub fn filename(prefix: &str, number: u32) -> String {
    format!("{prefix}.part-{number:06}.gif")
}
pub(super) fn hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn validate_index(i: &Index) -> Result<()> {
    ensure!(
        [
            "starting",
            "recording",
            "completed",
            "partial",
            "failed",
            "cancelled"
        ]
        .contains(&i.state.as_str())
            && i.reason.len() <= 128,
        "invalid recording state/reason"
    );
    ensure!(
        i.schema == 1
            && hex(&i.session_id, 32)
            && !i.prefix.is_empty()
            && i.prefix.len() <= 110
            && i.prefix
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
        "invalid recording identity"
    );
    ensure!(
        (1..=300).contains(&i.segment_seconds) && i.parts.len() <= MAX_PARTS,
        "invalid segment limits"
    );
    let mut last_end = 0;
    let mut last_number = 0;
    for p in &i.parts {
        ensure!(
            p.reason.len() <= 128
                && p.recording.warnings.iter().all(|s| s.len() <= 4096)
                && p.observation_range.is_none_or(|r| r[0] <= r[1])
                && p.utc_range.is_none_or(|r| r[0] <= r[1]),
            "invalid segment summary"
        );
        let expected = filename(&i.prefix, p.number);
        let recovered = expected.replace(".gif", ".recovered.gif");
        ensure!(
            p.number > last_number
                && p.start_ms >= last_end
                && p.recording.duration_ms > 0
                && p.recording.duration_ms <= i.segment_seconds as u64 * 1000,
            "invalid segment ordering/duration"
        );
        ensure!(
            p.recording.output == Path::new(&expected)
                || p.recording.output == Path::new(&recovered),
            "segment path escapes recording group"
        );
        ensure!(
            p.recording
                .timeline
                .as_ref()
                .is_none_or(|v| *v == p.recording.output.with_extension("timeline.json"))
                && p.recording
                    .manifest
                    .as_ref()
                    .is_none_or(|v| *v == p.recording.output.with_extension("capture.json")),
            "invalid segment metadata paths"
        );
        ensure!(
            p.recording.bytes <= 256 * 1024 * 1024 && p.recording.warnings.len() <= 64,
            "invalid segment resource budget"
        );
        last_end = p
            .start_ms
            .checked_add(p.recording.duration_ms)
            .context("segment time overflow")?;
        last_number = p.number;
    }
    ensure!(
        i.failure.as_ref().is_none_or(|s| s.len() <= 16384),
        "failure string exceeds budget"
    );
    Ok(())
}
pub fn read_index(input: &Path) -> Result<Index> {
    let root = directory(input)?;
    let i: Index = read_json(&root.join("recording.json"), INDEX_LIMIT)?;
    validate_index(&i)?;
    Ok(i)
}
pub fn save_index(root: &Path, index: &Index) -> Result<()> {
    validate_index(index)?;
    let target = root.join("recording.json");
    if target.exists()
        && let Ok(old) = read_json::<Index>(&target, INDEX_LIMIT)
    {
        crate::storage::replace_json_bounded(
            &root.join("recording.previous.json"),
            &old,
            INDEX_LIMIT,
        )?;
    }
    crate::storage::replace_json_bounded(&target, index, INDEX_LIMIT)
}
pub fn process_stamp(pid: u32) -> Result<Option<u64>> {
    use windows_sys::Win32::{Foundation::*, System::Threading::*};
    if pid == 0 {
        return Ok(None);
    }
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x00100000, 0, pid);
        if h.is_null() {
            ensure!(
                GetLastError() == ERROR_INVALID_PARAMETER,
                "cannot establish recording owner liveness"
            );
            return Ok(None);
        }
        let state = WaitForSingleObject(h, 0);
        if state == 0 {
            CloseHandle(h);
            return Ok(None);
        }
        let mut c = std::mem::zeroed::<FILETIME>();
        let mut e = c;
        let mut k = c;
        let mut u = c;
        let ok = GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u);
        CloseHandle(h);
        ensure!(
            state == 258 && ok != 0,
            "cannot read recording owner identity"
        );
        Ok(Some(
            (c.dwHighDateTime as u64) << 32 | c.dwLowDateTime as u64,
        ))
    }
}
pub fn create(args: &SeriesArgs) -> Result<Plan> {
    args.validate()?;
    let parent = args.output_dir.canonicalize()?;
    let _lock = OperationLock::acquire(&parent)?;
    let _lease = crate::file_guard::Lease::output(&parent.join(".framelet-create"))?;
    if let Some(key) = &args.request_id {
        let mut count = 0;
        for entry in std::fs::read_dir(&parent)? {
            count += 1;
            ensure!(
                count <= 4096,
                "too many output entries for safe request_id discovery"
            );
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && entry.path().extension().is_some_and(|e| e == "framelet")
            {
                let old = read_index(&entry.path())?;
                if old.request_id.as_ref() == Some(key) {
                    let mut expected = args.clone();
                    expected.output_dir = parent.clone();
                    ensure!(
                        serde_json::to_value(&expected)? == serde_json::to_value(&old.request)?,
                        "request_id already used with different parameters"
                    );
                    return Ok(Plan {
                        directory: entry.path(),
                        index: old,
                    });
                }
            }
        }
    }
    let session_id = crate::identity::new_id()?;
    let mut time = unsafe { std::mem::zeroed() };
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut time) };
    let prefix = format!(
        "{}__{:04}{:02}{:02}-{:02}{:02}{:02}__{}",
        args.name,
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        &session_id[..8]
    );
    let root = parent.join(format!("{prefix}.framelet"));
    ensure!(
        root.join(format!("{prefix}.part-000001.timeline.json"))
            .as_os_str()
            .len()
            < 240,
        "output path too long; shorten output_dir or name"
    );
    std::fs::create_dir(&root).context("create exclusive recording group")?;
    std::fs::create_dir(root.join(".work"))?;
    let mut request = args.clone();
    request.output_dir = parent;
    let index = Index {
        schema: 1,
        session_id,
        prefix,
        state: "starting".into(),
        reason: "starting".into(),
        owner_pid: std::process::id(),
        owner_created: process_stamp(std::process::id())?.context("owner exited")?,
        segment_seconds: args.segment_seconds,
        request_id: args.request_id.clone(),
        request,
        parts: vec![],
        failure: None,
    };
    save_index(&root, &index)?;
    Ok(Plan {
        directory: root,
        index,
    })
}
pub fn part_summary(root: &Path, index: &Index) -> Value {
    json!({"directory":root,"index":root.join("recording.json"),"session_id":index.session_id,"state":index.state,"reason":index.reason,"committed_parts":index.parts.len(),"recorded_ms":index.parts.last().map_or(0,|p|p.start_ms+p.recording.duration_ms),"failure":index.failure,"next":"inspect recording.json; recover only after owner exits"})
}
pub fn inspect(input: &Path, offset: usize, limit: usize) -> Result<Value> {
    ensure!(limit > 0 && limit <= 100, "limit must be 1..100");
    let root = directory(input)?;
    let i = read_index(&root)?;
    let parts: Vec<_> = i.parts.iter().skip(offset).take(limit).collect();
    let mut v = part_summary(&root, &i);
    v["parts"] = serde_json::to_value(parts)?;
    v["offset"] = json!(offset);
    v["next_offset"] = if offset.saturating_add(limit) < i.parts.len() {
        json!(offset + limit)
    } else {
        Value::Null
    };
    Ok(v)
}
pub fn discover(root: &Path, offset: usize, limit: usize) -> Result<Value> {
    ensure!(limit > 0 && limit <= 100, "limit must be 1..100");
    let mut groups = vec![];
    let mut seen = 0;
    let mut truncated = false;
    for entry in std::fs::read_dir(root)? {
        seen += 1;
        if seen > 4096 {
            truncated = true;
            break;
        }
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().extension().is_some_and(|e| e == "framelet")
        {
            groups.push(entry.path())
        }
    }
    groups.sort();
    let total = groups.len();
    let mut items = vec![];
    for p in groups.iter().skip(offset).take(limit) {
        items.push(match read_index(p) {
            Ok(i) => part_summary(p, &i),
            Err(e) => json!({"directory":p,"state":"unreadable","error":e.to_string()}),
        })
    }
    Ok(
        json!({"recordings":items,"truncated":truncated,"next_offset":if offset.saturating_add(limit)<total{Some(offset+limit)}else{None}}),
    )
}
pub fn locate(input: &Path, seconds: f64) -> Result<(PathBuf, f64, u64)> {
    ensure!(
        seconds.is_finite() && (0.0..=86400.0).contains(&seconds),
        "global at must be 0..86400"
    );
    let root = directory(input)?;
    let i = read_index(&root)?;
    let at = (seconds * 1000.0).floor() as u64;
    let p = i
        .parts
        .iter()
        .find(|p| at >= p.start_ms && at < p.start_ms + p.recording.duration_ms)
        .context("timestamp outside committed segments")?;
    let file = root.join(&p.recording.output);
    crate::transaction::verify(&file)?;
    Ok((file, seconds - p.start_ms as f64 / 1000.0, p.start_ms))
}
pub fn disk_usage(root: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut count = 0;
    for dir in [root.to_path_buf(), root.join(".work")] {
        for e in std::fs::read_dir(dir)? {
            count += 1;
            ensure!(
                count <= MAX_PARTS * 8 + 32,
                "recording directory resource limit"
            );
            let e = e?;
            // Windows returns enumeration metadata without reopening a path that a finalizer
            // may just have renamed/deleted. It also retains non-following reparse semantics.
            let m = e
                .metadata()
                .context("measure recording disk usage snapshot")?;
            ensure!(
                !m.file_type().is_symlink(),
                "unexpected reparse entry in recording"
            );
            if m.is_file() {
                total = total.checked_add(m.len()).context("disk usage overflow")?;
            }
        }
    }
    Ok(total)
}
pub fn free_space(root: &Path) -> Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<_> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free = 0;
    ensure!(
        unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } != 0,
        "cannot query free disk space"
    );
    Ok(free)
}
pub fn relative_stats(stats: &mut crate::encode::Stats) -> Result<()> {
    stats.output = stats.output.file_name().context("missing GIF name")?.into();
    stats.timeline = stats
        .timeline
        .as_ref()
        .map(|p| PathBuf::from(p.file_name().unwrap_or_default()));
    stats.manifest = stats
        .manifest
        .as_ref()
        .map(|p| PathBuf::from(p.file_name().unwrap_or_default()));
    Ok(())
}
pub fn commit_part(root: &Path, part: &Part) -> Result<()> {
    let target = root.join(part.recording.output.with_extension("segment.json"));
    let mut file = crate::storage::StagedFile::new(&target)?;
    serde_json::to_writer(&mut file.file, part)?;
    file.publish(&target)?;
    Ok(())
}
