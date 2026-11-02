use crate::capture::Region;
use serde::{Deserialize, Serialize};
pub(super) const MAX_EVENTS: usize = 100_000;
pub(super) const MAX_SAMPLES: usize = 200_000;
#[derive(Clone, Serialize, Deserialize)]
pub struct Sample {
    #[serde(default)]
    pub id: Option<u64>,
    pub seq: u64,
    pub utc_us: u64,
    pub capture_us: u64,
    pub capture_end_us: u64,
    #[serde(default)]
    pub coverage_start_us: u64,
    pub hold_end_us: u64,
    pub media_start_us: u64,
    pub media_end_us: u64,
    pub gif_frame: u64,
    pub cursor: Option<[i32; 2]>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub device: Device,
    #[serde(default)]
    pub vk: Option<u32>,
    #[serde(default)]
    pub scan: Option<u32>,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub key_name: Option<String>,
    #[serde(default)]
    pub sample_id: Option<u64>,
    #[serde(default)]
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
pub struct Data {
    pub schema: u32,
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
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

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    #[default]
    Mouse,
    Keyboard,
}
