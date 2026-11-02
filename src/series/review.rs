//! A bounded global overview. Scene scoring remains an explicit per-part operation.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::PathBuf;
struct Stage {
    path: PathBuf,
    lease: Option<crate::file_guard::Lease>,
    published: bool,
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.published
            && self
                .path
                .file_name()
                .is_some_and(|s| s.to_string_lossy().starts_with(".framelet-review-"))
        {
            // Only this flat, owned stage is cleaned; never recursively traverse a swapped directory.
            if let Ok(entries) = std::fs::read_dir(&self.path) {
                for e in entries.flatten() {
                    if e.file_type().is_ok_and(|t| t.is_file()) {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            self.lease.take();
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}
pub fn extract(a: &crate::frames::ExtractArgs) -> Result<Value> {
    ensure!(
        (1..=64).contains(&a.count)
            && a.min_gap.is_finite()
            && (0.0..=3600.0).contains(&a.min_gap)
            && a.min_change.is_finite()
            && (0.0..=1.0).contains(&a.min_change)
            && a.padding.is_finite()
            && (0.0..=60.0).contains(&a.padding),
        "invalid extraction settings"
    );
    let root = super::store::directory(&a.input)?;
    let index = super::read_index(&root)?;
    let total = index
        .parts
        .last()
        .map(|p| p.start_ms + p.recording.duration_ms)
        .context("no committed segments to review")?;
    ensure!(!a.output.exists(), "output already exists");
    let output = std::path::absolute(&a.output)?;
    let _parent = crate::file_guard::Lease::output(&output)?;
    let path = output
        .parent()
        .context("missing output parent")?
        .join(format!(".framelet-review-{}", crate::identity::new_id()?));
    std::fs::create_dir(&path)?;
    let lease = crate::file_guard::Lease::output(&path.join("manifest.json"))?;
    let mut stage = Stage {
        path,
        lease: Some(lease),
        published: false,
    };
    let cols = a.count.min(4);
    let rows = a.count.div_ceil(cols);
    let sw = cols * 256;
    let sh = rows * 212;
    let mut sheet = vec![24u8; sw * sh * 3];
    let mut frames = vec![];
    let started = std::time::Instant::now();
    let mut missing = vec![];
    for n in 0..a.count {
        ensure!(
            started.elapsed().as_secs() < 120,
            "group overview exceeded 120 second budget; select a smaller interval/part"
        );
        let timestamp = if a.count == 1 {
            total / 2
        } else {
            n as u64 * (total - 1) / (a.count - 1) as u64
        };
        let (file, local, start) = match super::locate(&root, timestamp as f64 / 1000.0) {
            Ok(v) => v,
            Err(e) => {
                missing.push(json!({"at_ms":timestamp,"error":e.to_string()}));
                continue;
            }
        };
        let name = format!("frame-{:03}.png", n + 1);
        let temporary = stage.path.join(&name);
        let mut meta = crate::frames::at(&file, local, Some(&temporary), Default::default())?;
        let (w, h, rgb) = crate::frames::read_png(&temporary)?;
        let scale = (240.0 / w as f64).min(176.0 / h as f64).min(1.0);
        let tw = (w as f64 * scale).round().max(1.0) as u32;
        let th = (h as f64 * scale).round().max(1.0) as u32;
        let thumb = crate::frames::resize(&rgb, w, h, tw, th);
        let (x, y) = (n % cols * 256 + 8, n / cols * 212 + 24);
        for row in 0..th as usize {
            let at = ((y + row) * sw + x) * 3;
            sheet[at..at + tw as usize * 3]
                .copy_from_slice(&thumb[row * tw as usize * 3..(row + 1) * tw as usize * 3]);
        }
        crate::frames::label(&mut sheet, sw, x, y - 18, n + 1);
        meta["image"] = json!(output.join(name));
        meta["output"] = meta["image"].clone();
        meta["order"] = json!(n + 1);
        meta["segment"] = json!(file);
        meta["segment_start_ms"] = json!(start);
        meta["global_requested_ms"] = json!(timestamp);
        meta["local_timestamp_ms"] = meta["timestamp_ms"].clone();
        if let Some(t) = meta["timestamp_ms"].as_u64() {
            meta["timestamp_ms"] = json!(start + t)
        }
        frames.push(meta);
    }
    let report = json!({"schema_version":"1.1","input":root,"output":output,"manifest":output.join("manifest.json"),"contact_sheet":output.join("contact-sheet.png"),"duration_ms":total,"mode":"uniform","requested_mode":a.mode,"semantic_analysis":false,"selection_note":"Global overview samples committed parts uniformly in time; use scene mode on a chosen part for visual-change ranking. Missing spans are reported, not invented.","frames":frames,"missing":missing});
    crate::encode::screenshot(
        &stage.path.join("contact-sheet.png"),
        sw as u32,
        sh as u32,
        &sheet,
    )?;
    let mut manifest = crate::storage::StagedFile::new(&stage.path.join("manifest.json"))?;
    serde_json::to_writer_pretty(&mut manifest.file, &report)?;
    manifest.publish(&stage.path.join("manifest.json"))?;
    stage.lease.take();
    std::fs::rename(&stage.path, &output)?;
    stage.published = true;
    Ok(report)
}
