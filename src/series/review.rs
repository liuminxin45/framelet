//! Bounded global overview: one forward decode per selected segment.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};
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
    let stage = crate::storage::StagedDirectory::new(&output)?;
    let cols = a.count.min(4);
    let rows = a.count.div_ceil(cols);
    let sw = cols * 256;
    let sh = rows * 212;
    let mut sheet = vec![24u8; sw * sh * 3];
    let mut frames = vec![];
    let started = std::time::Instant::now();
    let mut missing = vec![];
    let mut groups = BTreeMap::<PathBuf, Vec<(usize, u64, u64, u64)>>::new();
    for n in 0..a.count {
        let timestamp = if a.count == 1 {
            total / 2
        } else {
            n as u64 * (total - 1) / (a.count - 1) as u64
        };
        let (file, local, start) = match super::locate(&root, timestamp as f64 / 1000.) {
            Ok(v) => v,
            Err(e) => {
                missing.push(json!({"at_ms":timestamp,"error":e.to_string()}));
                continue;
            }
        };
        groups
            .entry(file)
            .or_default()
            .push((n, (local * 1000.).round() as u64, timestamp, start));
    }
    let mut budget = 0u64;
    for file in groups.keys() {
        let part = index
            .parts
            .iter()
            .find(|p| root.join(&p.recording.output) == *file)
            .context("overview segment missing")?;
        budget = budget.saturating_add(
            part.recording
                .frames
                .saturating_mul(part.recording.width as u64)
                .saturating_mul(part.recording.height as u64),
        );
    }
    ensure!(
        budget <= 2_000_000_000,
        "group overview exceeds aggregate pixel budget; choose a smaller part"
    );
    let mut decoded = 0u64;
    let mut converted = 0u64;
    let passes = groups.len();
    for (file, requests) in groups {
        ensure!(
            started.elapsed().as_secs() < 120,
            "group overview exceeded time budget"
        );
        let last = requests
            .iter()
            .map(|r| r.1)
            .max()
            .context("empty overview segment")?;
        let mut found = 0usize;
        let work = crate::frames::scan(
            &file,
            |_, at, delay, _, _| requests.iter().any(|r| r.1 >= at && r.1 < at + delay),
            |index, at, delay, w, h, rgb| {
                for &(n, local, timestamp, start) in
                    requests.iter().filter(|r| r.1 >= at && r.1 < at + delay)
                {
                    let name = format!("frame-{:03}.png", n + 1);
                    crate::encode::screenshot(&stage.path.join(&name), w, h, rgb)?;
                    let scale = (240. / w as f64).min(176. / h as f64).min(1.);
                    let tw = (w as f64 * scale).round().max(1.) as u32;
                    let th = (h as f64 * scale).round().max(1.) as u32;
                    let thumb = crate::frames::resize(rgb, w, h, tw, th);
                    let (x, y) = (n % cols * 256 + 8, n / cols * 212 + 24);
                    for row in 0..th as usize {
                        let d = ((y + row) * sw + x) * 3;
                        sheet[d..d + tw as usize * 3].copy_from_slice(
                            &thumb[row * tw as usize * 3..(row + 1) * tw as usize * 3],
                        );
                    }
                    crate::frames::label(&mut sheet, sw, x, y - 18, n + 1);
                    frames.push(json!({"input":file,"requested_ms":local,"frame_index":index,"timestamp_ms":start+at,"local_timestamp_ms":at,"duration_ms":delay,"width":w,"height":h,"image":output.join(&name),"output":output.join(name),"order":n+1,"segment":file,"segment_start_ms":start,"global_requested_ms":timestamp}));
                    found += 1;
                }
                Ok(at + delay <= last)
            },
        )?;
        ensure!(
            found == requests.len(),
            "group overview timestamp outside actual media; output not published"
        );
        decoded += work.decoded_frames;
        converted += work.converted_frames;
    }
    frames.sort_by_key(|v| v["order"].as_u64());
    let report = json!({"schema_version":"2.0","input":root,"output":output,"manifest":output.join("manifest.json"),"contact_sheet":output.join("contact-sheet.png"),"duration_ms":total,"mode":"uniform","requested_mode":a.mode,"semantic_analysis":false,"selection_note":"Global overview samples committed parts uniformly in time; use scene mode on a chosen part for visual-change ranking. Missing spans are reported, not invented.","frames":frames,"missing":missing,"decode_passes":passes,"work":{"decoded_frames":decoded,"converted_frames":converted}});
    crate::encode::screenshot(
        &stage.path.join("contact-sheet.png"),
        sw as u32,
        sh as u32,
        &sheet,
    )?;
    crate::storage::replace_json(&stage.path.join("manifest.json"), &report)?;
    stage.publish()?;
    Ok(report)
}
