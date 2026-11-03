use crate::capture::Region;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Parser, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordArgs {
    #[arg(long, allow_hyphen_values = true)]
    #[serde(default)]
    pub region: Option<Region>,
    #[arg(long, default_value_t = 30)]
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[arg(long, default_value_t = 60.0)]
    #[serde(default = "default_duration")]
    pub duration: f64,
    #[arg(short, long)]
    pub output: PathBuf,
    #[arg(long)]
    #[serde(default)]
    pub cursor: bool,
    /// 1 = best color quantization, 30 = fastest.
    #[arg(long, default_value_t = 10)]
    #[serde(default = "default_quality")]
    pub quality: i32,
    /// Named stop handle, e.g. demo. Stop with `framelet stop --session demo`.
    #[arg(long)]
    #[serde(default)]
    pub session: Option<String>,
    /// Emit JSON Lines progress on stderr; final result is always JSON on stdout.
    #[arg(long)]
    #[serde(default)]
    pub events: bool,
    /// Scale down to this width, preserving aspect ratio. Never upscale.
    #[arg(long)]
    #[serde(default)]
    pub max_width: Option<u32>,
    /// Solid privacy rectangles, relative to the capture region: x,y,width,height. Repeatable.
    #[arg(long = "mask", allow_hyphen_values = true)]
    #[serde(default)]
    pub masks: Vec<Region>,
    /// Countdown before capture, in seconds (0..30).
    #[arg(long, default_value_t = 0.0)]
    #[serde(default)]
    pub delay: f64,
    /// Maximum GIF bytes in MiB; exceeding the budget discards unfinished output.
    #[arg(long, default_value_t = 256)]
    #[serde(default = "default_max_mib")]
    pub max_mib: u64,
}
fn default_max_mib() -> u64 {
    256
}
fn default_fps() -> u32 {
    30
}
fn default_duration() -> f64 {
    60.0
}
fn default_quality() -> i32 {
    10
}
