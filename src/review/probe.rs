use crate::{capture::Region, frames::ProbeOptions};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(clap::Args, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    pub input: PathBuf,
    #[arg(long,num_args=1..=64,required=true)]
    pub times: Vec<f64>,
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long,value_enum,default_value_t=Export::Sheet)]
    #[serde(default)]
    pub export: Export,
    #[arg(skip)]
    #[serde(default)]
    pub include_image: bool,
    #[arg(long)]
    pub reference: Option<PathBuf>,
    #[arg(long, allow_hyphen_values = true)]
    pub compare_region: Option<Region>,
    #[arg(long, default_value_t = 24)]
    #[serde(default = "threshold")]
    pub difference_threshold: u8,
}
#[derive(Clone, Copy, Default, clap::ValueEnum, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Export {
    #[default]
    Sheet,
    Frames,
}
fn threshold() -> u8 {
    24
}
struct Frame {
    index: usize,
    at: u64,
    delay: u64,
    w: u32,
    h: u32,
    rgb: Vec<u8>,
}
pub fn compare(
    rgb: &[u8],
    w: u32,
    h: u32,
    reference: &(u32, u32, Vec<u8>),
    region: Option<Region>,
    threshold: u8,
) -> Result<Value> {
    ensure!(
        (reference.0, reference.1) == (w, h),
        "reference dimensions must match the source GIF"
    );
    let r = region.unwrap_or(Region {
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
            && r.x as u64 + r.width as u64 <= w as u64
            && r.y as u64 + r.height as u64 <= h as u64,
        "compare_region must fit inside the image"
    );
    let (mut changed, mut sum) = (0u64, 0u64);
    for y in r.y as u32..r.y as u32 + r.height {
        for x in r.x as u32..r.x as u32 + r.width {
            let i = (y as usize * w as usize + x as usize) * 3;
            let d = [
                rgb[i].abs_diff(reference.2[i]),
                rgb[i + 1].abs_diff(reference.2[i + 1]),
                rgb[i + 2].abs_diff(reference.2[i + 2]),
            ];
            changed += u64::from(*d.iter().max().unwrap_or(&0) > threshold);
            sum += d.iter().map(|&v| u64::from(v)).sum::<u64>();
        }
    }
    let pixels = u64::from(r.width) * u64::from(r.height);
    Ok(
        json!({"region":r,"difference_threshold":threshold,"metric":"max_absolute_rgb_channel","changed_pixels":changed,"pixels":pixels,"changed_fraction":changed as f64/pixels as f64,"mean_absolute_error":sum as f64/(pixels*3) as f64}),
    )
}
fn metadata(
    frame: &Frame,
    input: &Path,
    requested: u64,
    reference: Option<&(u32, u32, Vec<u8>)>,
    options: &ProbeOptions,
) -> Result<Value> {
    let mut value = json!({"input":input.canonicalize()?,"requested_ms":requested,"frame_index":frame.index,"timestamp_ms":frame.at,"duration_ms":frame.delay,"width":frame.w,"height":frame.h});
    if let Some(reference) = reference {
        value["comparison"] = compare(
            &frame.rgb,
            frame.w,
            frame.h,
            reference,
            options.region,
            options.threshold,
        )?;
        value["comparison"]["reference"] = json!(options.reference);
    }
    Ok(value)
}
pub fn single(
    input: &Path,
    seconds: f64,
    output: Option<&Path>,
    options: ProbeOptions,
) -> Result<Value> {
    ensure!(
        seconds.is_finite() && (0.0..=3600.).contains(&seconds),
        "at must be 0..3600 seconds"
    );
    ensure!(
        options.reference.is_some() || options.region.is_none(),
        "compare_region requires reference"
    );
    if let Some(output) = output {
        crate::safety::extension(output, "png")?;
        ensure!(!output.try_exists()?, "output already exists");
    }
    let reference = options
        .reference
        .as_deref()
        .map(crate::frames::read_png)
        .transpose()?;
    let requested = (seconds * 1000.).floor() as u64;
    let frame = crate::review_cache::get_tag(
        input,
        &[input.to_path_buf()],
        &format!("frame:{requested}"),
        || {
            let mut frame = None;
            crate::frames::scan(
                input,
                |_, at, delay, _, _| requested >= at && requested < at + delay,
                |index, at, delay, w, h, rgb| {
                    if requested >= at && requested < at + delay {
                        frame = Some(Frame {
                            index,
                            at,
                            delay,
                            w,
                            h,
                            rgb: rgb.to_vec(),
                        });
                        return Ok(false);
                    }
                    Ok(true)
                },
            )?;
            let frame = frame.context("timestamp is outside GIF playback duration")?;
            let bytes = frame.rgb.capacity() + 128;
            Ok((frame, bytes))
        },
    )?;
    let mut value = metadata(&frame, input, requested, reference.as_ref(), &options)?;
    if let Some(output) = output {
        let result = crate::encode::screenshot(output, frame.w, frame.h, &frame.rgb)?;
        for key in ["output", "bytes", "warnings"] {
            value[key] = result[key].clone();
        }
    }
    Ok(value)
}
pub fn batch(
    input: &Path,
    times: &[f64],
    options: ProbeOptions,
    output: Option<&Path>,
    export: Export,
) -> Result<Value> {
    ensure!(
        (1..=64).contains(&times.len())
            && times
                .iter()
                .all(|t| t.is_finite() && (0.0..=86400.).contains(t)),
        "times requires 1..64 finite timestamps in 0..86400 seconds"
    );
    ensure!(
        options.reference.is_some() || options.region.is_none(),
        "compare_region requires reference"
    );
    if crate::series::is_group(input) {
        // One traversal per selected segment; never repeat that segment for each timestamp.
        let mut groups: BTreeMap<PathBuf, Vec<(usize, f64, u64)>> = BTreeMap::new();
        for (i, &time) in times.iter().enumerate() {
            let (file, local, start) = crate::series::locate(input, time)?;
            groups.entry(file).or_default().push((i, local, start));
        }
        let mut frames = vec![Value::Null; times.len()];
        let mut passes = 0u64;
        let mut decoded = 0u64;
        ensure!(
            output.is_none(),
            "group batch numeric review: export selected segment images separately"
        );
        for (file, requests) in groups {
            let local: Vec<_> = requests.iter().map(|r| r.1).collect();
            let value = batch(
                &file,
                &local,
                ProbeOptions {
                    reference: options.reference.clone(),
                    region: options.region,
                    threshold: options.threshold,
                },
                None,
                export,
            )?;
            passes += value["decode_passes"].as_u64().unwrap_or(0);
            decoded += value["work"]["decoded_frames"].as_u64().unwrap_or(0);
            for ((i, _, start), frame) in requests
                .into_iter()
                .zip(value["frames"].as_array().context("batch frames missing")?)
            {
                let mut f = frame.clone();
                f["segment_start_ms"] = json!(start);
                f["global_requested_ms"] = json!((times[i] * 1000.).floor() as u64);
                frames[i] = f;
            }
        }
        return Ok(
            json!({"input":input,"frames":frames,"decode_passes":passes,"work":{"decoded_frames":decoded},"coordinate_space":"each segment's decoded image pixels"}),
        );
    }
    let reference = options
        .reference
        .as_deref()
        .map(crate::frames::read_png)
        .transpose()?;
    let mut requested: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (i, &t) in times.iter().enumerate() {
        requested
            .entry((t * 1000.).floor() as u64)
            .or_default()
            .push(i);
    }
    let mut result = vec![Value::Null; times.len()];
    let max = *requested.last_key_value().context("empty times")?.0;
    let mut exported = Vec::new();
    let stage = output
        .map(crate::storage::StagedDirectory::new)
        .transpose()?;
    let cols = times.len().min(4);
    let rows = times.len().div_ceil(cols);
    let sw = cols * 256;
    let sh = rows * 212;
    let mut sheet = output.map(|_| vec![28u8; sw * sh * 3]);
    let work = crate::frames::scan(
        input,
        |_, at, delay, _, _| requested.range(at..at + delay).next().is_some(),
        |index, at, delay, w, h, rgb| {
            let matches: Vec<_> = requested.range(at..at + delay).collect();
            if !matches.is_empty() {
                let frame = Frame {
                    index,
                    at,
                    delay,
                    w,
                    h,
                    rgb: rgb.to_vec(),
                };
                for (&time, indices) in matches {
                    let mut value = metadata(&frame, input, time, reference.as_ref(), &options)?;
                    if let Some(output) = output.filter(|_| export == Export::Frames) {
                        let name = stage
                            .as_ref()
                            .context("batch stage missing")?
                            .path
                            .join(format!("frame-{:010}ms.png", time));
                        let saved = crate::encode::screenshot(&name, w, h, rgb)?;
                        value["output"] =
                            json!(output.join(name.file_name().context("frame filename missing")?));
                        let _ = saved;
                        exported
                            .push(output.join(name.file_name().context("frame filename missing")?));
                    }
                    for &i in indices {
                        result[i] = value.clone();
                        result[i]["request_index"] = json!(i);
                        if let Some(sheet) = &mut sheet {
                            let scale = (240.0 / w as f64).min(180.0 / h as f64).min(1.0);
                            let tw = (w as f64 * scale).round().max(1.) as u32;
                            let th = (h as f64 * scale).round().max(1.) as u32;
                            let thumb = crate::frames::resize(rgb, w, h, tw, th);
                            let (x, y) = (i % cols * 256 + 8, i / cols * 212 + 24);
                            for row in 0..th as usize {
                                let d = ((y + row) * sw + x) * 3;
                                let a = row * tw as usize * 3;
                                sheet[d..d + tw as usize * 3]
                                    .copy_from_slice(&thumb[a..a + tw as usize * 3]);
                            }
                            crate::frames::label(sheet, sw, x, y - 18, i + 1);
                            result[i]["sheet_tile"] = json!(i + 1);
                        }
                    }
                }
            }
            Ok(at + delay <= max)
        },
    );
    let work = work?;
    ensure!(
        result.iter().all(|v| !v.is_null()),
        "timestamp is outside GIF playback duration; output was not published"
    );
    if let Some(stage) = stage {
        if let Some(sheet) = &sheet {
            crate::encode::screenshot(
                &stage.path.join("contact-sheet.png"),
                sw as u32,
                sh as u32,
                sheet,
            )?;
        }
        crate::storage::replace_json(
            &stage.path.join("manifest.json"),
            &json!({"frames":result,"work":work}),
        )?;
        stage.publish()?;
    }
    Ok(
        json!({"input":input.canonicalize()?,"frames":result,"decode_passes":1,"work":work,"exported":exported,"contact_sheet":output.map(|p|p.join("contact-sheet.png")),"include_image":false}),
    )
}
