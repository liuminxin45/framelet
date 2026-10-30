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
}
impl GifWriter {
    pub fn new(path: &Path, width: u32, height: u32, quality: i32) -> Result<Self> {
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
        let partial = path.with_file_name(format!(
            "{}.{}.partial",
            path.file_name().unwrap().to_string_lossy(),
            std::process::id()
        ));
        let f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .context("create output (parent directory must exist)")?;
        let mut enc = Encoder::new(BufWriter::new(f), width as u16, height as u16, &[])?;
        enc.set_repeat(Repeat::Infinite)?;
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
        })
    }
    fn flush(&mut self) -> Result<()> {
        if let Some(frame) = self.pending.take() {
            self.enc.as_mut().unwrap().write_frame(&frame)?;
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
            self.pending.as_mut().unwrap().delay += delay;
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
        Ok(())
    }
    pub fn finish(mut self) -> Result<Stats> {
        ensure!(self.samples > 0, "recording contains no frames");
        self.flush()?;
        let enc = self.enc.take().unwrap();
        let mut writer = enc.into_inner()?;
        use std::io::Write;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        // Hard-link publishes without replacing an existing user file, even under races.
        std::fs::hard_link(&self.partial, &self.path)
            .context("publish output without overwrite")?;
        self.committed = true;
        std::fs::remove_file(&self.partial)?;
        Ok(Stats {
            output: self.path.clone(),
            width: self.width,
            height: self.height,
            frames: self.frames,
            samples: self.samples,
            duration_ms: self.duration_cs * 10,
            bytes: std::fs::metadata(&self.path)?.len(),
        })
    }
}
impl Drop for GifWriter {
    fn drop(&mut self) {
        if !self.committed {
            self.enc.take();
            let _ = std::fs::remove_file(&self.partial);
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Stats {
    pub output: PathBuf,
    pub width: u16,
    pub height: u16,
    pub frames: u64,
    pub samples: u64,
    pub duration_ms: u64,
    pub bytes: u64,
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
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut enc = png::Encoder::new(BufWriter::new(file), width, height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(rgb)?;
    writer.finish()?;
    Ok(
        serde_json::json!({"output":std::path::absolute(path)?,"width":width,"height":height,"bytes":std::fs::metadata(path)?.len()}),
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
            && speed > 0.0
            && speed <= 100.0
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
    let mut writer: Option<GifWriter> = None;
    let mut timing_error = 0.0;
    crate::frames::decode(
        |_, at, delay_ms, w, h, rgb| {
            let time = at as f64 / 1000.0;
            let next = (at + delay_ms) as f64 / 1000.0;
            let included = next.min(end.unwrap_or(f64::INFINITY)) - time.max(start);
            if included > 0.0 {
                let desired = included / speed * 100.0 + timing_error;
                let delay = desired.round().clamp(1.0, 65535.0);
                timing_error = desired - delay;
                if writer.is_none() {
                    writer = Some(GifWriter::new(output, w, h, 10)?);
                }
                writer.as_mut().unwrap().push(rgb.to_vec(), delay as u16)?;
            }
            Ok(end.is_none_or(|e| next < e))
        },
        input,
    )?;
    writer.context("trim range contains no frames")?.finish()
}
pub fn inspect(input: &Path) -> Result<serde_json::Value> {
    let mut reader = gif::DecodeOptions::new().read_info(File::open(input)?)?;
    let (w, h) = (reader.width(), reader.height());
    let (mut frames, mut cs) = (0u64, 0u64);
    while let Some(f) = reader.read_next_frame()? {
        frames += 1;
        cs += u64::from(f.delay);
    }
    Ok(
        serde_json::json!({"input":std::path::absolute(input)?,"width":w,"height":h,"frames":frames,"duration_ms":cs*10,"bytes":std::fs::metadata(input)?.len()}),
    )
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
