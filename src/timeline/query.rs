use super::{
    model::*,
    store::{load, path},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
pub fn query(
    gif: &Path,
    at: Option<f64>,
    utc: Option<u64>,
    event_id: Option<u64>,
) -> Result<Value> {
    if crate::series::is_group(gif) {
        return crate::series::query(gif, at, utc, event_id);
    }
    ensure!(
        at.is_none_or(|v| v.is_finite() && (0.0..=3600.0).contains(&v))
            && [at.is_some(), utc.is_some(), event_id.is_some()]
                .into_iter()
                .filter(|v| *v)
                .count()
                <= 1,
        "choose media at OR utc_us"
    );
    let d=load(gif)?.context("no adjacent timeline; recordings made before this feature have no recoverable interaction history")?;
    let at = at.map(|x| (x * 1e6) as u64);
    let selected_event = event_id.and_then(|id| d.events.iter().find(|e| e.id == Some(id)));
    let sample = d.samples.iter().find(|s| {
        if event_id.is_some() {
            selected_event.is_some_and(|e| {
                e.sample_id == s.id
                    && e.media_us
                        .is_some_and(|m| m >= s.media_start_us && m < s.media_end_us)
            })
        } else if let Some(u) = utc {
            u >= s
                .utc_us
                .saturating_add(s.coverage_start_us.saturating_sub(s.capture_us))
                && u < s
                    .utc_us
                    .saturating_add(s.hold_end_us.saturating_sub(s.capture_us))
        } else {
            at.is_some_and(|a| a >= s.media_start_us && a < s.media_end_us)
        }
    });
    let center = selected_event
        .and_then(|e| e.media_us)
        .or(at.filter(|_| sample.is_some()))
        .or_else(|| {
            sample.map(|s| {
                let delta = utc
                    .unwrap_or(s.utc_us)
                    .saturating_sub(s.utc_us)
                    .saturating_sub(s.coverage_start_us.saturating_sub(s.capture_us));
                s.media_start_us
                    + ((delta as u128 * (s.media_end_us - s.media_start_us) as u128)
                        / s.hold_end_us.saturating_sub(s.coverage_start_us).max(1) as u128)
                        as u64
            })
        });
    let events: Vec<_> = d
        .events
        .iter()
        .filter(|e|if let Some(id)=event_id{e.id==Some(id)}else if at.is_none() && utc.is_none(){true}else{center.is_some_and(|t|e.media_us.is_some_and(|m|m.abs_diff(t)<=500_000))})
        .take(100)
        .map(|e| {
            let mut v = json!({"id":e.id,"device":e.device,"event_uid":e.id.map(|id|format!("{}:observation:{id}",d.session_id)),"sample_id":e.sample_id,"sample_uid":e.sample_id.map(|id|format!("{}:observation:{id}",d.session_id)),"frame_uid":e.gif_frame.map(|f|format!("{}:frame:{f}",d.asset_id.as_deref().unwrap_or(&d.session_id))),"offset_us":e.offset_us,"vk":e.vk,"scan":e.scan,"key_name":e.key_name,"flags":e.flags,"unicode_utf16_unit":if e.device==Device::Keyboard && e.vk==Some(231){e.scan}else{None},"seq":e.seq,"utc_us":e.utc_us,"mono_us":e.mono_us,"kind":e.kind,"point":e.point,"injected":e.injected,"wheel":e.wheel,"media_us":e.media_us,"gif_frame":e.gif_frame});
            v["gif_point"] = json!(e.point.map(|p| [
                p[0] as f64 * d.width as f64 / d.region.width.max(1) as f64,
                p[1] as f64 * d.height as f64 / d.region.height.max(1) as f64
            ]));
            v["desktop_point"] = json!(e.point.map(|p| [
                p[0] as i64 + d.region.x as i64,
                p[1] as i64 + d.region.y as i64
            ]));
            v["action"] = json!(match e.kind {
                WM_KEYDOWN=>"key_down",WM_KEYUP=>"key_up",WM_SYSKEYDOWN=>"system_key_down",WM_SYSKEYUP=>"system_key_up",
                WM_MOUSEMOVE => "move",
                WM_LBUTTONDOWN => "left_down",
                WM_LBUTTONUP => "left_up",
                WM_RBUTTONDOWN => "right_down",
                WM_RBUTTONUP => "right_up",
                WM_MBUTTONDOWN => "middle_down",
                WM_MBUTTONUP => "middle_up",
                WM_MOUSEWHEEL => "wheel",
                WM_MOUSEHWHEEL => "horizontal_wheel",
                WM_XBUTTONDOWN => "x_down",
                WM_XBUTTONUP => "x_up",
                _ => "other",
            });
            v
        })
        .collect();
    let dwells = dwell(&d);
    Ok(
        json!({"timeline":path(gif),"session_id":d.session_id,"asset_id":d.asset_id,"sample_uid":sample.and_then(|s|s.id.map(|id|format!("{}:observation:{id}",d.session_id))),"region":d.region,"width":d.width,"height":d.height,"duration_ms":d.duration_ms,"sample_count":d.samples.len(),"event_count":d.events.len(),"sample":sample,"matched_media_us":center,"events":events,"event_limit":100,"dwell_candidates":dwells,"dropped_events":d.dropped_events,"warnings":d.warnings,"coordinate_space":"source-region-relative physical pixels; GIF x=x*width/region.width, y=y*height/region.height","accuracy":"capture intervals and hook observation times, not hardware-synchronous timestamps; dwell candidates are positional, not semantic hover"}),
    )
}
pub(super) fn dwell(d: &Data) -> Vec<Value> {
    let mut points: Vec<_> = d
        .samples
        .iter()
        .map(|s| (s.coverage_start_us, s.media_start_us, s.cursor))
        .chain(d.events.iter().filter_map(|e| {
            (e.device == Device::Mouse)
                .then_some(e)
                .and_then(|e| e.media_us.map(|m| (e.mono_us, m, e.point)))
        }))
        .collect();
    for pair in d.samples.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        if b.coverage_start_us > a.hold_end_us.saturating_add(1_000) {
            points.push((a.hold_end_us, a.media_end_us, a.cursor));
            points.push((a.hold_end_us.saturating_add(1), a.media_end_us, None));
        }
    }
    points.sort_by_key(|p| p.0);
    if let (Some(s), Some(last)) = (d.samples.last(), points.last().copied()) {
        points.push((s.hold_end_us, s.media_end_us, last.2));
    }
    let mut output = Vec::new();
    let mut anchor: Option<(u64, u64, Option<[i32; 2]>)> = None;
    let mut previous: Option<(u64, u64)> = None;
    for p in points {
        let moved = anchor.is_some_and(|a| match (a.2, p.2) {
            (Some(a), Some(b)) => {
                (a[0] as i64 - b[0] as i64).pow(2) + (a[1] as i64 - b[1] as i64).pow(2) > 16
            }
            _ => true,
        });
        if moved {
            if let (Some(a), Some((mono_end, end))) = (anchor, previous)
                && a.2.is_some()
                && mono_end.saturating_sub(a.0) >= 500_000
                && output.len() < 100
            {
                output.push(json!({"point":a.2,"from_us":a.1,"to_us":end}));
            }
            anchor = None;
        }
        if anchor.is_none() {
            anchor = Some(p);
        }
        previous = Some((p.0, p.1));
    }
    if let (Some(a), Some((mono_end, end))) = (anchor, previous)
        && a.2.is_some()
        && mono_end.saturating_sub(a.0) >= 500_000
        && output.len() < 100
    {
        output.push(json!({"point":a.2,"from_us":a.1,"to_us":end}));
    }
    output
}
