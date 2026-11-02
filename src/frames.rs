use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Uniform,
    Scene,
}
fn default_count() -> usize {
    8
}
fn default_mode() -> Mode {
    Mode::Scene
}
fn default_gap() -> f64 {
    0.5
}
fn default_change() -> f64 {
    0.04
}
fn default_padding() -> f64 {
    0.75
}
#[derive(Args, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractArgs {
    pub input: PathBuf,
    #[arg(short, long)]
    pub output: PathBuf,
    #[arg(long, default_value_t = 8)]
    #[serde(default = "default_count")]
    pub count: usize,
    #[arg(long,value_enum,default_value_t=Mode::Scene)]
    #[serde(default = "default_mode")]
    pub mode: Mode,
    #[arg(long, default_value_t = 0.5)]
    #[serde(default = "default_gap")]
    pub min_gap: f64,
    #[arg(long, default_value_t = 0.04)]
    #[serde(default = "default_change")]
    pub min_change: f64,
    #[arg(long, default_value_t = 0.75)]
    #[serde(default = "default_padding")]
    pub padding: f64,
}
#[derive(Clone)]
struct Sample {
    index: usize,
    at: u64,
    delay: u64,
    score: f64,
}
#[derive(Serialize)]
pub struct SelectedFrame {
    pub order: usize,
    pub frame_index: usize,
    pub timestamp_ms: u64,
    pub duration_ms: u64,
    pub change_score: f64,
    pub reason: String,
    pub image: PathBuf,
}
#[derive(Serialize)]
pub struct ClipRange {
    pub start_ms: u64,
    pub end_ms: u64,
}
#[derive(Serialize)]
pub struct Report {
    pub schema_version: &'static str,
    pub input: PathBuf,
    pub output: PathBuf,
    pub manifest: PathBuf,
    pub contact_sheet: PathBuf,
    pub width: u32,
    pub height: u32,
    pub source_frames: usize,
    pub duration_ms: u64,
    pub mode: Mode,
    pub semantic_analysis: bool,
    pub selection_note: &'static str,
    pub frames: Vec<SelectedFrame>,
    pub suggested_ranges: Vec<ClipRange>,
}
// Reconstruct cropped GIF frames before scoring or exporting. Keep only a canvas and optional Previous snapshot.
pub fn validate_all(path: &Path) -> Result<()> {
    decode(|_, _, _, _, _, _| Ok(true), path)
}
pub(crate) fn decode(
    mut visit: impl FnMut(usize, u64, u64, u32, u32, &[u8]) -> Result<bool>,
    path: &Path,
) -> Result<()> {
    let _lease = crate::file_guard::Lease::input(path)?;
    ensure!(
        std::fs::metadata(path)?.len() <= 512 * 1024 * 1024,
        "GIF input exceeds 512 MiB budget"
    );
    let started = std::time::Instant::now();
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        std::num::NonZeroU64::new(16_777_216 * 4).unwrap(),
    ));
    let mut reader = options.read_info(File::open(path)?)?;
    let (w, h) = (u32::from(reader.width()), u32::from(reader.height()));
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 16_777_216,
        "GIF exceeds the 16 MP image budget"
    );
    let background = reader
        .bg_color()
        .and_then(|i| {
            reader
                .global_palette()
                .and_then(|p| p.get(i * 3..i * 3 + 3))
        })
        .map(|p| [p[0], p[1], p[2], 255])
        .unwrap_or([0; 4]);
    let mut canvas = vec![0u8; w as usize * h as usize * 4];
    let mut at = 0;
    let mut index = 0;
    while let Some(f) = reader.read_next_frame()? {
        ensure!(index < 100_000, "GIF exceeds 100000 frame budget");
        ensure!(
            (index as u64 + 1) * u64::from(w) * u64::from(h) <= 2_000_000_000,
            "GIF exceeds 2 gigapixel compositing budget; trim the input first"
        );
        ensure!(
            started.elapsed().as_secs() < 60,
            "GIF processing exceeded 60 second budget"
        );
        ensure!(
            u32::from(f.left) + u32::from(f.width) <= w
                && u32::from(f.top) + u32::from(f.height) <= h,
            "GIF frame outside canvas"
        );
        if index == 0 && f.transparent.is_none() {
            for p in canvas.as_chunks_mut::<4>().0 {
                *p = background;
            }
        }
        ensure!(
            f.buffer.len() == usize::from(f.width) * usize::from(f.height) * 4,
            "invalid GIF frame pixel buffer"
        );
        let saved = (f.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        for y in 0..usize::from(f.height) {
            for x in 0..usize::from(f.width) {
                let s = (y * usize::from(f.width) + x) * 4;
                let d = ((y + usize::from(f.top)) * w as usize + x + usize::from(f.left)) * 4;
                if f.buffer[s + 3] != 0 {
                    canvas[d..d + 4].copy_from_slice(&f.buffer[s..s + 4]);
                }
            }
        }
        let rgb: Vec<u8> = canvas
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                if p[3] == 0 {
                    [0; 3]
                } else {
                    [p[0], p[1], p[2]]
                }
            })
            .collect();
        let delay = u64::from(f.delay.max(1)) * 10;
        let keep_reading = visit(index, at, delay, w, h, &rgb)?;
        if let Some(saved) = saved {
            canvas = saved;
        } else if f.dispose == gif::DisposalMethod::Background {
            let bg = if f.transparent.is_some() {
                [0; 4]
            } else {
                background
            };
            for y in usize::from(f.top)..usize::from(f.top) + usize::from(f.height) {
                for x in usize::from(f.left)..usize::from(f.left) + usize::from(f.width) {
                    let i = (y * w as usize + x) * 4;
                    canvas[i..i + 4].copy_from_slice(&bg);
                }
            }
        }
        index += 1;
        at += delay;
        if !keep_reading {
            break;
        }
    }
    ensure!(index > 0, "GIF contains no frames");
    Ok(())
}
pub(crate) fn resize(rgb: &[u8], w: u32, h: u32, tw: u32, th: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(tw as usize * th as usize * 3);
    for y in 0..th {
        for x in 0..tw {
            let i = (((u64::from(y) * u64::from(h) / u64::from(th)) * u64::from(w)
                + u64::from(x) * u64::from(w) / u64::from(tw))
                * 3) as usize;
            out.extend_from_slice(&rgb[i..i + 3]);
        }
    }
    out
}
fn choose(samples: &[Sample], a: &ExtractArgs, total: u64) -> Vec<usize> {
    let mut picks = Vec::new();
    match a.mode {
        Mode::Uniform => {
            for n in 0..a.count {
                let t = if a.count == 1 {
                    total / 2
                } else {
                    n as u64 * total.saturating_sub(1) / (a.count - 1) as u64
                };
                let i = samples
                    .partition_point(|s| s.at + s.delay <= t)
                    .min(samples.len() - 1);
                if !picks.contains(&i) {
                    picks.push(i);
                }
            }
        }
        Mode::Scene => {
            picks.push(0);
            let mut ranked: Vec<_> = (1..samples.len()).collect();
            ranked.sort_by(|&x, &y| {
                samples[y]
                    .score
                    .total_cmp(&samples[x].score)
                    .then(x.cmp(&y))
            });
            for i in ranked {
                if picks.len() >= a.count {
                    break;
                }
                if samples[i].score >= a.min_change
                    && picks.iter().all(|&j| {
                        samples[i].at.abs_diff(samples[j].at) as f64 >= a.min_gap * 1000.0
                    })
                {
                    picks.push(i);
                }
            }
        }
    }
    picks.sort_unstable();
    picks
}
fn ranges(frames: &[SelectedFrame], total: u64, padding: f64) -> Vec<ClipRange> {
    let pad = (padding * 1000.0).round() as u64;
    let mut out: Vec<ClipRange> = Vec::new();
    for f in frames {
        let start = f.timestamp_ms.saturating_sub(pad);
        let end = (f.timestamp_ms + f.duration_ms.min(1000) + pad).min(total);
        if let Some(last) = out.last_mut()
            && start <= last.end_ms
        {
            last.end_ms = last.end_ms.max(end);
        } else {
            out.push(ClipRange {
                start_ms: start,
                end_ms: end,
            });
        }
    }
    out
}
fn png_bytes(rgb: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgb)?;
        writer.finish()?;
    }
    Ok(out)
}
// Tiny numeric labels map each sheet tile to the corresponding manifest entry.
pub(crate) fn label(canvas: &mut [u8], w: usize, x: usize, y: usize, n: usize) {
    const DIGITS: [[u8; 5]; 10] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
    ];
    for (c, b) in n.to_string().bytes().enumerate() {
        for (row, bits) in DIGITS[(b - b'0') as usize].iter().enumerate() {
            for col in 0..3 {
                if bits & (1 << (2 - col)) != 0 {
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let i = ((y + row * 2 + dy) * w + x + c * 8 + col * 2 + dx) * 3;
                            canvas[i..i + 3].fill(240);
                        }
                    }
                }
            }
        }
    }
}
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub fn extract(a: &ExtractArgs) -> Result<Report> {
    ensure!((1..=64).contains(&a.count), "count must be 1..64");
    ensure!(
        a.min_gap.is_finite() && a.min_gap >= 0.0 && a.min_gap <= 3600.0,
        "min_gap must be 0..3600 seconds"
    );
    ensure!(
        a.min_change.is_finite() && (0.0..=1.0).contains(&a.min_change),
        "min_change must be 0..1"
    );
    ensure!(
        a.padding.is_finite() && (0.0..=60.0).contains(&a.padding),
        "padding must be 0..60 seconds"
    );
    let _output_lease = crate::file_guard::Lease::output(&a.output)?;
    let input = a.input.canonicalize().context("open input GIF")?;
    ensure!(
        std::fs::metadata(&input)?.len() <= 512 * 1024 * 1024,
        "input exceeds 512 MiB budget"
    );
    let output = std::path::absolute(&a.output)?;
    ensure!(!output.exists(), "output directory already exists");
    let parent = output
        .parent()
        .context("output requires a parent")?
        .canonicalize()
        .context("output parent must exist")?;
    let output = parent.join(output.file_name().context("invalid output directory")?);
    let identity = std::fs::metadata(&input)?;
    let mut samples = Vec::new();
    let mut last = Vec::new();
    let (mut width, mut height) = (0, 0);
    decode(
        |index, at, delay, w, h, rgb| {
            let thumb = resize(rgb, w, h, 32, 18);
            let score = if last.is_empty() {
                0.0
            } else {
                thumb
                    .iter()
                    .zip(&last)
                    .map(|(&a, &b)| f64::from(a.abs_diff(b)))
                    .sum::<f64>()
                    / (thumb.len() as f64 * 255.0)
            };
            last = thumb;
            samples.push(Sample {
                index,
                at,
                delay,
                score,
            });
            width = w;
            height = h;
            Ok(true)
        },
        &input,
    )?;
    let last_sample = samples.last().context("GIF contains no samples")?;
    let total = last_sample.at + last_sample.delay;
    let chosen = choose(&samples, a, total);
    let selected: BTreeMap<_, _> = chosen
        .iter()
        .enumerate()
        .map(|(order, &i)| (samples[i].index, order))
        .collect();
    let temp = parent.join(format!(
        ".framelet-{}-{}.partial",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    std::fs::create_dir(&temp)?;
    let stage = Stage(temp);
    let cols = chosen.len().clamp(1, 4);
    let rows = chosen.len().div_ceil(cols);
    let (sheet_w, sheet_h) = (cols * 256, rows * 212);
    let mut sheet = vec![28; sheet_w * sheet_h * 3];
    let mut frames = Vec::new();
    decode(
        |index, _, _, w, h, rgb| {
            if let Some(&order) = selected.get(&index) {
                let name = format!("frame-{:03}-{:010}ms.png", order + 1, samples[index].at);
                crate::encode::screenshot(&stage.0.join(&name), w, h, rgb)?;
                let scale = (240.0 / f64::from(w)).min(180.0 / f64::from(h)).min(1.0);
                let (tw, th) = (
                    (f64::from(w) * scale).round().max(1.0) as u32,
                    (f64::from(h) * scale).round().max(1.0) as u32,
                );
                let thumb = resize(rgb, w, h, tw, th);
                let (x, y) = (order % cols * 256 + 8, order / cols * 212 + 24);
                for row in 0..th as usize {
                    let d = ((y + row) * sheet_w + x) * 3;
                    let s = row * tw as usize * 3;
                    sheet[d..d + tw as usize * 3].copy_from_slice(&thumb[s..s + tw as usize * 3]);
                }
                label(&mut sheet, sheet_w, x, y - 18, order + 1);
                let s = &samples[index];
                frames.push(SelectedFrame {
                    order: order + 1,
                    frame_index: index,
                    timestamp_ms: s.at,
                    duration_ms: s.delay,
                    change_score: s.score,
                    reason: match a.mode {
                        Mode::Uniform => "uniform_time_sample",
                        Mode::Scene if index == 0 => "opening_context",
                        Mode::Scene => "visual_change",
                    }
                    .into(),
                    image: output.join(name),
                });
            }
            Ok(true)
        },
        &input,
    )?;
    let after = std::fs::metadata(&input)?;
    ensure!(
        identity.len() == after.len() && identity.modified()? == after.modified()?,
        "input changed during extraction"
    );
    ensure!(
        frames.len() == chosen.len(),
        "input frame inventory changed during extraction"
    );
    let report = Report {
        schema_version: "1.0",
        input,
        output: output.clone(),
        manifest: output.join("manifest.json"),
        contact_sheet: output.join("contact-sheet.png"),
        width,
        height,
        source_frames: samples.len(),
        duration_ms: total,
        mode: a.mode,
        semantic_analysis: false,
        selection_note: "Visual-change candidates only. Review the images to judge semantic importance; suggested ranges are context windows, not semantic highlights.",
        suggested_ranges: ranges(&frames, total, a.padding),
        frames,
    };
    std::fs::write(
        stage.0.join("contact-sheet.png"),
        png_bytes(&sheet, sheet_w as u32, sheet_h as u32)?,
    )?;
    std::fs::write(
        stage.0.join("manifest.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    ensure!(!output.exists(), "output appeared during extraction");
    std::fs::rename(&stage.0, &output).context("publish frame directory")?;
    Ok(report)
}
pub fn extract_value(a: &ExtractArgs) -> Result<serde_json::Value> {
    if crate::series::is_group(&a.input) {
        crate::series::extract(a)
    } else {
        Ok(serde_json::to_value(extract(a)?)?)
    }
}
/// GIFs have no general random-access index: decode the prefix, but return only one image.
/// This saves model context during bisection; it does not promise logarithmic decoder work.
pub struct ProbeOptions {
    pub reference: Option<PathBuf>,
    pub region: Option<crate::capture::Region>,
    pub threshold: u8,
}
impl Default for ProbeOptions {
    fn default() -> Self {
        Self {
            reference: None,
            region: None,
            threshold: 24,
        }
    }
}
pub fn at(
    input: &Path,
    seconds: f64,
    output: Option<&Path>,
    options: ProbeOptions,
) -> Result<serde_json::Value> {
    if crate::series::is_group(input) {
        let (file, local, start) = crate::series::locate(input, seconds)?;
        let mut value = at(&file, local, output, options)?;
        value["recording_group"] = serde_json::json!(input);
        value["segment_start_ms"] = serde_json::json!(start);
        value["global_requested_ms"] = serde_json::json!((seconds * 1000.0).floor() as u64);
        return Ok(value);
    }
    ensure!(
        seconds.is_finite() && (0.0..=3600.0).contains(&seconds),
        "at must be 0..3600 seconds"
    );
    if let Some(output) = output {
        crate::safety::extension(output, "png")?;
        ensure!(!output.exists(), "output already exists");
    }
    ensure!(
        options.reference.is_some() || options.region.is_none(),
        "compare_region requires reference"
    );
    let reference = options.reference.as_deref().map(read_png).transpose()?;
    let requested = (seconds * 1000.0).floor() as u64;
    let mut result = None;
    decode(
        |index, start, duration, w, h, rgb| {
            if requested >= start && requested < start + duration {
                let comparison = if let Some((rw, rh, ref_pixels)) = &reference {
                    ensure!(
                        (*rw, *rh) == (w, h),
                        "reference dimensions must match the source GIF"
                    );
                    let r = options.region.unwrap_or(crate::capture::Region {
                        x: 0,
                        y: 0,
                        width: w,
                        height: h,
                    });
                    ensure!(
                        r.x >= 0
                            && r.y >= 0
                            && r.width > 0
                            && r.height > 0
                            && i64::from(r.x) + i64::from(r.width) <= i64::from(w)
                            && i64::from(r.y) + i64::from(r.height) <= i64::from(h),
                        "compare_region must fit inside the image"
                    );
                    let (mut changed, mut sum) = (0u64, 0u64);
                    for y in r.y as u32..r.y as u32 + r.height {
                        for x in r.x as u32..r.x as u32 + r.width {
                            let at = (y as usize * w as usize + x as usize) * 3;
                            let diffs = [
                                rgb[at].abs_diff(ref_pixels[at]),
                                rgb[at + 1].abs_diff(ref_pixels[at + 1]),
                                rgb[at + 2].abs_diff(ref_pixels[at + 2]),
                            ];
                            changed +=
                                u64::from(diffs[0].max(diffs[1]).max(diffs[2]) > options.threshold);
                            sum += diffs.iter().map(|d| u64::from(*d)).sum::<u64>();
                        }
                    }
                    let pixels = u64::from(r.width) * u64::from(r.height);
                    Some(
                        serde_json::json!({"reference":options.reference,"region":r,"difference_threshold":options.threshold,"metric":"max_absolute_rgb_channel","changed_pixels":changed,"pixels":pixels,"changed_fraction":changed as f64/pixels as f64,"mean_absolute_error":sum as f64/(pixels*3) as f64}),
                    )
                } else {
                    None
                };
                let mut value = if let Some(output) = output {
                    crate::encode::screenshot(output, w, h, rgb)?
                } else {
                    serde_json::json!({"width":w,"height":h})
                };
                value["input"] = serde_json::json!(input.canonicalize()?);
                value["requested_ms"] = serde_json::json!(requested);
                value["frame_index"] = serde_json::json!(index);
                value["timestamp_ms"] = serde_json::json!(start);
                value["duration_ms"] = serde_json::json!(duration);
                if let Some(comparison) = comparison {
                    value["comparison"] = comparison;
                }
                result = Some(value);
                return Ok(false);
            }
            Ok(true)
        },
        input,
    )?;
    result.context("timestamp is outside GIF playback duration")
}

pub(crate) fn read_png(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let _lease = crate::file_guard::Lease::input(path)?;
    ensure!(
        std::fs::metadata(path)?.len() <= 32 * 1024 * 1024,
        "PNG exceeds 32 MiB budget"
    );
    let mut decoder = png::Decoder::new(File::open(path)?);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let (w, h) = (reader.info().width, reader.info().height);
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 16_777_216,
        "PNG exceeds 16 MP budget"
    );
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes)?;
    let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
    for p in bytes[..info.buffer_size()].chunks(info.color_type.samples()) {
        let (r, g, b, alpha) = match info.color_type {
            png::ColorType::Rgb => (p[0], p[1], p[2], 255),
            png::ColorType::Rgba => (p[0], p[1], p[2], p[3]),
            png::ColorType::Grayscale => (p[0], p[0], p[0], 255),
            png::ColorType::GrayscaleAlpha => (p[0], p[0], p[0], p[1]),
            _ => anyhow::bail!("unsupported PNG color type"),
        };
        rgb.extend([r, g, b].map(|v| (u16::from(v) * u16::from(alpha) / 255) as u8));
    }
    Ok((w, h, rgb))
}

pub fn preview_png(path: &Path, max_side: u32) -> Result<(serde_json::Value, Vec<u8>)> {
    let (w, h, rgb) = read_png(path)?;
    let mut side = max_side;
    loop {
        let scale = (f64::from(side) / f64::from(w.max(h))).min(1.0);
        let (tw, th) = (
            (f64::from(w) * scale).round().max(1.0) as u32,
            (f64::from(h) * scale).round().max(1.0) as u32,
        );
        let thumb = resize(&rgb, w, h, tw, th);
        let data = png_bytes(&thumb, tw, th)?;
        if data.len() <= 1024 * 1024 {
            return Ok((
                serde_json::json!({"input":path.canonicalize()?,"source_width":w,"source_height":h,"preview_width":tw,"preview_height":th}),
                data,
            ));
        }
        ensure!(side > 64, "PNG preview exceeds 1 MiB");
        side /= 2;
    }
}
