//! One request sees one index and validates each selected immutable part once.
use super::{model::Index, store};
use crate::file_guard::Lease;
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub struct ReviewSnapshot {
    pub root: PathBuf,
    pub index: Index,
    _root: Lease,
    verified: BTreeMap<PathBuf, Vec<Lease>>,
}
impl ReviewSnapshot {
    pub fn new(input: &Path) -> Result<Self> {
        let root = store::directory(input)?;
        // Pin the directory, not recording.json: active recording may publish a new index.
        let lease = Lease::output(&root.join(".framelet-review"))?;
        let index = store::read_index(&root)?;
        Ok(Self {
            root,
            index,
            _root: lease,
            verified: BTreeMap::new(),
        })
    }
    pub fn locate(&mut self, seconds: f64) -> Result<(PathBuf, f64, u64)> {
        ensure!(
            seconds.is_finite() && (0.0..=86400.0).contains(&seconds),
            "global at must be 0..86400"
        );
        let at = (seconds * 1000.0).floor() as u64;
        let part = self
            .index
            .parts
            .get(
                self.index
                    .parts
                    .partition_point(|p| p.start_ms + p.recording.duration_ms <= at),
            )
            .filter(|p| p.start_ms <= at)
            .context("timestamp outside committed segments")?;
        let start = part.start_ms;
        let file = self.root.join(&part.recording.output);
        if !self.verified.contains_key(&file) {
            ensure!(
                self.verified.len() < 64,
                "review exceeds 64 selected segments"
            );
            let mut leases = vec![Lease::input(&file)?];
            for sidecar in [
                crate::timeline::sidecar_path(&file),
                crate::transaction::manifest(&file),
            ] {
                if sidecar.try_exists()? {
                    leases.push(Lease::input(&sidecar)?);
                }
            }
            crate::transaction::verify(&file)?;
            self.verified.insert(file.clone(), leases);
        }
        Ok((file, seconds - start as f64 / 1000.0, start))
    }
    pub fn verified_segments(&self) -> usize {
        self.verified.len()
    }
}
