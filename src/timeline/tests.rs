use super::*;
use super::{query::dwell, recorder::Shared};
use crate::{capture::Region, encode::Stats};
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
    time::Instant,
};
use windows_sys::Win32::UI::WindowsAndMessaging::WM_LBUTTONDOWN;
fn fixture() -> Data {
    Data {
        schema: 1,
        asset_id: None,
        gif_sha256: None,
        session_id: "test".into(),
        region: Region {
            x: -100,
            y: 20,
            width: 200,
            height: 100,
        },
        width: 100,
        height: 50,
        gif_bytes: 0,
        duration_ms: 2000,
        frames: 1,
        samples: vec![Sample {
            id: None,
            seq: 0,
            utc_us: 1_000_000,
            capture_us: 100,
            capture_end_us: 200,
            coverage_start_us: 100,
            hold_end_us: 2_000_100,
            media_start_us: 0,
            media_end_us: 2_000_000,
            gif_frame: 0,
            cursor: Some([50, 50]),
        }],
        events: vec![Event {
            id: None,
            device: Device::Mouse,
            vk: None,
            scan: None,
            flags: 0,
            key_name: None,
            sample_id: None,
            offset_us: None,
            seq: 0,
            utc_us: 2_000_000,
            mono_us: 1_000_100,
            kind: WM_LBUTTONDOWN,
            point: Some([50, 50]),
            injected: true,
            wheel: 0,
            media_us: Some(1_000_000),
            gif_frame: Some(0),
        }],
        dropped_events: 0,
        warnings: vec![],
    }
}
#[test]
fn clip_speed_preserves_capture_identity_and_event_utc() {
    let d = fixture();
    let stats = Stats {
        output: PathBuf::new(),
        width: 100,
        height: 50,
        frames: 1,
        samples: 1,
        duration_ms: 500,
        bytes: 100,
        timeline: None,
        manifest: None,
        warnings: vec![],
    };
    let n = remap(d, &[(500_000, 1_500_000, 0, 500_000, 0)], &stats).unwrap();
    assert_eq!(n.samples[0].utc_us, 1_000_000);
    assert_eq!(n.samples[0].capture_us, 100);
    assert_eq!(n.samples[0].coverage_start_us, 500_100);
    assert_eq!(n.samples[0].hold_end_us, 1_500_100);
    assert_eq!(n.events[0].utc_us, 2_000_000);
    assert_eq!(n.events[0].media_us, Some(250_000));
    let n = remap(
        n,
        &[(100_000, 400_000, 0, 300_000, 0)],
        &Stats {
            duration_ms: 300,
            ..stats
        },
    )
    .unwrap();
    assert_eq!(n.events[0].media_us, Some(150_000));
    assert_eq!(n.samples[0].utc_us, 1_000_000);
}
#[test]
fn masks_negative_monitor_and_boundary_coordinates() {
    let shared = Shared {
        start: Instant::now(),
        next_id: AtomicU64::new(0),
        region: Region {
            x: -100,
            y: 20,
            width: 200,
            height: 100,
        },
        masks: vec![Region {
            x: 10,
            y: 10,
            width: 20,
            height: 20,
        }],
        events: Mutex::new(vec![]),
        dropped: AtomicU64::new(0),
        stopping: AtomicBool::new(false),
        callback_failed: AtomicBool::new(false),
    };
    assert_eq!(shared.point(-100, 20), Some([0, 0]));
    assert_eq!(shared.point(-90, 30), None);
    assert_eq!(shared.point(100, 20), None);
}
#[test]
fn dwell_excludes_movement_and_pause_gaps() {
    let mut d = fixture();
    assert!(!dwell(&d).is_empty());
    d.events[0].point = Some([100, 50]);
    assert!(
        dwell(&d)
            .iter()
            .all(|x| x["to_us"].as_u64().unwrap() - x["from_us"].as_u64().unwrap() <= 1_000_000)
    );
    d.events.clear();
    d.samples[0].hold_end_us = 1_000_100;
    d.samples[0].media_end_us = 1_000_000;
    let mut second = d.samples[0].clone();
    second.capture_us = 3_000_100;
    second.coverage_start_us = 3_000_100;
    second.hold_end_us = 4_000_100;
    second.media_start_us = 1_000_000;
    second.media_end_us = 2_000_000;
    d.samples.push(second);
    let intervals = dwell(&d);
    assert_eq!(intervals.len(), 2);
    assert!(
        intervals
            .iter()
            .all(|x| x["to_us"].as_u64().unwrap() - x["from_us"].as_u64().unwrap() <= 1_000_000)
    );
}
