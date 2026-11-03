use crate::capture::Region;
use serde::{Deserialize, Serialize};
pub(super) const MAX_EVENTS: usize = 100_000;
pub(super) const MAX_SAMPLES: usize = 200_000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub id: Option<u64>,
    pub seq: u64,
    pub utc_us: u64,
    pub capture_us: u64,
    pub capture_end_us: u64,
    pub coverage_start_us: u64,
    pub hold_end_us: u64,
    pub media_start_us: u64,
    pub media_end_us: u64,
    pub gif_frame: u64,
    pub cursor: Option<[i32; 2]>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub marker: Option<crate::recording::markers::Request>,
    pub id: Option<u64>,
    pub device: Device,
    pub vk: Option<u32>,
    pub scan: Option<u32>,
    pub flags: u32,
    pub key_name: Option<String>,
    pub sample_id: Option<u64>,
    pub offset_us: Option<i64>,
    pub seq: u64,
    pub utc_us: u64,
    pub mono_us: u64,
    pub kind: u32,
    pub point: Option<[i32; 2]>,
    pub injected: bool,
    pub wheel: i16,
    pub media_us: Option<u64>,
    pub gif_frame: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Data {
    pub capture_fps: Option<u32>,
    pub schema: u32,
    pub asset_id: Option<String>,
    pub gif_sha256: Option<String>,
    pub session_id: String,
    pub region: Region,
    pub width: u32,
    pub height: u32,
    pub gif_bytes: u64,
    pub duration_ms: u64,
    pub frames: u64,
    pub samples: Vec<Sample>,
    pub events: Vec<Event>,
    pub dropped_events: u64,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    #[default]
    Mouse,
    Keyboard,
    Marker,
}
