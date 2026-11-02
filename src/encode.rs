use anyhow::{Context, Result, ensure};
use gif::{DisposalMethod, Encoder, Frame, Repeat};
use serde::Serialize;
use std::{
    fs::{File, OpenOptions},
    io::BufWriter,
    path::{Path, PathBuf},
};

// Single pending frame: stationary time is merged without retaining a recording in RAM.
pub struct GifWriter {
    enc: Option<Encoder<BufWriter<File>>>,
    previous: Vec<u8>,
    pending: Option<Frame<'static>>,
    width: u16,
    height: u16,
    quality: i32,
    pub frames: u64,
    pub samples: u64,
    pub duration_cs: u64,
    path: PathBuf,
    partial: PathBuf,
    committed: bool,
    limit: u64,
    timeline: Option<crate::timeline::Recorder>,
    _lease: crate::file_guard::Lease,
    _work_lease: Option<crate::file_guard::Lease>,
    _pending: Option<crate::transaction::Pending>,
    preserve_partial: bool,
}
impl GifWriter {
    pub fn new(path: &Path, width: u32, height: u32, quality: i32) -> Result<Self> {
        Self::new_in(path, width, height, quality, None)
    }
    pub fn new_in(
        path: &Path,
        width: u32,
        height: u32,
        quality: i32,
        work: Option<&Path>,
    ) -> Result<Self> {
        ensure!(
            width > 0 && height > 0 && width <= 8192 && height <= 8192,
            "invalid GIF dimensions"
        );
        ensure!(
            (1..=30).contains(&quality),
            "quality must be 1 (best) to 30 (fastest)"
        );
        ensure!(!path.exists(), "output already exists: {}", path.display());
        let path = std::path::absolute(path)?;
        let lease = crate::file_guard::Lease::output(&path)?;
        let partial_name = format!(
            "{}.{}-{}.partial",
            path.file_name()
                .context("output requires a filename")?
                .to_string_lossy(),
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        let partial = work.map_or_else(
            || path.with_file_name(&partial_name),
            |p| p.join(&partial_name),
        );
        let work_lease = work
            .map(|_| crate::file_guard::Lease::output(&partial))
            .transpose()?;
        use std::os::windows::fs::OpenOptionsExt;
        let f = OpenOptions::new()
            .write(true)
            .access_mode(
                windows_sys::Win32::Foundation::GENERIC_READ
                    | windows_sys::Win32::Foundation::GENERIC_WRITE
                    | windows_sys::Win32::Storage::FileSystem::DELETE,
            )
            .create_new(true)
            .open(&partial)
            .context("create output (parent directory must exist)")?;
        let initialized = (|| -> Result<_> {
            let mut enc = Encoder::new(BufWriter::new(f), width as u16, height as u16, &[])?;
            enc.set_repeat(Repeat::Infinite)?;
            Ok(enc)
        })();
        let enc = match initialized {
            Ok(enc) => enc,
            Err(e) => {
                let _ = std::fs::remove_file(&partial);
                return Err(e);
            }
        };
        Ok(Self {
            enc: Some(enc),
            previous: Vec::new(),
            pending: None,
            width: width as u16,
            height: height as u16,
            quality,
            frames: 0,
            samples: 0,
            duration_cs: 0,
            path,
            partial,
            committed: false,
            limit: 256 * 1024 * 1024,
            timeline: None,
            _lease: lease,
            _work_lease: work_lease,
            _pending: None,
            preserve_partial: work.is_some(),
        })
    }
    pub fn track(
        &mut self,
        region: crate::capture::Region,
        masks: &[crate::capture::Region],
    ) -> Result<()> {
        crate::timeline::available(&self.path)?;
        self._pending = Some(crate::transaction::begin(&self.path)?);
        self.timeline = Some(crate::timeline::Recorder::new(
            region,
            self.width as u32,
            self.height as u32,
            masks,
        )?);
        Ok(())
    }
    pub fn capture(&mut self, grab: &crate::capture::Grabber) -> Result<Vec<u8>> {
        let at = self.timeline.as_ref().map(|t| t.begin());
        let pixels = grab.rgb()?;
        if let (Some(t), Some(at)) = (self.timeline.as_mut(), at) {
            t.captured(at);
        }
        Ok(pixels)
    }
    fn mapped(&mut self, delay: u16) {
        if let Some(t) = &mut self.timeline {
            t.pushed(
                (self.duration_cs - u64::from(delay)) * 10_000,
                self.duration_cs * 10_000,
                self.frames,
            );
        }
    }
    pub fn set_limit(&mut self, bytes: u64) {
        self.limit = bytes;
    }
    pub fn partial_path(&self) -> &Path {
        &self.partial
    }
    pub fn snapshot_stats(&self, bytes: u64) -> Stats {
        Stats {
            output: self.path.clone(),
            width: self.width,
            height: self.height,
            frames: self.frames,
            samples: self.samples,
            duration_ms: self.duration_cs * 10,
            bytes,
            timeline: None,
            manifest: None,
            warnings: vec![],
        }
    }
    pub fn checkpoint(&mut self) -> Result<u64> {
        use std::io::Write;
        self.flush()?;
        let out = self
            .enc
            .as_mut()
            .context("GIF encoder is closed")?
            .get_mut();
        out.flush()?;
        out.get_ref().sync_all()?;
        let bytes = out.get_ref().metadata()?.len();
        ensure!(bytes <= self.limit, "GIF exceeds output byte budget");
        Ok(bytes)
    }
    fn flush(&mut self) -> Result<()> {
        if let Some(frame) = self.pending.take() {
            self.enc
                .as_mut()
                .context("GIF encoder is closed")?
                .write_frame(&frame)?;
            ensure!(
                std::fs::metadata(&self.partial)?.len() <= self.limit,
                "GIF exceeds output byte budget; unfinished output discarded"
            );
            self.frames += 1;
        }
        Ok(())
    }
    pub fn push(&mut self, rgb: Vec<u8>, delay: u16) -> Result<()> {
        ensure!(
            rgb.len() == self.width as usize * self.height as usize * 3,
            "pixel buffer length mismatch"
        );
        ensure!(delay > 0, "GIF delay must be positive");
        self.samples += 1;
        self.duration_cs += u64::from(delay);
        if rgb == self.previous
            && self
                .pending
                .as_ref()
                .is_some_and(|f| u32::from(f.delay) + u32::from(delay) <= 65535)
        {
            self.pending
                .as_mut()
                .context("missing pending GIF frame")?
                .delay += delay;
            self.mapped(delay);
            return Ok(());
        }
        let (x, y, w, h) = bounds(
            &self.previous,
            &rgb,
            self.width as usize,
            self.height as usize,
        );
        self.flush()?;
        let mut crop = Vec::with_capacity(w * h * 3);
        for row in y..y + h {
            crop.extend_from_slice(
                &rgb[(row * self.width as usize + x) * 3..(row * self.width as usize + x + w) * 3],
            );
        }
        let mut frame = Frame::from_rgb_speed(w as u16, h as u16, &crop, self.quality);
        frame.left = x as u16;
        frame.top = y as u16;
        frame.delay = delay;
        frame.dispose = DisposalMethod::Keep;
        self.pending = Some(frame);
        self.previous = rgb;
        self.mapped(delay);
        Ok(())
    }
    pub fn finish(mut self) -> Result<Stats> {
        ensure!(self.samples > 0, "recording contains no frames");
        self.flush()?;
        let enc = self.enc.take().context("GIF encoder already finalized")?;
        let mut writer = enc.into_inner()?;
        use std::io::Write;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        ensure!(
            std::fs::metadata(&self.partial)?.len() <= self.limit,
            "GIF exceeds output byte budget; unfinished output discarded"
        );
        // Hard-link publishes without replacing an existing user file, even under races.
        crate::storage::publish_file(writer.get_ref(), &self.path)?;
        self.committed = true;
        drop(writer);
        let cleanup_warnings = Vec::new();
        let mut stats = Stats {
            output: self.path.clone(),
            width: self.width,
            height: self.height,
            frames: self.frames,
            samples: self.samples,
            duration_ms: self.duration_cs * 10,
            bytes: std::fs::metadata(&self.path)?.len(),
            timeline: None,
            manifest: None,
            warnings: cleanup_warnings,
        };
        if let Some(t) = self.timeline.take() {
            match crate::runtime::guard("timeline.finalize", || Ok(t.finish(&stats))) {
                Ok(data) => {
                    stats.warnings.extend(data.warnings.iter().cloned());
                    if data.dropped_events > 0 {
                        stats
                            .warnings
                            .push(format!("{} mouse events dropped", data.dropped_events));
                    }
                    match crate::timeline::publish(&self.path, &data) {
                        Ok(p) => stats.timeline = Some(p),
                        Err(e) => {
                            crate::runtime::report("timeline.publish", &e);
                            stats
                                .warnings
                                .push(format!("GIF saved; timeline publication failed: {e}"));
                        }
                    }
                }
                Err(e) => {
                    crate::runtime::report("timeline.finalize", &e);
                    stats
                        .warnings
                        .push("GIF saved; timeline finalization failed".into());
                }
            }
        }
        if self._pending.is_some() && stats.timeline.is_some() {
            match crate::transaction::complete(&self.path) {
                Ok(p) => stats.manifest = Some(p),
                Err(e) => {
                    crate::runtime::report("transaction.complete", &e);
                    stats
                        .warnings
                        .push(format!("GIF saved; transaction incomplete: {e}"));
                }
            }
        }
        Ok(stats)
    }
}
impl Drop for GifWriter {
    fn drop(&mut self) {
        if !self.committed {
            self.enc.take();
            if !self.preserve_partial || self.samples == 0 {
                let _ = std::fs::remove_file(&self.partial);
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Stats {
    pub output: PathBuf,
    pub width: u16,
    pub height: u16,
    pub frames: u64,
    pub samples: u64,
    pub duration_ms: u64,
    pub bytes: u64,
    pub timeline: Option<PathBuf>,
    #[serde(default)]
    pub manifest: Option<PathBuf>,
    pub warnings: Vec<String>,
}
fn bounds(a: &[u8], b: &[u8], w: usize, h: usize) -> (usize, usize, usize, usize) {
    if a.len() != b.len() {
        return (0, 0, w, h);
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 3;
            if a[i..i + 3] != b[i..i + 3] {
                minx = minx.min(x);
                maxx = maxx.max(x);
                miny = miny.min(y);
                maxy = maxy.max(y);
            }
        }
    }
    if minx == w {
        (0, 0, 1, 1)
    } else {
        (minx, miny, maxx - minx + 1, maxy - miny + 1)
    }
}

pub fn screenshot(path: &Path, width: u32, height: u32, rgb: &[u8]) -> Result<serde_json::Value> {
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 16_777_216,
        "PNG exceeds 16 MP budget"
    );
    ensure!(
        rgb.len() == width as usize * height as usize * 3,
        "pixel buffer length mismatch"
    );
    let path = std::path::absolute(path)?;
    ensure!(!path.exists(), "output already exists");
    let mut stage = crate::storage::StagedFile::new(&path)?;
    {
        let mut enc = png::Encoder::new(&mut stage.file, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header()?;
        writer.write_image_data(rgb)?;
        writer.finish()?;
    }
    let warnings = stage.publish(&path)?;
    Ok(
        serde_json::json!({"output":path,"width":width,"height":height,"bytes":std::fs::metadata(&path)?.len(),"warnings":warnings}),
    )
}

pub fn edit(
    input: &Path,
    output: &Path,
    start: f64,
    end: Option<f64>,
    speed: f64,
) -> Result<Stats> {
    ensure!(
        speed.is_finite()
            && (0.01..=100.0).contains(&speed)
            && start.is_finite()
            && start >= 0.0
            && end.is_none_or(|e| e.is_finite() && e > start),
        "invalid trim range or speed"
    );
    ensure!(
        !output.exists(),
        "output already exists: {}",
        output.display()
    );
    let source_timeline = crate::timeline::load(input)?;
    let _pending = if source_timeline.is_some() {
        Some(crate::transaction::begin(output)?)
    } else {
        None
    };
    if source_timeline.is_some() {
        crate::timeline::available(output)?;
    }
    let mut spans = Vec::new();
    let mut writer: Option<GifWriter> = None;
    let mut timing_error = 0.0;
    let mut output_cs = 0u64;
    crate::frames::decode(
        |_, at, delay_ms, w, h, rgb| {
            let time = at as f64 / 1000.0;
            let next = (at + delay_ms) as f64 / 1000.0;
            let included = next.min(end.unwrap_or(f64::INFINITY)) - time.max(start);
            if included > 0.0 {
                let desired = included / speed * 100.0 + timing_error;
                let mut delay = desired.round().max(1.0) as u64;
                timing_error = desired - delay as f64;
                output_cs += delay;
                ensure!(
                    output_cs <= 360_000,
                    "edited GIF exceeds 1 hour duration budget"
                );
                if writer.is_none() {
                    writer = Some(GifWriter::new(output, w, h, 10)?);
                }
                while delay > 0 {
                    let part = delay.min(65535);
                    let w = writer.as_mut().context("GIF editor not initialized")?;
                    let out_start = w.duration_cs * 10_000;
                    w.push(rgb.to_vec(), part as u16)?;
                    // Split long holds while preserving the exact rounded output duration.
                    let src_start = (time.max(start) * 1e6).round() as u64;
                    let src_end = (next.min(end.unwrap_or(f64::INFINITY)) * 1e6).round() as u64;
                    let total = desired.round().max(1.0) as u64;
                    let consumed = total - delay;
                    spans.push((
                        src_start + (src_end - src_start) * consumed / total,
                        src_start + (src_end - src_start) * (consumed + part) / total,
                        out_start,
                        w.duration_cs * 10_000,
                        w.frames,
                    ));
                    delay -= part;
                }
            }
            Ok(end.is_none_or(|e| next < e))
        },
        input,
    )?;
    let mut stats = writer.context("trim range contains no frames")?.finish()?;
    if let Some(data) = source_timeline {
        let data = crate::timeline::remap(data, &spans, &stats)?;
        stats.warnings.extend(data.warnings.iter().cloned());
        match crate::timeline::publish(output, &data) {
            Ok(p) => stats.timeline = Some(p),
            Err(e) => {
                crate::runtime::report("timeline.edit.publish", &e);
                stats
                    .warnings
                    .push(format!("GIF saved; timeline publication failed: {e}"));
            }
        }
    }
    if stats.timeline.is_some() {
        match crate::transaction::complete(output) {
            Ok(p) => stats.manifest = Some(p),
            Err(e) => {
                crate::runtime::report("transaction.edit", &e);
                stats
                    .warnings
                    .push(format!("edit saved; transaction incomplete: {e}"));
            }
        }
    }
    Ok(stats)
}
pub fn inspect(input: &Path) -> Result<serde_json::Value> {
    Ok(inventory(input)?.0)
}
pub(crate) fn inventory(input: &Path) -> Result<(serde_json::Value, Vec<(u64, u64)>)> {
    let _lease = crate::file_guard::Lease::input(input)?;
    ensure!(
        std::fs::metadata(input)?.len() <= 512 * 1024 * 1024,
        "GIF exceeds 512 MiB input budget"
    );
    let started = std::time::Instant::now();
    let mut options = gif::DecodeOptions::new();
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        std::num::NonZeroU64::new(16_777_216 * 4).context("invalid decoder limit")?,
    ));
    let mut reader = options.read_info(File::open(input)?)?;
    let (w, h) = (reader.width(), reader.height());
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 16_777_216,
        "GIF exceeds 16 MP canvas budget"
    );
    let (mut frames, mut cs) = (0u64, 0u64);
    let mut intervals = Vec::new();
    while let Some(f) = reader.read_next_frame()? {
        frames += 1;
        ensure!(
            frames <= 100_000
                && frames * u64::from(w) * u64::from(h) <= 2_000_000_000
                && started.elapsed().as_secs() < 60,
            "GIF exceeds processing budget"
        );
        ensure!(
            u32::from(f.left) + u32::from(f.width) <= u32::from(w)
                && u32::from(f.top) + u32::from(f.height) <= u32::from(h),
            "GIF frame outside canvas"
        );
        let start = cs * 10;
        cs += u64::from(f.delay.max(1));
        intervals.push((start, cs * 10));
    }
    ensure!(frames > 0, "GIF contains no frames");
    Ok((
        serde_json::json!({"input":std::path::absolute(input)?,"width":w,"height":h,"frames":frames,"duration_ms":cs*10,"bytes":std::fs::metadata(input)?.len()}),
        intervals,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("framelet-{}-{name}.gif", std::process::id()))
    }
    #[test]
    fn delta_merge_and_trim() {
        let p = path("delta");
        let q = path("trim");
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&q);
        let mut w = GifWriter::new(&p, 8, 8, 10).unwrap();
        let a = vec![0; 8 * 8 * 3];
        let mut b = a.clone();
        b[3 * 9] = 255;
        w.push(a.clone(), 10).unwrap();
        w.push(a, 10).unwrap();
        w.push(b, 10).unwrap();
        let s = w.finish().unwrap();
        assert_eq!(s.frames, 2);
        assert_eq!(s.duration_ms, 300);
        assert!(GifWriter::new(&p, 8, 8, 10).is_err());
        let mut r = gif::DecodeOptions::new()
            .read_info(File::open(&p).unwrap())
            .unwrap();
        assert_eq!(r.read_next_frame().unwrap().unwrap().delay, 20);
        let f = r.read_next_frame().unwrap().unwrap();
        assert_eq!((f.left, f.top, f.width, f.height), (1, 1, 1, 1));
        let s = edit(&p, &q, 0.1, Some(0.3), 2.0).unwrap();
        assert_eq!(s.duration_ms, 100);
        std::fs::remove_file(p).unwrap();
        std::fs::remove_file(q).unwrap();
    }
    #[test]
    fn invalid_options_and_empty_output() {
        let p = path("empty");
        let _ = std::fs::remove_file(&p);
        assert!(GifWriter::new(&p, 0, 1, 10).is_err());
        assert!(GifWriter::new(&p, 1, 1, 0).is_err());
        let w = GifWriter::new(&p, 1, 1, 10).unwrap();
        assert!(w.finish().is_err());
        assert!(!p.exists());
    }
}
