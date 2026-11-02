//! Bounded candidate thumbnails collected during the analysis pass, never full RGB history.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

const INTERIOR_LIMIT: usize = 30; // Plus opening and closing contexts: <=32 distinct images.
pub const PIXEL_BUDGET: usize = 32 * 240 * 180 * 3;

#[derive(Clone, Copy)]
pub struct SourceFrame {
    pub index: usize,
    pub at: u64,
    pub delay: u64,
    pub width: u32,
    pub height: u32,
}
struct Thumbnail {
    source: SourceFrame,
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    score: f64,
}
#[derive(Default)]
pub struct Preview {
    first: Option<Arc<Thumbnail>>,
    last: Option<Arc<Thumbnail>>,
    tail: Option<(SourceFrame, f64)>,
    candidates: Vec<Arc<Thumbnail>>,
    peak_bytes: usize,
}
impl Preview {
    fn thumbnail(source: SourceFrame, rgb: &[u8], score: f64) -> Arc<Thumbnail> {
        let scale = (240. / source.width as f64)
            .min(180. / source.height as f64)
            .min(1.);
        let width = (source.width as f64 * scale).round().max(1.) as u32;
        let height = (source.height as f64 * scale).round().max(1.) as u32;
        let pixels = if (width, height) == (source.width, source.height) {
            rgb.to_vec()
        } else {
            crate::frames::resize(rgb, source.width, source.height, width, height)
        };
        Arc::new(Thumbnail {
            source,
            width,
            height,
            rgb: pixels,
            score,
        })
    }
    pub fn observe(&mut self, source: SourceFrame, rgb: &[u8], score: f64, changed: bool) {
        self.tail = Some((source, score));
        if self.first.is_none() {
            self.first = Some(Self::thumbnail(source, rgb, score));
            self.peak_bytes = self.retained_bytes();
            return;
        }
        if !changed {
            return;
        }
        let incoming = self.candidates.len();
        if incoming == INTERIOR_LIMIT {
            // Keep the strongest change. Thin the densest chronological neighbourhood
            // first. Decide before resizing: rejected candidates allocate no image.
            let at = |i: usize| self.candidates.get(i).map_or(source.at, |v| v.source.at);
            let change = |i: usize| self.candidates.get(i).map_or(score, |v| v.score);
            let peak = (0..=incoming)
                .max_by(|&a, &b| change(a).total_cmp(&change(b)).then(b.cmp(&a)))
                .unwrap_or(0);
            let span = |i: usize| {
                let before = if i == 0 {
                    self.first.as_ref().map_or(0, |v| v.source.at)
                } else {
                    at(i - 1)
                };
                let after = at(i + 1);
                after.saturating_sub(before)
            };
            let remove = (0..=incoming)
                .filter(|&i| i != peak)
                .min_by(|&a, &b| {
                    span(a)
                        .cmp(&span(b))
                        .then_with(|| change(a).total_cmp(&change(b)))
                        .then(a.cmp(&b))
                })
                .expect("more than 30 candidates leaves a non-peak removal");
            if remove == incoming {
                return;
            }
            self.candidates.remove(remove);
        }
        self.candidates.push(Self::thumbnail(source, rgb, score));
        self.peak_bytes = self.peak_bytes.max(self.retained_bytes());
    }
    pub fn finish(&mut self, last_rgb: &[u8]) -> Result<()> {
        let (source, score) = self.tail.context("preview tail missing")?;
        ensure!(
            last_rgb.len() == source.width as usize * source.height as usize * 3,
            "preview tail dimensions mismatch"
        );
        // Visual already owns the last RGB for change comparison; borrow it once
        // instead of regenerating a closing thumbnail on every frame.
        self.last = self
            .first
            .iter()
            .chain(self.candidates.iter())
            .find(|t| t.source.index == source.index)
            .cloned()
            .or_else(|| Some(Self::thumbnail(source, last_rgb, score)));
        self.peak_bytes = self.peak_bytes.max(self.retained_bytes());
        Ok(())
    }
    fn retained_bytes(&self) -> usize {
        let mut seen = BTreeSet::new();
        self.first
            .iter()
            .chain(self.last.iter())
            .chain(self.candidates.iter())
            .filter(|t| seen.insert(t.source.index))
            .map(|t| t.rgb.len())
            .sum()
    }
    fn selected(&self, count: usize) -> Vec<Arc<Thumbnail>> {
        let unique: BTreeMap<_, _> = self
            .first
            .iter()
            .chain(self.candidates.iter())
            .chain(self.last.iter())
            .map(|t| (t.source.index, t.clone()))
            .collect();
        let all: Vec<_> = unique.into_values().collect();
        if all.len() <= count {
            return all;
        }
        let peak = (0..all.len())
            .max_by(|&a, &b| all[a].score.total_cmp(&all[b].score).then(b.cmp(&a)))
            .unwrap_or(0);
        if count == 1 {
            return vec![all[peak].clone()];
        }
        let mut selected = vec![0, all.len() - 1];
        if count > 2 && !selected.contains(&peak) {
            selected.push(peak);
        }
        while selected.len() < count {
            let next = (0..all.len())
                .filter(|i| !selected.contains(i))
                .max_by(|&a, &b| {
                    let distance = |i: usize| {
                        selected
                            .iter()
                            .map(|&j| all[i].source.at.abs_diff(all[j].source.at))
                            .min()
                            .unwrap_or(0)
                    };
                    distance(a)
                        .cmp(&distance(b))
                        .then_with(|| all[a].score.total_cmp(&all[b].score))
                        .then(b.cmp(&a))
                });
            let Some(next) = next else {
                break;
            };
            selected.push(next);
        }
        selected.sort_unstable();
        selected.into_iter().map(|i| all[i].clone()).collect()
    }
    pub fn export(&self, input: &Path, output: &Path, count: usize) -> Result<Value> {
        ensure!(
            (1..=16).contains(&count) && self.peak_bytes <= PIXEL_BUDGET,
            "preview count/pixel budget exceeded"
        );
        let frames = self.selected(count);
        ensure!(!frames.is_empty(), "no frames overlap the analysis window");
        let input = input.canonicalize()?;
        let stage = crate::storage::StagedDirectory::new(output)?;
        let cols = frames.len().min(4);
        let rows = frames.len().div_ceil(cols);
        let (width, height) = (cols * 256, rows * 212);
        let mut canvas = vec![28u8; width * height * 3];
        let first = self
            .first
            .as_ref()
            .context("preview opening missing")?
            .source
            .index;
        let last = self
            .last
            .as_ref()
            .context("preview closing missing")?
            .source
            .index;
        let mut metadata = Vec::new();
        for (i, thumb) in frames.iter().enumerate() {
            let (x, y) = (i % cols * 256 + 8, i / cols * 212 + 24);
            for row in 0..thumb.height as usize {
                let dst = ((y + row) * width + x) * 3;
                let src = row * thumb.width as usize * 3;
                canvas[dst..dst + thumb.width as usize * 3]
                    .copy_from_slice(&thumb.rgb[src..src + thumb.width as usize * 3]);
            }
            crate::frames::label(&mut canvas, width, x, y - 18, i + 1);
            let s = thumb.source;
            let reason = match (s.index == first, s.index == last) {
                (true, true) => "opening_and_closing_context",
                (true, false) => "opening_context",
                (false, true) => "closing_context",
                _ => "change_and_time_coverage",
            };
            metadata.push(json!({"input":input,"request_index":i,"frame_index":s.index,"requested_ms":s.at,"timestamp_ms":s.at,"duration_ms":s.delay,"width":s.width,"height":s.height,"thumbnail_width":thumb.width,"thumbnail_height":thumb.height,"sheet_tile":i+1,"spatial_change":thumb.score,"reason":reason}));
        }
        let work = json!({"decoded_frames":0,"converted_frames":0,"decode_passes":0,"peak_thumbnail_bytes":self.peak_bytes,"thumbnail_pixel_budget":PIXEL_BUDGET,"selected_frames":frames.len()});
        let result = json!({"frames":metadata,"work":work,"selection":"opening/closing context + visual change and temporal coverage; not semantic highlights","contact_sheet":output.join("contact-sheet.png")});
        crate::encode::screenshot(
            &stage.path.join("contact-sheet.png"),
            width as u32,
            height as u32,
            &canvas,
        )?;
        crate::storage::replace_json(&stage.path.join("manifest.json"), &result)?;
        stage.publish()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_long_motion_keeps_context_and_strongest_transient() {
        let mut preview = Preview::default();
        let pixels = vec![40; 32 * 24 * 3];
        for index in 0..1000 {
            preview.observe(
                SourceFrame {
                    index,
                    at: index as u64 * 10,
                    delay: 10,
                    width: 32,
                    height: 24,
                },
                &pixels,
                if index == 501 { 1. } else { 0.1 },
                index > 0,
            );
            assert!(preview.retained_bytes() <= 32 * 32 * 24 * 3);
        }
        preview.finish(&pixels).unwrap();
        assert!(preview.peak_bytes <= 32 * 32 * 24 * 3);
        let frames = preview.selected(6);
        assert_eq!(frames.len(), 6);
        assert_eq!(frames.first().unwrap().source.index, 0);
        assert_eq!(frames.last().unwrap().source.index, 999);
        assert!(frames.iter().any(|t| t.source.index == 501));
        assert!(
            frames
                .windows(2)
                .all(|p| p[0].source.index < p[1].source.index)
        );
        assert_eq!(preview.selected(1)[0].source.index, 501);
    }
}
