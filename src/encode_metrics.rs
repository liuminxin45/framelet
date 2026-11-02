use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub target_fps: Option<u32>,
    pub source_samples: u64,
    pub samples_per_media_second: Option<f64>,
    pub capture_us: u64,
    pub transform_us: u64,
    pub encode_pipeline_us: u64,
    pub quantize_us: u64,
    pub lzw_write_us: u64,
    pub finalize_us: u64,
    pub missed_full_periods: u64,
    pub exact_palette_frames: u64,
    pub quantized_frames: u64,
    pub merged_samples: u64,
    pub checkpoint_snapshots: u64,
}
pub fn micros(elapsed: std::time::Duration) -> u64 {
    elapsed.as_micros().min(u64::MAX as u128) as u64
}
