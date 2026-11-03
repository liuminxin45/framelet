//! Prepare a separate demo candidate. Publication is no-overwrite and never destroys the raw capture.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(clap::Args, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    pub input: PathBuf,
    #[arg(short, long)]
    pub output: PathBuf,
    #[arg(long)]
    pub start: Option<f64>,
    #[arg(long)]
    pub end: Option<f64>,
}
pub fn run(a: &Args) -> Result<Value> {
    ensure!(
        !crate::series::is_group(&a.input),
        "choose one committed series part for demo preparation; analyze group windows first"
    );
    crate::safety::extension(&a.output, "gif")?;
    ensure!(
        !a.output.try_exists()?,
        "candidate output already exists; inspect it or choose a distinct path"
    );
    ensure!(
        a.start.is_some() == a.end.is_some(),
        "provide both start and end, or neither"
    );
    ensure!(
        a.start.is_none_or(|s| s.is_finite() && s >= 0.)
            && a.end
                .is_none_or(|e| e.is_finite() && e > a.start.unwrap_or(0.)),
        "invalid demo range"
    );
    // Hold source and optional sidecars stable through analysis and edit.
    let _source = crate::file_guard::Lease::input(&a.input)?;
    let mut leases = Vec::new();
    for p in [
        crate::timeline::sidecar_path(&a.input),
        crate::transaction::manifest(&a.input),
    ] {
        if p.try_exists()? {
            leases.push(crate::file_guard::Lease::input(&p)?);
        }
    }
    let before = super::analyze::run(&super::analyze::Args {
        input: a.input.clone(),
        output: None,
        start: 0.,
        end: None,
        min_change: 0.04,
        padding: 0.25,
        count: 6,
        include_image: false,
    })?;
    let range = if let (Some(start), Some(end)) = (a.start, a.end) {
        [start, end]
    } else {
        ensure!(
            before["truncated"] != true,
            "candidate intervals truncated; inspect and provide an explicit range"
        );
        let span=before["quality"]["suggested_trim_ms"].as_array().context("no unambiguous trim suggestion; inspect markers/actions and provide explicit start/end")?;
        [
            span[0].as_u64().context("invalid suggested start")? as f64 / 1000.,
            span[1].as_u64().context("invalid suggested end")? as f64 / 1000.,
        ]
    };
    ensure!(
        range[1]
            <= before["analyzed_end_ms"]
                .as_u64()
                .context("missing duration")? as f64
                / 1000.,
        "demo range exceeds source duration"
    );
    let edited = crate::encode::edit(&a.input, &a.output, range[0], Some(range[1]), 1.)?;
    let state = if edited.stats.warnings.is_empty() {
        "candidate_saved"
    } else {
        "candidate_saved_with_warnings"
    };
    let mut value = json!({"state":state,"output":edited.stats.output,"source":a.input.canonicalize()?,"source_range_seconds":range,
        "recording":edited,"source_quality":before["quality"],"visual_review_required":true,
        "next":"batch-probe the candidate opening, every main action and closing result; review the GIF before publishing. If motion is under-sampled, re-record; never fabricate frames."});
    // An optional post-publication check must not hide an already saved GIF on failure.
    match super::analyze::run(&super::analyze::Args {
        input: a.output.clone(),
        output: None,
        start: 0.,
        end: None,
        min_change: 0.04,
        padding: 0.25,
        count: 6,
        include_image: false,
    }) {
        Ok(after) => {
            value["quality"] = after["quality"].clone();
            value["ranges"] = after["ranges"].clone();
        }
        Err(e) => {
            value["state"] = json!("candidate_saved_review_failed");
            value["review_error"] = json!(format!("{e:#}"));
            value["failure"] = json!(crate::runtime::report("demo.review", &e));
        }
    }
    Ok(value)
}
