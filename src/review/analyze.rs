use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(clap::Args, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    pub input: PathBuf,
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long, default_value_t = 0.)]
    #[serde(default)]
    pub start: f64,
    #[arg(long)]
    pub end: Option<f64>,
    #[arg(long, default_value_t = 0.04)]
    #[serde(default = "change")]
    pub min_change: f64,
    #[arg(long, default_value_t = 0.25)]
    #[serde(default = "padding")]
    pub padding: f64,
    #[arg(long, default_value_t = 6)]
    #[serde(default = "count")]
    pub count: usize,
    #[arg(skip)]
    #[serde(default)]
    pub include_image: bool,
}
fn change() -> f64 {
    0.04
}
fn padding() -> f64 {
    0.25
}
fn count() -> usize {
    6
}
pub fn run(a: &Args) -> Result<Value> {
    ensure!(
        a.start.is_finite()
            && a.start >= 0.
            && a.end.is_none_or(|e| e.is_finite() && e > a.start)
            && a.min_change.is_finite()
            && (0.0..=1.).contains(&a.min_change)
            && a.padding.is_finite()
            && (0.0..=5.).contains(&a.padding)
            && (1..=16).contains(&a.count),
        "invalid analysis time/threshold/padding/count"
    );
    ensure!(
        !a.include_image || a.output.is_some(),
        "output is required for analysis image"
    );
    if crate::series::is_group(&a.input) {
        return group(a);
    }
    let _input_lease = crate::file_guard::Lease::input(&a.input)?;
    let mut visual = super::visual::Visual::default();
    let mut ranges = Vec::<[u64; 2]>::new();
    let mut peaks = Vec::<Value>::new();
    let mut total = 0;
    let mut changes = 0u64;
    let pad = (a.padding * 1000.).round() as u64;
    let begin = (a.start * 1000.).floor() as u64;
    let end = a
        .end
        .map(|t| (t * 1000.).floor() as u64)
        .unwrap_or(u64::MAX);
    let work = crate::frames::scan(
        &a.input,
        // The containing/preceding frame seeds the first in-window difference.
        // Prefix compositing still runs, but does not allocate/compare RGB.
        |_, at, delay, _, _| at < end && at + delay >= begin,
        |index, at, delay, w, h, rgb| {
            total = at + delay;
            let score = if rgb.is_empty() {
                0.
            } else {
                visual.push(rgb, w, h)
            };
            if at >= begin && at < end && score > 0. && score >= a.min_change {
                changes += 1;
                add(
                    &mut ranges,
                    [
                        at.saturating_sub(pad).max(begin),
                        (at + delay.min(100) + pad).min(end),
                    ],
                );
                if peaks.len() < a.count * 2 {
                    peaks.push(json!({"frame_index":index,"timestamp_ms":at,"duration_ms":delay,"spatial_change":score}));
                }
            }
            Ok(at + delay < end)
        },
    )?;
    ensure!(begin < total, "analysis start outside duration");
    let effective_end = total.min(end);
    let mut input_actions = 0usize;
    let mut trace_warnings = Vec::new();
    if let Some(data) = crate::timeline::load(&a.input)? {
        trace_warnings = data.warnings;
        if data.dropped_events > 0 {
            trace_warnings.push(format!("{} input events dropped", data.dropped_events));
        }
        let mut input_ranges = Vec::new();
        for event in data.events.iter().filter(|e| e.kind != 512) {
            if let Some(t) = event
                .media_us
                .map(|t| t / 1000)
                .filter(|t| *t >= begin && *t < effective_end)
            {
                input_actions += 1;
                add(
                    &mut input_ranges,
                    [
                        t.saturating_sub(pad).max(begin),
                        (t + 500 + pad).min(effective_end),
                    ],
                );
            }
        }
        ranges.extend(input_ranges);
        ranges.sort_by_key(|r| r[0]);
        let mut merged = Vec::new();
        for range in ranges {
            add(&mut merged, range);
        }
        ranges = merged;
    }
    for range in &mut ranges {
        range[1] = range[1].min(effective_end);
    }
    let matched_ranges = ranges.len();
    ranges.truncate(32);
    let mut result = json!({"input":a.input.canonicalize()?,"analyzed_start_ms":begin,"analyzed_end_ms":effective_end,"ranges":ranges,"range_count":matched_ranges,"truncated":matched_ranges>32,"changed_frames":changes,"input_actions":input_actions,"candidates":peaks,"candidates_truncated":changes>a.count as u64*2,"semantic_analysis":false,"algorithm":"16px spatial change + input actions + padded merged intervals","warnings":trace_warnings,"work":work,"next":"Review candidate boundaries, then edit the chosen interval; input time is not visual response time."});
    if let Some(output) = &a.output {
        let span = ranges.first().map(|r| r[0]).unwrap_or(begin);
        let finish = ranges
            .last()
            .map(|r| r[1])
            .unwrap_or(effective_end)
            .max(span + 1);
        let times: Vec<_> = (0..a.count)
            .map(|i| {
                let t = if a.count == 1 {
                    (span + finish) / 2
                } else {
                    span + i as u64 * (finish - span - 1) / (a.count - 1) as u64
                };
                t as f64 / 1000.
            })
            .collect();
        let preview = super::probe::batch(
            &a.input,
            &times,
            Default::default(),
            Some(output),
            super::probe::Export::Sheet,
        )?;
        result["review_frames"] = preview["frames"].clone();
        result["contact_sheet"] = preview["contact_sheet"].clone();
        result["output"] = json!(output);
        result["preview_work"] = preview["work"].clone();
    }
    Ok(result)
}
fn add(ranges: &mut Vec<[u64; 2]>, range: [u64; 2]) {
    if range[1] <= range[0] {
        return;
    }
    if let Some(last) = ranges.last_mut()
        && range[0] <= last[1]
    {
        last[1] = last[1].max(range[1]);
        return;
    }
    // Bounded metadata even when every frame is a separate transient.
    if ranges.len() < 100000 {
        ranges.push(range);
    }
}
fn group(a: &Args) -> Result<Value> {
    ensure!(
        a.output.is_none(),
        "choose a segment for analysis image export"
    );
    let index = crate::series::read_index(&a.input)?;
    let root = if a.input.is_dir() {
        a.input.clone()
    } else {
        a.input
            .parent()
            .context("group parent missing")?
            .to_path_buf()
    };
    let end = a.end.unwrap_or(86400.);
    let parts: Vec<_> = index
        .parts
        .iter()
        .filter(|p| {
            p.start_ms as f64 / 1000. < end
                && (p.start_ms + p.recording.duration_ms) as f64 / 1000. > a.start
        })
        .collect();
    ensure!(
        !parts.is_empty() && parts.len() <= 16,
        "select a window covering 1..16 committed segments"
    );
    let mut ranges = Vec::new();
    let mut segments = Vec::new();
    let mut total_pixels = 0u64;
    for part in parts {
        total_pixels = total_pixels.saturating_add(
            part.recording
                .frames
                .saturating_mul(part.recording.width as u64)
                .saturating_mul(part.recording.height as u64),
        );
        ensure!(
            total_pixels <= 2_000_000_000,
            "analysis window exceeds aggregate pixel budget; choose a smaller window"
        );
        let start = part.start_ms as f64 / 1000.;
        let local = run(&Args {
            input: root.join(&part.recording.output),
            output: None,
            start: (a.start - start).max(0.),
            end: Some((end - start).min(part.recording.duration_ms as f64 / 1000.)),
            min_change: a.min_change,
            padding: a.padding,
            count: a.count,
            include_image: false,
        })?;
        for r in local["ranges"].as_array().context("ranges missing")? {
            add(
                &mut ranges,
                [
                    part.start_ms + r[0].as_u64().context("range start")?,
                    part.start_ms + r[1].as_u64().context("range end")?,
                ],
            );
        }
        segments.push(json!({"part":part.number,"path":root.join(&part.recording.output),"input_actions":local["input_actions"],"warnings":local["warnings"]}));
    }
    let count = ranges.len();
    ranges.truncate(32);
    Ok(
        json!({"input":a.input,"ranges":ranges,"range_count":count,"truncated":count>32,"segments":segments,"semantic_analysis":false,"coordinate_space":"global playback milliseconds"}),
    )
}
