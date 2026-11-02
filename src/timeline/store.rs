use super::model::*;
use anyhow::{Result, ensure};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
pub fn path(gif: &Path) -> PathBuf {
    gif.with_extension("timeline.json")
}
pub fn available(gif: &Path) -> Result<()> {
    ensure!(
        matches!(path(gif).symlink_metadata(),Err(e) if e.kind()==std::io::ErrorKind::NotFound),
        "timeline output already exists or is inaccessible"
    );
    Ok(())
}
pub fn publish(gif: &Path, data: &Data) -> Result<PathBuf> {
    let _lease = crate::file_guard::Lease::input(gif)?;
    let target = path(gif);
    let mut data = serde_json::to_value(data)?;
    data["gif_sha256"] = serde_json::json!(crate::identity::sha256(gif)?);
    let mut stage = crate::storage::StagedFile::new(&target)?;
    crate::json_io::write(&mut stage.file, &data)?;
    stage.file.flush()?;
    ensure!(
        stage.file.metadata()?.len() <= 64 * 1024 * 1024,
        "timeline exceeds 64 MiB; GIF preserved"
    );
    let _warnings = stage.publish(&target)?;
    Ok(target)
}
pub fn load(gif: &Path) -> Result<Option<Data>> {
    crate::transaction::verify(gif)?;
    let _gif_lease = crate::file_guard::Lease::input(gif)?;
    let p = path(gif);
    if !p.try_exists()? {
        return Ok(None);
    }
    // An adjacent sidecar must not redirect MCP reads outside the GIF's allowed root.
    ensure!(
        p.symlink_metadata()?.file_type().is_file(),
        "timeline must be a regular file"
    );
    ensure!(
        p.canonicalize()?.parent() == gif.canonicalize()?.parent(),
        "timeline escapes GIF directory"
    );
    ensure!(
        p.metadata()?.len() <= 64 * 1024 * 1024,
        "timeline exceeds 64 MiB"
    );
    let _sidecar_lease = crate::file_guard::Lease::input(&p)?;
    let d: Data = crate::json_io::read(std::fs::File::open(p)?)?;
    ensure!(
        d.schema == 2 && d.samples.len() <= MAX_SAMPLES && d.events.len() <= MAX_EVENTS,
        "unsupported timeline or resource limit"
    );
    ensure!(
        d.region.width > 0
            && d.region.height > 0
            && d.width > 0
            && d.height > 0
            && d.duration_ms <= 3_600_000
            && d.samples
                .windows(2)
                .all(|w| w[0].capture_us <= w[1].capture_us)
            && d.samples
                .iter()
                .all(|s| s.hold_end_us > s.coverage_start_us && s.coverage_start_us >= s.capture_us)
            && d.events
                .iter()
                .all(|e| e.media_us.is_none_or(|m| m < d.duration_ms * 1000)
                    && e.gif_frame.is_none_or(|f| f < d.frames)),
        "invalid timeline events/order"
    );
    let (info, intervals) = crate::encode::inventory(gif)?;
    ensure!(
        info["bytes"].as_u64() == Some(d.gif_bytes)
            && info["duration_ms"].as_u64() == Some(d.duration_ms)
            && info["frames"].as_u64() == Some(d.frames)
            && info["width"].as_u64() == Some(d.width as u64)
            && info["height"].as_u64() == Some(d.height as u64),
        "timeline/GIF mismatch"
    );
    ensure!(
        d.samples.iter().all(|s| s.media_start_us < s.media_end_us
            && s.media_end_us <= d.duration_ms * 1000
            && s.gif_frame < d.frames
            && s.capture_us <= s.capture_end_us),
        "invalid sample mapping"
    );
    let point_ok = |p: Option<[i32; 2]>| {
        p.is_none_or(|p| {
            p[0] >= 0
                && p[1] >= 0
                && (p[0] as u32) < d.region.width
                && (p[1] as u32) < d.region.height
        })
    };
    ensure!(
        d.session_id.len() <= 128
            && d.warnings.len() <= 64
            && d.warnings.iter().all(|w| w.len() <= 4096),
        "timeline strings exceed limits"
    );
    ensure!(
        d.samples
            .windows(2)
            .all(|w| w[0].media_end_us <= w[1].media_start_us
                && w[0].hold_end_us <= w[1].coverage_start_us)
            && d.events.windows(2).all(|w| w[0].mono_us <= w[1].mono_us),
        "timeline ranges overlap or are unordered"
    );
    ensure!(
        d.samples.iter().all(|s| point_ok(s.cursor)
            && intervals
                .get(s.gif_frame as usize)
                .is_some_and(|&(a, b)| s.media_start_us >= a * 1000 && s.media_end_us <= b * 1000)),
        "sample frame/coordinate mapping mismatch"
    );
    ensure!(
        d.events.iter().all(|e| point_ok(e.point)
            && match (e.media_us, e.gif_frame) {
                (None, None) => true,
                (Some(t), Some(f)) => intervals
                    .get(f as usize)
                    .is_some_and(|&(a, b)| t >= a * 1000 && t < b * 1000),
                _ => false,
            }),
        "event frame/coordinate mapping mismatch"
    );
    if let Some(expected) = &d.gif_sha256 {
        ensure!(
            expected == &crate::identity::sha256(gif)?,
            "GIF content SHA-256 mismatch"
        );
    }
    {
        let hex = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
        ensure!(
            hex(&d.session_id, 32)
                && d.asset_id.as_ref().is_some_and(|s| hex(s, 32))
                && d.gif_sha256.as_ref().is_some_and(|s| hex(s, 64)),
            "schema-2 requires random session/asset IDs and a content digest"
        );
        let mut ids = std::collections::HashSet::new();
        for e in &d.events {
            ensure!(
                e.id.is_some_and(|id| ids.insert(id)),
                "duplicate/missing event ID"
            );
            ensure!(
                [256, 257, 260, 261].contains(&e.kind) || e.device != Device::Keyboard,
                "invalid keyboard message kind"
            );
            ensure!(
                e.device != Device::Keyboard
                    || (e.vk.is_some_and(|v| v <= 255)
                        && e.scan.is_some()
                        && e.key_name.as_ref().is_none_or(|s| s.len() <= 256)),
                "invalid keyboard event"
            );
        }
        let mut sample_ids: std::collections::HashMap<u64, Vec<&Sample>> =
            std::collections::HashMap::new();
        for s in &d.samples {
            let id = s.id.ok_or_else(|| anyhow::anyhow!("missing sample ID"))?;
            ensure!(!ids.contains(&id), "sample/event ID collision");
            let spans = sample_ids.entry(id).or_default();
            if let Some(old) = spans.first() {
                ensure!(
                    old.seq == s.seq
                        && old.utc_us == s.utc_us
                        && old.capture_us == s.capture_us
                        && old.capture_end_us == s.capture_end_us,
                    "sample identity collision"
                );
            }
            spans.push(s);
        }
        for e in &d.events {
            if let Some(id) = e.sample_id {
                let spans = sample_ids
                    .get(&id)
                    .ok_or_else(|| anyhow::anyhow!("unknown event sample ID"))?;
                let time = e
                    .media_us
                    .ok_or_else(|| anyhow::anyhow!("linked event has no media time"))?;
                let sample = spans
                    .get(spans.partition_point(|s| s.media_end_us <= time))
                    .filter(|s| s.media_start_us <= time)
                    .ok_or_else(|| anyhow::anyhow!("event/sample interval mismatch"))?;
                let offset = (e.mono_us as i128 - sample.capture_us as i128)
                    .clamp(i64::MIN as i128, i64::MAX as i128) as i64;
                ensure!(
                    e.gif_frame == Some(sample.gif_frame) && e.offset_us == Some(offset),
                    "event/sample frame or offset mismatch"
                );
            } else {
                ensure!(
                    e.offset_us.is_none(),
                    "unmapped event has a stale sample offset"
                );
            }
        }
    }
    Ok(Some(d))
}
