//! Numeric review evidence, never a claim of semantic correctness or visual approval.
use crate::timeline::{Data, Device};
use serde_json::{Value, json};
#[derive(Default)]
pub struct Holds {
    pub first: Option<[u64; 2]>,
    pub last: Option<[u64; 2]>,
    pub longest: Vec<[u64; 2]>,
    pub count: u64,
    pub long_hold_ms: u64,
}
impl Holds {
    pub fn observe(&mut self, at: u64, delay: u64, begin: u64, end: u64) {
        let span = [at.max(begin), (at + delay).min(end)];
        if span[1] <= span[0] {
            return;
        }
        if self.first.is_none() {
            self.first = Some(span);
        }
        self.last = Some(span);
        self.count += 1;
        if span[1] - span[0] > 1000 {
            self.long_hold_ms += span[1] - span[0];
        }
        self.longest.push(span);
        self.longest.sort_by_key(|s| std::cmp::Reverse(s[1] - s[0]));
        self.longest.truncate(8);
    }
}
pub fn report(
    data: Option<&Data>,
    holds: &Holds,
    begin: u64,
    end: u64,
    ranges: &[[u64; 2]],
) -> Value {
    let mut findings = Vec::<Value>::new();
    let mut finding =
        |code: &str, message: &str| findings.push(json!({"code":code,"message":message}));
    let first = holds.first.map_or(0, |s| s[1] - s[0]);
    let last = holds.last.map_or(0, |s| s[1] - s[0]);
    if first > 2000 {
        finding(
            "long_opening_hold",
            "Opening image holds over 2 seconds; review context before trimming.",
        );
    }
    if last > 2000 {
        finding(
            "long_closing_hold",
            "Closing image holds over 2 seconds; preserve necessary result-reading time.",
        );
    }
    if holds
        .longest
        .iter()
        .any(|s| s[1] - s[0] > 3000 && Some(*s) != holds.first && Some(*s) != holds.last)
    {
        finding(
            "long_internal_hold",
            "An internal image holds over 3 seconds; may be intentional loading or reading.",
        );
    }
    let mut sampling = json!({"status":"unknown","reason":"no source timeline; GIF frame count is not sampling FPS"});
    let mut markers = Vec::new();
    let mut explicit = Vec::new();
    let mut open = None;
    let mut marker_issue = false;
    let mut integrity = "unknown";
    if let Some(d) = data {
        integrity = if d.dropped_events == 0 && d.warnings.is_empty() {
            "trace_available"
        } else {
            "incomplete"
        };
        if integrity == "incomplete" {
            finding(
                "trace_incomplete",
                "Input loss or recording warnings exist; inspect original trace before claiming completeness.",
            );
        }
        for event in d.events.iter().filter(|e| e.device == Device::Marker) {
            markers.push(json!({"id":event.id,"marker":event.marker,"media_us":event.media_us,"utc_us":event.utc_us,"mono_us":event.mono_us}));
            if let Some(m) = &event.marker {
                use crate::recording::markers::Kind;
                match (m.kind, event.media_us) {
                    (Kind::Begin, Some(t)) => {
                        if open.replace(t).is_some() {
                            marker_issue = true;
                        }
                    }
                    (Kind::End, Some(t)) => {
                        if let Some(a) = open.take() {
                            if t > a {
                                explicit.push([a / 1000, t.div_ceil(1000)]);
                            } else {
                                marker_issue = true;
                            }
                        } else {
                            marker_issue = true;
                        }
                    }
                    (Kind::Point, Some(_)) => {}
                    _ => marker_issue = true,
                }
            }
        }
        if open.is_some() {
            marker_issue = true;
        }
        if marker_issue {
            finding(
                "marker_interval_incomplete",
                "Unmapped/unpaired markers: do not silently guess their intended boundary; choose an explicit range or inspect adjacent series parts.",
            );
        }
        for span in &mut explicit {
            span[0] = span[0].max(begin);
            span[1] = span[1].min(end);
        }
        explicit.retain(|s| s[1] > s[0]);
        // Limit rate/gap checks to retained action/marker windows, not the static whole recording.
        let windows = if explicit.is_empty() {
            ranges
        } else {
            &explicit
        };
        let selected: Vec<_> = d
            .samples
            .iter()
            .filter(|s| {
                windows
                    .iter()
                    .any(|w| s.media_start_us < w[1] * 1000 && s.media_end_us > w[0] * 1000)
            })
            .collect();
        let mut gaps = Vec::new();
        for pair in selected.windows(2) {
            if pair[0].id == pair[1].id {
                continue;
            }
            // Do not measure across separate action windows or across removed edit gaps.
            if windows.iter().any(|w| {
                pair[0].media_start_us < w[1] * 1000
                    && pair[1].media_end_us > w[0] * 1000
                    && pair[0].media_end_us >= w[0] * 1000
                    && pair[1].media_start_us <= w[1] * 1000
            }) && pair[0].hold_end_us.abs_diff(pair[1].coverage_start_us) <= 1000
            {
                gaps.push(pair[1].capture_us.saturating_sub(pair[0].capture_us));
            }
        }
        let max_gap = gaps.iter().copied().max();
        let total: u64 = gaps.iter().sum();
        let rate = (total > 0).then(|| gaps.len() as f64 * 1e6 / total as f64);
        if d.capture_fps.is_some_and(|f| f < 30) {
            finding(
                "low_presentation_fps",
                "Capture target is below 30 FPS; acceptable for tests, a deliberate compromise for human-facing motion demos.",
            );
        }
        if d.capture_fps
            .zip(rate)
            .is_some_and(|(target, rate)| rate < f64::from(target) * 0.8)
        {
            finding(
                "action_sampling_below_target",
                "Measured action-window sampling is below 80% of the requested target.",
            );
        }
        if d.capture_fps
            .zip(max_gap)
            .is_some_and(|(target, gap)| gap > 3_000_000 / u64::from(target))
        {
            finding(
                "action_sampling_gap",
                "An action-window capture gap exceeds three requested frame periods.",
            );
        }
        if windows.is_empty() {
            finding(
                "no_action_candidates",
                "No action/change interval found; use explicit markers/range for intentional static demonstrations.",
            );
        }
        sampling = json!({"status":if rate.is_some(){"measured"}else{"insufficient_samples"},"requested_fps":d.capture_fps,"action_sample_spans":selected.len(),"measured_action_fps":rate,"longest_action_gap_us":max_gap,"measurement":"original monotonic capture intervals inside candidate windows; encoded frames are not sampling FPS"});
    } else {
        finding(
            "trace_unavailable",
            "No input/sample timeline; visual candidates remain usable but sampling and action coverage are unknown.",
        );
    }
    let candidate = if !marker_issue && !explicit.is_empty() {
        Some([
            explicit[0][0].saturating_sub(500).max(begin),
            (explicit.last().map_or(0, |s| s[1]) + 1000).min(end),
        ])
    } else if marker_issue {
        None
    } else {
        ranges
            .first()
            .zip(ranges.last())
            .map(|(a, b)| [a[0].saturating_sub(250).max(begin), (b[1] + 750).min(end)])
    };
    json!({"capture_integrity":integrity,"presentation_status":if findings.is_empty(){"needs_visual_review"}else{"needs_attention"},"visual_review_required":true,
        "findings":findings,"holds":{"opening_ms":first,"closing_ms":last,"longest":holds.longest,"encoded_images":holds.count,"long_hold_ms":holds.long_hold_ms,"long_hold_fraction":holds.long_hold_ms as f64/end.saturating_sub(begin).max(1) as f64,"long_hold_threshold_ms":1000,"definition":"encoded composited image hold, not semantic inactivity"},
        "sampling":sampling,"markers":markers,"marked_ranges_ms":explicit,"suggested_trim_ms":candidate,
        "trim_basis":if marker_issue{"incomplete_markers"}else if !explicit.is_empty(){"explicit_markers"}else{"input_and_visual_candidates_with_context"},
        "policy":"Preserve raw media. Markers express intent; input is not UI response. Never remove internal waits automatically. Review opening, each main action and final result. A saved file is not an approved demo."})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn holds_clip_to_window_and_never_approve_a_static_file() {
        let mut h = Holds::default();
        h.observe(0, 5000, 1000, 4000);
        h.observe(5000, 100, 1000, 4000);
        let q = report(None, &h, 1000, 4000, &[]);
        assert_eq!(q["holds"]["opening_ms"], 3000);
        assert_eq!(q["sampling"]["status"], "unknown");
        assert_eq!(q["visual_review_required"], true);
        assert!(q["suggested_trim_ms"].is_null());
        assert_ne!(q["presentation_status"], "approved");
    }
}
