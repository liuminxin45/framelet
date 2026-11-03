pub mod markers;
mod options;
mod services;
use crate::{
    capture::{self, Grabber, Region},
    encode::{GifWriter, Stats},
};
use anyhow::{Result, ensure};
pub use options::RecordArgs;
pub use services::{doctor, shot};
use std::{
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Control {
    pub markers: std::sync::Mutex<markers::Ledger>,
    pub series: std::sync::Mutex<Option<serde_json::Value>>,
    pub stop: AtomicBool,
    pub single: AtomicBool,
    pub cancel: AtomicBool,
    pub paused: AtomicBool,
    pub phase: AtomicU8, // 0 preparing/countdown, 1 recording, 2 paused, 3 finalizing
    pub samples: AtomicU64,
    pub elapsed_ms: AtomicU64,
}
pub fn validate(a: &RecordArgs) -> Result<(Region, u32, u32)> {
    ensure!((1..=60).contains(&a.fps), "fps must be 1..60");
    ensure!(
        a.duration.is_finite() && a.duration > 0.0 && a.duration <= 300.0,
        "single GIF duration must be >0 and <=300 seconds; use record-series for longer capture"
    );
    ensure!(
        a.delay.is_finite() && (0.0..=30.0).contains(&a.delay),
        "delay must be 0..30 seconds"
    );
    ensure!((1..=30).contains(&a.quality), "quality must be 1..30");
    ensure!((1..=1024).contains(&a.max_mib), "max_mib must be 1..1024");
    ensure!(a.masks.len() <= 32, "at most 32 privacy masks");
    crate::safety::extension(&a.output, "gif")?;
    ensure!(!a.output.exists(), "output already exists");
    let r = a.region.unwrap_or_else(capture::desktop);
    capture::validate(r)?;
    let w = a.max_width.unwrap_or(r.width).min(r.width);
    ensure!(
        w > 0 && a.max_width.is_none_or(|w| w <= 8192),
        "max_width must be 1..8192"
    );
    let h = (u64::from(r.height) * u64::from(w) / u64::from(r.width)).max(1) as u32;
    ensure!(
        u64::from(w) * u64::from(h) * u64::from(a.fps) <= 80_000_000,
        "capture exceeds 80 MP/s encoding budget; lower fps or max_width"
    );
    for m in &a.masks {
        ensure!(
            m.x >= 0
                && m.y >= 0
                && m.width > 0
                && m.height > 0
                && i64::from(m.x) + i64::from(m.width) <= i64::from(r.width)
                && i64::from(m.y) + i64::from(m.height) <= i64::from(r.height),
            "mask must fit inside the source region; coordinates are relative to its top-left"
        );
    }
    Ok((r, w, h))
}
pub fn transform(mut rgb: Vec<u8>, r: Region, w: u32, h: u32, masks: &[Region]) -> Vec<u8> {
    // Redact before resampling and before any frame reaches an encoder.
    for m in masks {
        for y in m.y as u32..m.y as u32 + m.height {
            let at = (u64::from(y) * u64::from(r.width) + m.x as u64) as usize * 3;
            rgb[at..at + m.width as usize * 3].fill(24);
        }
    }
    if (w, h) == (r.width, r.height) {
        return rgb;
    }
    // Area averaging preserves small text better than nearest-neighbor downsampling.
    let mut out = vec![0; w as usize * h as usize * 3];
    for y in 0..h {
        let y0 = y * r.height / h;
        let y1 = ((y + 1) * r.height / h).max(y0 + 1);
        for x in 0..w {
            let x0 = x * r.width / w;
            let x1 = ((x + 1) * r.width / w).max(x0 + 1);
            let mut sums = [0u32; 3];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let at = (sy as usize * r.width as usize + sx as usize) * 3;
                    for c in 0..3 {
                        sums[c] += u32::from(rgb[at + c]);
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            let at = (y as usize * w as usize + x as usize) * 3;
            for c in 0..3 {
                out[at + c] = (sums[c] / n) as u8;
            }
        }
    }
    out
}
fn capture_frame(
    writer: &mut GifWriter,
    grab: &Grabber,
    r: Region,
    w: u32,
    h: u32,
    masks: &[Region],
) -> Result<Vec<u8>> {
    let rgb = writer.capture(grab)?;
    let began = std::time::Instant::now();
    let rgb = transform(rgb, r, w, h, masks);
    writer.observe_transform(began.elapsed());
    Ok(rgb)
}
pub fn run(a: &RecordArgs, c: &Control, external_stop: impl Fn() -> bool) -> Result<Stats> {
    crate::worker::supervise(a, c, external_stop)
}
pub(crate) fn run_local(
    a: &RecordArgs,
    c: &Control,
    external_stop: impl Fn() -> bool,
    mut notify: impl FnMut(&Control),
) -> Result<Stats> {
    let (r, w, h) = validate(a)?;
    crate::timeline::available(&a.output)?;
    let wait = Instant::now();
    while wait.elapsed().as_secs_f64() < a.delay {
        ensure!(
            !c.cancel.load(Ordering::Relaxed)
                && !c.stop.load(Ordering::Relaxed)
                && !external_stop(),
            "recording cancelled before capture"
        );
        notify(c);
        std::thread::sleep(Duration::from_millis(20));
    }
    let grab = Grabber::new(r, a.cursor)?;
    let mut writer = GifWriter::new(&a.output, w, h, a.quality)?;
    writer.track(r, &a.masks)?;
    writer.set_limit(a.max_mib * 1024 * 1024);
    writer.set_capture_fps(a.fps);
    let start = Instant::now();
    let duration = Duration::from_secs_f64(a.duration);
    let period = Duration::from_secs_f64(1.0 / f64::from(a.fps));
    let mut active = Duration::ZERO;
    let mut tick = start;
    let mut next = Duration::ZERO;
    let mut accounted = 0u64;
    let mut previous = None;
    let mut was_paused = false;
    let mut progress = Instant::now();
    if a.events {
        crate::runtime::progress(
            serde_json::json!({"event":"started","region":r,"width":w,"height":h}),
        );
    }
    loop {
        writer.process_markers(c)?;
        notify(c);
        let now = Instant::now();
        if !was_paused {
            active +=
                now.duration_since(start).min(duration) - tick.duration_since(start).min(duration);
        }
        tick = now;
        ensure!(
            !c.cancel.load(Ordering::Relaxed),
            "recording cancelled; unfinished output discarded"
        );
        let paused = c.paused.load(Ordering::Relaxed);
        let stopping = start.elapsed() >= duration
            || active >= duration
            || c.stop.load(Ordering::Relaxed)
            || external_stop();
        c.elapsed_ms
            .store(active.as_millis() as u64, Ordering::Relaxed);
        if a.events && progress.elapsed() >= Duration::from_secs(1) {
            crate::runtime::progress(
                serde_json::json!({"event":"progress","elapsed_ms":start.elapsed().as_millis(),"recorded_ms":active.as_millis(),"samples":c.samples.load(Ordering::Relaxed),"paused":paused}),
            );
            progress = Instant::now();
        }
        if (paused || stopping || active >= next) && previous.is_some() {
            let target = (active.as_secs_f64() * 100.0).round() as u64;
            let delay = target.saturating_sub(accounted).clamp(1, 65535) as u16;
            if let Some(pixels) = previous.take() {
                writer.push(pixels, delay)?;
            }
            accounted += u64::from(delay);
        }
        if stopping {
            if writer.samples == 0 {
                let pixels = capture_frame(&mut writer, &grab, r, w, h, &a.masks)?;
                writer.push(pixels, 1)?;
            }
            break;
        }
        if paused && c.single.swap(false, Ordering::Relaxed) {
            let pixels = capture_frame(&mut writer, &grab, r, w, h, &a.masks)?;
            writer.push(pixels, 10)?;
            accounted += 10;
            active += Duration::from_millis(100);
            c.samples.fetch_add(1, Ordering::Relaxed);
        }
        if paused {
            c.phase.store(2, Ordering::Relaxed);
        }
        if !paused && (was_paused || active >= next) {
            writer.observe_lateness(active.saturating_sub(next), period);
            let rgb = capture_frame(&mut writer, &grab, r, w, h, &a.masks)?;
            previous = Some(rgb);
            c.samples.fetch_add(1, Ordering::Relaxed);
            c.phase.store(1, Ordering::Relaxed);
            next = if was_paused {
                active + period
            } else {
                next + period
            };
            if next <= active {
                next = active + period;
            }
        }
        was_paused = paused;
        // Bounded polling keeps pause/stop/EOF responsive, including at 1 fps.
        let wait = sample_wait(next, active, tick.elapsed(), paused);
        if wait.is_zero() {
            std::thread::yield_now()
        } else {
            std::thread::sleep(wait)
        }
    }
    c.phase.store(3, Ordering::Relaxed);
    notify(c);
    let stats = writer.finish()?;
    if a.events {
        crate::runtime::progress(serde_json::json!({"event":"saved","result":stats}));
    }
    Ok(stats)
}

/// Include capture/encoding work in the wait; fixed polling can consume a 60-FPS frame budget.
pub fn sample_wait(next: Duration, active: Duration, work: Duration, paused: bool) -> Duration {
    if paused {
        Duration::from_millis(10)
    } else {
        next.saturating_sub(active.saturating_add(work))
            .min(Duration::from_millis(10))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_wait_accounts_for_work_and_never_blocks_controls() {
        let deadline = Duration::from_micros(16667);
        assert_eq!(
            sample_wait(deadline, Duration::ZERO, Duration::from_millis(5), false),
            Duration::from_millis(10)
        );
        assert_eq!(
            sample_wait(deadline, Duration::ZERO, Duration::from_millis(15), false),
            Duration::from_micros(1667)
        );
        assert_eq!(
            sample_wait(deadline, Duration::ZERO, Duration::from_millis(20), false),
            Duration::ZERO
        );
        assert_eq!(
            sample_wait(deadline, Duration::ZERO, Duration::ZERO, true),
            Duration::from_millis(10)
        );
    }
    #[test]
    fn masks_precede_resampling() {
        let r = Region {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        };
        let mask = Region {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
        let pixels = transform(vec![255; 24], r, 2, 1, &[mask]);
        assert_eq!(pixels, vec![24, 24, 24, 255, 255, 255]);
    }
}
