use anyhow::{Result, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Args, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SeriesArgs {
    /// Existing directory in which a uniquely named .framelet group is created.
    #[arg(long)]
    pub output_dir: PathBuf,
    #[arg(long, default_value = "recording")]
    pub name: String,
    /// Total active recording seconds, not the per-GIF limit.
    #[arg(long, conflicts_with = "until_stop")]
    pub duration: Option<f64>,
    #[arg(long, conflicts_with = "duration")]
    pub until_stop: bool,
    #[arg(long, default_value_t = 300)]
    pub segment_seconds: u16,
    #[arg(long, default_value_t = 28800)]
    pub max_wall_seconds: u32,
    #[arg(long, default_value_t = 2048)]
    pub total_mib: u64,
    #[arg(long, default_value_t = 256)]
    pub max_mib: u64,
    #[arg(long, allow_hyphen_values = true)]
    pub region: Option<crate::capture::Region>,
    #[arg(long, default_value_t = 30)]
    pub fps: u32,
    #[arg(long)]
    pub cursor: bool,
    #[arg(long, default_value_t = 10)]
    pub quality: i32,
    #[arg(long)]
    pub max_width: Option<u32>,
    #[arg(long = "mask", allow_hyphen_values = true)]
    pub masks: Vec<crate::capture::Region>,
    #[arg(long, default_value_t = 0.0)]
    pub delay: f64,
    #[arg(long)]
    pub events: bool,
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long)]
    pub request_id: Option<String>,
}
impl Default for SeriesArgs {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::new(),
            name: "recording".into(),
            duration: None,
            until_stop: false,
            segment_seconds: 300,
            max_wall_seconds: 28800,
            total_mib: 2048,
            max_mib: 256,
            region: None,
            fps: 30,
            cursor: false,
            quality: 10,
            max_width: None,
            masks: vec![],
            delay: 0.0,
            events: false,
            session: None,
            request_id: None,
        }
    }
}
impl SeriesArgs {
    pub fn capture(&self, output: &Path) -> crate::recording::RecordArgs {
        crate::recording::RecordArgs {
            region: self.region,
            fps: self.fps,
            duration: 1.0,
            output: output.into(),
            cursor: self.cursor,
            quality: self.quality,
            session: None,
            events: false,
            max_width: self.max_width,
            masks: self.masks.clone(),
            delay: self.delay,
            max_mib: self.max_mib,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.output_dir.is_dir(),
            "output_dir must be an existing directory"
        );
        ensure!(
            !self.name.is_empty()
                && self.name.len() <= 48
                && !self.name.ends_with([' ', '.'])
                && self
                    .name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
            "name must be 1..48 UTF-8 bytes of letters, digits, hyphens or underscores"
        );
        let upper = self.name.to_ascii_uppercase();
        ensure!(
            !["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
                && !(1..=9).any(|n| upper == format!("COM{n}") || upper == format!("LPT{n}")),
            "reserved Windows name"
        );
        ensure!(
            self.duration.is_some() ^ self.until_stop,
            "choose duration OR until_stop:true"
        );
        ensure!(
            (1..=300).contains(&self.segment_seconds),
            "segment_seconds must be 1..300"
        );
        ensure!(
            (1..=86400).contains(&self.max_wall_seconds),
            "max_wall_seconds must be 1..86400"
        );
        ensure!(
            self.duration
                .is_none_or(|d| d.is_finite() && d >= 0.01 && d <= self.max_wall_seconds as f64),
            "duration must be finite, >=0.01 and <=max_wall_seconds"
        );
        ensure!(
            (1..=256).contains(&self.max_mib)
                && self.total_mib >= self.max_mib
                && self.total_mib <= 1_048_576,
            "require 1<=max_mib<=256 and max_mib<=total_mib<=1048576"
        );
        ensure!(
            self.request_id.as_ref().is_none_or(|s| !s.is_empty()
                && s.len() <= 80
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')),
            "request_id must be 1..80 ASCII letters, digits, hyphens or underscores"
        );
        crate::recording::validate(
            &self.capture(
                &self
                    .output_dir
                    .join(format!(".probe-{}.gif", crate::identity::new_id()?)),
            ),
        )?;
        Ok(())
    }
}
