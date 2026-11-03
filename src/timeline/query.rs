use super::{
    model::*,
    store::{load, path},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
#[derive(Clone, Copy, Default, Deserialize, Serialize, clap::ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Detail {
    #[default]
    Summary,
    Full,
}
#[derive(Clone, Deserialize, Serialize, clap::Args)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    #[arg(long,value_enum,default_value_t=Detail::Summary)]
    pub detail: Detail,
    #[arg(long, value_enum)]
    pub device: Option<Device>,
    #[arg(long = "event-kind")]
    pub event_kinds: Vec<u32>,
    #[arg(long)]
    pub include_moves: bool,
    #[arg(long, num_args = 2)]
    pub time_range: Option<Vec<f64>>,
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long, default_value_t = 8)]
    pub limit: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            device: None,
            event_kinds: vec![],
            include_moves: false,
            time_range: None,
            cursor: None,
            limit: 8,
            detail: Detail::Summary,
        }
    }
}
struct Snapshot {
    revision: String,
    data: Data,
    by_id: std::collections::HashMap<u64, usize>,
    dwells: Vec<Value>,
}
pub fn query(
    gif: &Path,
    at: Option<f64>,
    utc: Option<u64>,
    event_id: Option<u64>,
    options: &Options,
) -> Result<Value> {
    if crate::series::is_group(gif) {
        return crate::series::query(gif, at, utc, event_id, options);
    }
    ensure!(
        (1..=100).contains(&options.limit) && options.event_kinds.len() <= 32,
        "limit must be 1..100; at most 32 event kinds"
    );
    ensure!(
        at.is_none_or(|v| v.is_finite() && (0.0..=3600.0).contains(&v))
            && [
                at.is_some(),
                utc.is_some(),
                event_id.is_some(),
                options.time_range.is_some()
            ]
            .into_iter()
            .filter(|v| *v)
            .count()
                <= 1,
        "choose at OR utc_us OR event_id OR time_range"
    );
    if let Some(range) = &options.time_range {
        ensure!(range.len() == 2, "time_range requires two times");
        let (a, b) = (range[0], range[1]);
        ensure!(
            a.is_finite() && b.is_finite() && a >= 0. && b > a && b <= 3600.,
            "invalid time_range"
        );
    }
    let snapshot = crate::review_cache::get(
        gif,
        &[
            gif.to_path_buf(),
            path(gif),
            crate::transaction::manifest(gif),
        ],
        || {
            let data = load(gif)?.context(
                "no adjacent timeline; input history cannot be reconstructed from pixels",
            )?;
            let by_id = data
                .events
                .iter()
                .enumerate()
                .filter_map(|(i, e)| e.id.map(|id| (id, i)))
                .collect();
            let dwells = dwell(&data);
            let bytes = data.samples.capacity() * std::mem::size_of::<Sample>()
                + data.events.capacity() * std::mem::size_of::<Event>()
                + data
                    .events
                    .iter()
                    .map(|e| e.key_name.as_ref().map_or(0, String::capacity))
                    .sum::<usize>()
                + data.events.len() * 64
                + data.warnings.iter().map(String::capacity).sum::<usize>()
                + 131072;
            Ok((
                Snapshot {
                    revision: crate::identity::sha256(&path(gif))?,
                    data,
                    by_id,
                    dwells,
                },
                bytes,
            ))
        },
    )?;
    let d = &snapshot.data;
    let asset = d.asset_id.as_deref().context("timeline asset ID missing")?;
    let at = at.map(|t| (t * 1e6) as u64);
    let selected = event_id.and_then(|id| snapshot.by_id.get(&id).map(|&i| &d.events[i]));
    let sample = if let Some(t) = at {
        d.samples
            .get(d.samples.partition_point(|s| s.media_end_us <= t))
            .filter(|s| s.media_start_us <= t)
    } else {
        d.samples.iter().find(|s| {
            if event_id.is_some() {
                selected.is_some_and(|e| {
                    e.sample_id == s.id
                        && e.media_us
                            .is_some_and(|t| t >= s.media_start_us && t < s.media_end_us)
                })
            } else if let Some(t) = utc {
                t >= s
                    .utc_us
                    .saturating_add(s.coverage_start_us.saturating_sub(s.capture_us))
                    && t < s
                        .utc_us
                        .saturating_add(s.hold_end_us.saturating_sub(s.capture_us))
            } else {
                false
            }
        })
    };
    let center = selected
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
    let source: Box<dyn Iterator<Item = &Event>> = if event_id.is_some() {
        Box::new(selected.into_iter())
    } else {
        Box::new(d.events.iter())
    };
    let mut filtered: Vec<_> = source
        .filter(|e| {
            let selected = if let Some(id) = event_id {
                e.id == Some(id)
            } else if let Some(range) = &options.time_range {
                let (a, b) = (range[0], range[1]);
                e.media_us
                    .is_some_and(|t| t >= (a * 1e6) as u64 && t < (b * 1e6) as u64)
            } else if at.is_some() || utc.is_some() {
                center.is_some_and(|t| e.media_us.is_some_and(|m| m.abs_diff(t) <= 500000))
            } else {
                true
            };
            selected
                && (event_id.is_some() || options.include_moves || e.kind != WM_MOUSEMOVE)
                && options.device.is_none_or(|v| v == e.device)
                && (options.event_kinds.is_empty() || options.event_kinds.contains(&e.kind))
        })
        .collect();
    if let Some(t) = center {
        filtered.sort_by_key(|e| (e.media_us.map_or(u64::MAX, |m| m.abs_diff(t)), e.mono_us));
    }
    let mut identity_options = options.clone();
    identity_options.cursor = None;
    let signature = crate::identity::sha256_reader(std::io::Cursor::new(serde_json::to_vec(&(
        snapshot.revision.as_str(),
        at,
        utc,
        event_id,
        identity_options,
    ))?))?;
    let offset = if let Some(cursor) = &options.cursor {
        ensure!(cursor.len() <= 96, "cursor too long");
        let (stamp, position) = cursor.split_once(':').context("invalid cursor")?;
        ensure!(stamp == signature, "cursor snapshot or query changed");
        position.parse::<usize>()?
    } else {
        0
    };
    ensure!(offset <= filtered.len(), "cursor outside result");
    let end = offset.saturating_add(options.limit).min(filtered.len());
    let events:Vec<_>=filtered[offset..end].iter().map(|e| {
        let mut v=json!({"id":e.id,"device":e.device,"event_uid":e.id.map(|id|format!("{}:observation:{id}",d.session_id)),"sample_id":e.sample_id,"sample_uid":e.sample_id.map(|id|format!("{}:observation:{id}",d.session_id)),"frame_uid":e.gif_frame.map(|f|format!("{asset}:frame:{f}")),"offset_us":e.offset_us,"vk":e.vk,"scan":e.scan,"key_name":e.key_name,"flags":e.flags,"seq":e.seq,"utc_us":e.utc_us,"mono_us":e.mono_us,"kind":e.kind,"point":e.point,"injected":e.injected,"wheel":e.wheel,"media_us":e.media_us,"gif_frame":e.gif_frame});
        v["gif_point"]=json!(e.point.map(|p|[p[0] as f64*d.width as f64/d.region.width.max(1) as f64,p[1] as f64*d.height as f64/d.region.height.max(1) as f64]));
        v["desktop_point"]=json!(e.point.map(|p|[p[0] as i64+d.region.x as i64,p[1] as i64+d.region.y as i64]));
        v["unicode_utf16_unit"]=json!(if e.device==Device::Keyboard&&e.vk==Some(231){e.scan}else{None});
        v["marker"]=json!(e.marker);
        v["action"]=json!(match e.kind{WM_KEYDOWN=>"key_down",WM_KEYUP=>"key_up",WM_SYSKEYDOWN=>"system_key_down",WM_SYSKEYUP=>"system_key_up",WM_MOUSEMOVE=>"move",WM_LBUTTONDOWN=>"left_down",WM_LBUTTONUP=>"left_up",WM_RBUTTONDOWN=>"right_down",WM_RBUTTONUP=>"right_up",WM_MBUTTONDOWN=>"middle_down",WM_MBUTTONUP=>"middle_up",WM_MOUSEWHEEL=>"wheel",WM_MOUSEHWHEEL=>"horizontal_wheel",WM_XBUTTONDOWN=>"x_down",WM_XBUTTONUP=>"x_up",_=>"other"});
        if options.detail==Detail::Summary {
            v.as_object_mut().expect("event object").retain(|key,_|["id","device","action","sample_id","offset_us","utc_us","media_us","gif_frame","point","vk","key_name","marker"].contains(&key.as_str()));
        }v
    }).collect();
    Ok(
        json!({"timeline":path(gif),"session_id":d.session_id,"asset_id":d.asset_id,"sample_uid":sample.and_then(|s|s.id.map(|id|format!("{}:observation:{id}",d.session_id))),"region":d.region,"width":d.width,"height":d.height,"duration_ms":d.duration_ms,"sample_count":d.samples.len(),"event_count":d.events.len(),"sample":sample,"matched_media_us":center,"markers":d.events.iter().filter(|e|e.device==Device::Marker).collect::<Vec<_>>(),"capture_fps":d.capture_fps,"events":events,"matched_total":filtered.len(),"returned_count":end-offset,"truncated":end<filtered.len(),"next_cursor":(end<filtered.len()).then(||format!("{signature}:{end}")),"order":if center.is_some(){"nearest_center"}else{"chronological"},"event_limit":options.limit,"detail":options.detail,"identity_namespace":{"event":"session_id + observation id","frame":"asset_id + gif_frame"},"dwell_candidates":if options.detail==Detail::Full{&snapshot.dwells[..]}else{&snapshot.dwells[..snapshot.dwells.len().min(4)]},"dwell_total":snapshot.dwells.len(),"dropped_events":d.dropped_events,"warnings":d.warnings,"coordinate_space":"source-region-relative physical pixels; GIF x=x*width/region.width, y=y*height/region.height","accuracy":"capture/hook intervals, not hardware-synchronous timestamps; positional dwell is not semantic hover"}),
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
