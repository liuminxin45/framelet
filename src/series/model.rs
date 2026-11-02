use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MAX_PARTS: usize = 4096;
pub const INDEX_LIMIT: u64 = 8 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
pub struct Part {
    #[serde(default)]
    pub observation_range: Option<[u64; 2]>,
    #[serde(default)]
    pub utc_range: Option<[u64; 2]>,
    pub number: u32,
    pub start_ms: u64,
    pub reason: String,
    pub recording: crate::encode::Stats,
}
impl Part {
    pub fn new(
        number: u32,
        start_ms: u64,
        reason: String,
        recording: crate::encode::Stats,
        data: &crate::timeline::Data,
    ) -> Self {
        let mut ids = data
            .samples
            .iter()
            .filter_map(|s| s.id)
            .chain(data.events.iter().filter_map(|e| e.id));
        let observation_range = ids
            .next()
            .map(|first| ids.fold([first, first], |r, id| [r[0].min(id), r[1].max(id)]));
        let utc_range = data
            .samples
            .iter()
            .map(|s| {
                [
                    s.utc_us,
                    s.utc_us
                        .saturating_add(s.hold_end_us.saturating_sub(s.capture_us)),
                ]
            })
            .chain(data.events.iter().map(|e| [e.utc_us, e.utc_us]))
            .reduce(|a, b| [a[0].min(b[0]), a[1].max(b[1])]);
        Self {
            number,
            start_ms,
            reason,
            recording,
            observation_range,
            utc_range,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Index {
    pub schema: u32,
    pub session_id: String,
    pub prefix: String,
    pub state: String,
    pub reason: String,
    pub owner_pid: u32,
    pub owner_created: u64,
    pub segment_seconds: u16,
    pub request_id: Option<String>,
    pub request: super::SeriesArgs,
    pub parts: Vec<Part>,
    pub failure: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Plan {
    pub directory: PathBuf,
    pub index: Index,
}
#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    pub gif_prefix_sha256: String,
    pub schema: u32,
    pub session_id: String,
    pub number: u32,
    pub start_ms: u64,
    pub source: String,
    pub target: String,
    pub safe_bytes: u64,
    pub recording: crate::encode::Stats,
    pub timeline: crate::timeline::Data,
}
