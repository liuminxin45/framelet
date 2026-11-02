use super::{model::*, store::*};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

pub fn recover(input: &Path) -> Result<Value> {
    let root = directory(input)?;
    let _lock = OperationLock::acquire(&root)?;
    let _lease = crate::file_guard::Lease::output(&root.join("recording.json"))?;
    let mut i = read_index(&root).or_else(|_| {
        let old: Index = read_json(&root.join("recording.previous.json"), INDEX_LIMIT)?;
        validate_index(&old)?;
        Ok::<_, anyhow::Error>(old)
    })?;
    ensure!(
        process_stamp(i.owner_pid)? != Some(i.owner_created),
        "recording owner is still active; stop before recovery"
    );
    let mut checkpoints = vec![];
    let mut count = 0;
    for entry in std::fs::read_dir(&root)? {
        count += 1;
        ensure!(
            count <= MAX_PARTS * 5 + 16,
            "recording directory resource limit"
        );
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&format!("{}.part-", i.prefix)) && name.ends_with(".segment.json") {
            let part: Part = read_json(&entry.path(), 512 * 1024)?;
            let mut check = i.clone();
            check.parts = vec![part.clone()];
            validate_index(&check)?;
            let media = root.join(&part.recording.output);
            let _media_lease = crate::file_guard::Lease::input(&media)?;
            ensure!(
                media.metadata()?.len() == part.recording.bytes,
                "committed media size mismatch"
            );
            if part.recording.manifest.is_some() {
                ensure!(
                    crate::transaction::manifest(&media).is_file(),
                    "committed manifest missing"
                );
                crate::transaction::verify(&media)?;
            }
            if !i.parts.iter().any(|p| p.number == part.number) {
                i.parts.push(part)
            }
        }
    }
    for entry in std::fs::read_dir(root.join(".work"))? {
        let entry = entry?;
        count += 1;
        ensure!(
            count <= MAX_PARTS * 8 + 32,
            "checkpoint directory resource limit"
        );
        if entry
            .file_name()
            .to_string_lossy()
            .ends_with(".checkpoint.json")
        {
            checkpoints.push(entry.path())
        }
    }
    checkpoints.sort();
    let mut recovered = 0;
    let mut errors = vec![];
    for cp in checkpoints {
        let result = (|| -> Result<()> {
            let c: Checkpoint = read_json(&cp, 64 * 1024 * 1024)?;
            ensure!(
                c.schema == 1
                    && c.session_id == i.session_id
                    && c.timeline.session_id == i.session_id,
                "checkpoint identity mismatch"
            );
            if i.parts.iter().any(|p| p.number == c.number) {
                return Ok(());
            }
            ensure!(
                c.number > 0
                    && c.number as usize <= MAX_PARTS
                    && c.target == filename(&i.prefix, c.number),
                "invalid checkpoint target"
            );
            ensure!(
                Path::new(&c.source).file_name() == Some(std::ffi::OsStr::new(&c.source))
                    && c.source.starts_with(&format!("{}.", c.target))
                    && c.source.ends_with(".partial"),
                "checkpoint source escapes work directory"
            );
            ensure!(
                c.safe_bytes > 0
                    && c.safe_bytes <= 256 * 1024 * 1024
                    && c.recording.duration_ms > 0
                    && c.recording.duration_ms <= i.segment_seconds as u64 * 1000,
                "invalid checkpoint length/duration"
            );
            let source = root.join(".work").join(&c.source);
            let _input = crate::file_guard::Lease::input(&source)?;
            ensure!(
                hex(&c.gif_prefix_sha256, 64)
                    && crate::identity::sha256_reader(File::open(&source)?.take(c.safe_bytes))?
                        == c.gif_prefix_sha256,
                "checkpoint GIF prefix digest mismatch"
            );
            let mut name = c.target.clone();
            if root.join(&name).exists() {
                name = name.replace(".gif", ".recovered.gif")
            }
            let target = root.join(name);
            ensure!(
                !target.exists(),
                "recovery output already exists without a commit"
            );
            let candidate = root
                .join(".work")
                .join(format!("recovery-{}.gif", crate::identity::new_id()?));
            let mut staged = crate::storage::StagedFile::new(&candidate)?;
            let mut source = File::open(&source)?.take(c.safe_bytes);
            ensure!(
                std::io::copy(&mut source, &mut staged.file)? == c.safe_bytes,
                "truncated checkpoint source"
            );
            staged.file.write_all(&[0x3b])?;
            staged.file.flush()?;
            staged.publish(&candidate)?;
            let info = crate::encode::inspect(&candidate)?;
            ensure!(
                info["duration_ms"].as_u64() == Some(c.recording.duration_ms)
                    && info["frames"].as_u64() == Some(c.recording.frames),
                "checkpoint GIF/time mismatch"
            );
            // Decode every frame before publication; inventory alone is insufficient for corrupt image blocks.
            crate::frames::validate_all(&candidate)?;
            crate::storage::copy(&candidate, &target)?;
            std::fs::remove_file(&candidate)?;
            let mut stats = c.recording;
            stats.output = target.clone();
            stats.bytes = target.metadata()?.len();
            let mut data = c.timeline;
            data.gif_bytes = stats.bytes;
            stats.timeline = Some(crate::timeline::publish(&target, &data)?);
            // Strict validation binds every event/sample to actual GIF intervals.
            crate::timeline::load(&target)?;
            stats.manifest = Some(crate::transaction::complete(&target)?);
            stats.warnings.push(
                "recovered to last durable checkpoint; later observations may be missing".into(),
            );
            relative_stats(&mut stats)?;
            let part = Part::new(
                c.number,
                c.start_ms,
                "recovered_checkpoint".into(),
                stats,
                &data,
            );
            commit_part(&root, &part)?;
            i.parts.push(part);
            recovered += 1;
            Ok(())
        })();
        if let Err(e) = result
            && errors.len() < 16
        {
            errors.push(format!("{}: {e:#}", cp.display()))
        }
    }
    i.parts.sort_by_key(|p| p.number);
    validate_index(&i)?;
    if recovered > 0 || !matches!(i.state.as_str(), "completed" | "partial" | "cancelled") {
        i.state = if i.parts.is_empty() {
            "failed"
        } else {
            "partial"
        }
        .into();
        i.reason = "recovery".into();
    }
    if !errors.is_empty() {
        i.state = if i.parts.is_empty() {
            "failed"
        } else {
            "partial"
        }
        .into();
        i.failure = Some(errors.join("; ").chars().take(4096).collect())
    }
    save_index(&root, &i)?;
    let mut v = part_summary(&root, &i);
    v["recovered_parts"] = json!(recovered);
    v["errors"] = json!(errors);
    Ok(v)
}
