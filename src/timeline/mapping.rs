use super::model::*;
use crate::encode::Stats;
// Actual per-frame output timing, including centisecond rounding, drives trim/speed remapping.
pub fn remap(
    mut data: Data,
    spans: &[(u64, u64, u64, u64, u64)],
    stats: &Stats,
) -> anyhow::Result<Data> {
    data.asset_id = Some(crate::identity::new_id()?);
    data.samples = data
        .samples
        .into_iter()
        .flat_map(|s| {
            spans[spans.partition_point(|p| p.1 <= s.media_start_us)..]
                .iter()
                .take_while(move |p| p.0 < s.media_end_us)
                .filter_map(move |&(a, b, c, d, f)| {
                    if b <= a {
                        return None;
                    }
                    let lo = s.media_start_us.max(a);
                    let hi = s.media_end_us.min(b);
                    if hi <= lo {
                        return None;
                    }
                    let mut n = s.clone();
                    n.media_start_us =
                        c + ((lo - a) as u128 * (d - c) as u128 / (b - a) as u128) as u64;
                    n.media_end_us =
                        c + ((hi - a) as u128 * (d - c) as u128 / (b - a) as u128) as u64;
                    n.gif_frame = f;
                    let oldspan = s.media_end_us - s.media_start_us;
                    let shift = ((lo - s.media_start_us) as u128
                        * (s.hold_end_us.saturating_sub(s.coverage_start_us)) as u128
                        / oldspan as u128) as u64;
                    n.coverage_start_us = s.coverage_start_us + shift;
                    n.hold_end_us = s.coverage_start_us
                        + ((hi - s.media_start_us) as u128
                            * (s.hold_end_us.saturating_sub(s.coverage_start_us)) as u128
                            / oldspan as u128) as u64;
                    (n.media_end_us > n.media_start_us && n.hold_end_us > n.coverage_start_us)
                        .then_some(n)
                })
        })
        .take(MAX_SAMPLES)
        .collect();
    if data.samples.len() == MAX_SAMPLES {
        data.warnings
            .push("edited timeline reached sample limit; mapping may be incomplete".into());
    }
    data.events.retain_mut(|e| {
        let Some(t) = e.media_us else {
            return false;
        };
        if let Some(&(a, b, c, d, f)) = spans
            .get(spans.partition_point(|p| p.1 <= t))
            .filter(|p| p.1 > p.0 && t >= p.0 && t < p.1)
        {
            e.media_us = Some(c + ((t - a) as u128 * (d - c) as u128 / (b - a) as u128) as u64);
            e.gif_frame = Some(f);
            true
        } else {
            false
        }
    });
    data.gif_bytes = stats.bytes;
    data.duration_ms = stats.duration_ms;
    data.frames = stats.frames;
    data.gif_sha256 = None;
    for e in &mut data.events {
        if let Some(t) = e.media_us {
            let i = data.samples.partition_point(|s| s.media_end_us <= t);
            if let Some(s) = data.samples.get(i).filter(|s| t >= s.media_start_us) {
                e.sample_id = s.id;
                e.offset_us = Some(
                    (e.mono_us as i128 - s.capture_us as i128)
                        .clamp(i64::MIN as i128, i64::MAX as i128) as i64,
                );
            } else {
                e.sample_id = None;
            }
        }
    }
    Ok(data)
}
