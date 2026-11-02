mod args;
mod engine;
mod model;
mod recovery;
mod review;
pub use review::extract;
mod snapshot;
mod store;
pub use snapshot::ReviewSnapshot;
#[cfg(test)]
mod tests;
pub use args::SeriesArgs;
pub use engine::run_local;
pub use model::Plan;
pub use recovery::recover;
pub use store::{create, discover, inspect, locate, read_index};
pub fn is_group(input: &std::path::Path) -> bool {
    input.is_dir() || input.file_name().is_some_and(|s| s == "recording.json")
}
pub fn query(
    input: &std::path::Path,
    at: Option<f64>,
    utc: Option<u64>,
    event: Option<u64>,
    options: &crate::timeline::QueryOptions,
) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        [at.is_some(), utc.is_some(), event.is_some()]
            .into_iter()
            .filter(|v| *v)
            .count()
            <= 1,
        "choose at OR utc_us OR event_id"
    );
    anyhow::ensure!(
        options.time_range.is_none() && options.cursor.is_none(),
        "choose a segment directly for paginated/range queries"
    );
    if let Some(t) = at {
        let (file, local, start) = locate(input, t)?;
        let mut value = crate::timeline::query(&file, Some(local), None, None, options)?;
        value["recording_group"] = serde_json::json!(input);
        value["segment_start_ms"] = serde_json::json!(start);
        return Ok(value);
    }
    if utc.is_none() && event.is_none() {
        return inspect(input, 0, 16);
    }
    let root = store::directory(input)?;
    let index = read_index(input)?;
    let candidates: Vec<_> = index
        .parts
        .iter()
        .filter(|p| {
            if let Some(id) = event {
                p.observation_range
                    .is_some_and(|r| id >= r[0] && id <= r[1])
            } else {
                utc.is_some_and(|u| p.utc_range.is_some_and(|r| u >= r[0] && u <= r[1]))
            }
        })
        .collect();
    anyhow::ensure!(
        candidates.len() <= 16,
        "ambiguous observation ranges; choose a segment directly"
    );
    for part in candidates {
        let value = crate::timeline::query(
            &root.join(&part.recording.output),
            None,
            utc,
            event,
            options,
        )?;
        if !value["events"].as_array().is_none_or(|v| v.is_empty()) || !value["sample"].is_null() {
            return Ok(value);
        }
    }
    anyhow::bail!("no matching observation in committed segments")
}
use anyhow::Result;
use serde_json::Value;
pub fn run(
    plan: &Plan,
    control: &crate::recording::Control,
    external_stop: impl Fn() -> bool,
) -> Result<Value> {
    let result = crate::worker::supervise_series(plan, control, external_stop);
    if let Err(e) = &result
        && let Ok(mut index) = read_index(&plan.directory)
    {
        index.state = if index.parts.is_empty() {
            "failed"
        } else {
            "partial"
        }
        .into();
        index.reason = "worker_failure".into();
        index.failure = Some(format!("{e:#}").chars().take(4096).collect());
        if let Err(save) = store::save_index(&plan.directory, &index) {
            crate::runtime::report("series.index.failure", &save);
        }
    }
    result
}
