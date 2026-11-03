use super::{model::*, store};
use crate::{
    capture::Grabber,
    encode::GifWriter,
    recording::Control,
    timeline::{Data, Recorder},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{atomic::Ordering, mpsc},
    time::{Duration, Instant},
};

struct Finalizer {
    rx: mpsc::Receiver<Result<Part>>,
    thread: std::thread::JoinHandle<()>,
}
impl Finalizer {
    fn start(
        root: &Path,
        writer: GifWriter,
        mut data: Data,
        number: u32,
        start_ms: u64,
        reason: &str,
    ) -> Result<Self> {
        let root = root.to_path_buf();
        let reason = reason.to_string();
        let (tx, rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("framelet-segment-commit".into())
            .spawn(move || {
                let result = crate::runtime::guard("series.segment.commit", || {
                    let mut stats = writer.finish()?;
                    data.gif_bytes = stats.bytes;
                    data.frames = stats.frames;
                    data.duration_ms = stats.duration_ms;
                    stats.warnings.extend(data.warnings.iter().cloned());
                    if data.dropped_events > 0 {
                        stats
                            .warnings
                            .push(format!("{} input events dropped", data.dropped_events))
                    }
                    match crate::timeline::publish(&stats.output, &data) {
                        Ok(p) => {
                            stats.timeline = Some(p);
                            match crate::transaction::complete(&stats.output) {
                                Ok(p) => stats.manifest = Some(p),
                                Err(e) => stats
                                    .warnings
                                    .push(format!("media saved; manifest failed: {e}")),
                            }
                        }
                        Err(e) => stats
                            .warnings
                            .push(format!("media saved; timeline failed: {e}")),
                    }
                    store::relative_stats(&mut stats)?;
                    let part = Part::new(number, start_ms, reason, stats, &data);
                    store::commit_part(&root, &part)?;
                    Ok(part)
                });
                let _ = tx.send(result);
            })?;
        Ok(Self { rx, thread })
    }
    fn poll(
        self,
        index: &mut Index,
        root: &Path,
        control: &Control,
        notify: &mut impl FnMut(&Control),
        wait: bool,
    ) -> Result<Option<Self>> {
        let began = Instant::now();
        loop {
            match self.rx.try_recv() {
                Ok(part) => {
                    let _ = self.thread.join();
                    let part = part?;
                    let file = root.join(&part.recording.output);
                    let checkpoint = root
                        .join(".work")
                        .join(file.file_name().context("part name")?)
                        .with_extension("checkpoint.json");
                    index.parts.push(part);
                    store::save_index(root, index)?;
                    if checkpoint.exists() {
                        std::fs::remove_file(&checkpoint)?;
                    }
                    return Ok(None);
                }
                Err(mpsc::TryRecvError::Empty) if !wait => return Ok(Some(self)),
                Err(mpsc::TryRecvError::Empty) => {
                    ensure!(
                        began.elapsed() < Duration::from_secs(15),
                        "segment finalization timeout; durable checkpoints preserved"
                    );
                    notify(control);
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => anyhow::bail!("segment finalizer exited without a result"),
            }
        }
    }
}
fn checkpoint(
    root: &Path,
    index: &Index,
    number: u32,
    start_ms: u64,
    writer: &mut GifWriter,
    recorder: &mut Recorder,
) -> Result<()> {
    if writer.samples == 0 {
        return Ok(());
    }
    let bytes = writer.checkpoint()?;
    let stats = writer.snapshot_stats(bytes + 1);
    let data = recorder.checkpoint_data(&stats);
    let c = Checkpoint {
        gif_prefix_sha256: writer.prefix_sha256()?,
        schema: 1,
        session_id: index.session_id.clone(),
        number,
        start_ms,
        source: writer
            .partial_path()
            .file_name()
            .context("partial filename")?
            .to_string_lossy()
            .into_owned(),
        target: stats
            .output
            .file_name()
            .context("GIF filename")?
            .to_string_lossy()
            .into_owned(),
        safe_bytes: bytes,
        recording: stats,
        timeline: data,
    };
    let path = root
        .join(".work")
        .join(store::filename(&index.prefix, number))
        .with_extension("checkpoint.json");
    crate::storage::replace_json(&path, &c)
}
fn new_writer(root: &Path, index: &Index, number: u32, w: u32, h: u32) -> Result<GifWriter> {
    let mut writer = GifWriter::new_in(
        &root.join(store::filename(&index.prefix, number)),
        w,
        h,
        index.request.quality,
        Some(&root.join(".work")),
    )?;
    writer.set_limit(index.request.max_mib * 1024 * 1024);
    writer.set_capture_fps(index.request.fps);
    Ok(writer)
}

pub fn run_local(plan: &Plan, c: &Control, mut notify: impl FnMut(&Control)) -> Result<Value> {
    let root = &plan.directory;
    let _lease = crate::file_guard::Lease::output(&root.join("recording.json"))?;
    let mut index = store::read_index(root)?;
    ensure!(
        index.parts.is_empty() && index.state == "starting",
        "recording group has already started; use status/recover"
    );
    index.owner_pid = std::process::id();
    index.owner_created =
        crate::identity::process_stamp(std::process::id())?.context("worker exited")?;
    index.state = "recording".into();
    index.reason = "recording".into();
    store::save_index(root, &index)?;
    let a = index.request.clone();
    a.validate()?;
    let (region, w, h) = crate::recording::validate(&a.capture(&root.join(".validation.gif")))?;
    let reserve = u64::from(w) * u64::from(h) * 2 + 65_536;
    ensure!(
        reserve < a.max_mib * 1024 * 1024,
        "max_mib cannot accommodate one worst-case frame and finalization reserve"
    );
    ensure!(
        store::free_space(root)? > 1_073_741_824 + reserve,
        "insufficient free disk space for safe recording"
    );
    let began = Instant::now();
    while began.elapsed().as_secs_f64() < a.delay {
        if c.stop.load(Ordering::Relaxed) || c.cancel.load(Ordering::Relaxed) {
            index.state = "cancelled".into();
            index.reason = "stopped_before_capture".into();
            store::save_index(root, &index)?;
            return Ok(store::part_summary(root, &index));
        }
        notify(c);
        std::thread::sleep(Duration::from_millis(10));
    }
    let grab = Grabber::new(region, a.cursor)?;
    let mut recorder = Recorder::new(region, w, h, &a.masks)?;
    recorder.set_identity(&index.session_id, &crate::identity::new_id()?);
    recorder.data.capture_fps = Some(a.fps);
    let mut number = 1u32;
    let mut start_cs = 0u64;
    let mut writer = Some(new_writer(root, &index, number, w, h)?);
    let mut finalizer: Option<Finalizer> = None;
    let mut previous: Option<Vec<u8>> = None;
    let mut active = Duration::ZERO;
    let mut tick = Instant::now();
    let started = tick;
    let mut was_paused = c.paused.load(Ordering::Relaxed);
    let mut next = Duration::ZERO;
    let period = Duration::from_secs_f64(1.0 / a.fps as f64);
    let total_cs = a.duration.map(|d| (d * 100.0).round() as u64);
    let mut last_checkpoint = Instant::now();
    let mut disk_tick = Instant::now();
    let mut used = store::disk_usage(root)?;
    let mut free = store::free_space(root)?;
    let mut ending = "duration".to_string();
    let result = crate::runtime::guard("series.capture", || -> Result<()> {
        loop {
            recorder.process_markers(c)?;
            notify(c);
            if let Some(f) = finalizer.take() {
                finalizer = f.poll(&mut index, root, c, &mut notify, false)?;
            }
            let now = Instant::now();
            let dt = now.duration_since(tick);
            tick = now;
            if !was_paused {
                active += dt;
            }
            if let Some(total) = total_cs {
                active = active.min(Duration::from_millis(total * 10));
            }
            let paused = c.paused.load(Ordering::Relaxed);
            let cancel = c.cancel.load(Ordering::Relaxed);
            let wall_limit = started.elapsed().as_secs() >= a.max_wall_seconds as u64;
            let done = total_cs.is_some_and(|total| active.as_millis() >= total as u128 * 10);
            let stop = cancel || done || wall_limit || c.stop.load(Ordering::Relaxed);
            if wall_limit {
                ending = "wall_limit".into()
            } else if cancel {
                ending = "cancelled_tail".into()
            } else if c.stop.load(Ordering::Relaxed) {
                ending = "user_stop".into()
            }
            c.elapsed_ms
                .store(active.as_millis() as u64, Ordering::Relaxed);
            let global_cs = (active.as_secs_f64() * 100.0).round() as u64;
            let current = writer.as_mut().context("missing active segment")?;
            let boundary = start_cs + a.segment_seconds as u64 * 100;
            let checkpoint_due = last_checkpoint.elapsed() >= Duration::from_secs(5);
            if previous.is_some() && (stop || paused || active >= next || global_cs >= boundary) {
                let target = global_cs.min(boundary).saturating_sub(start_cs);
                let delay = target.saturating_sub(current.duration_cs);
                if delay > 0 {
                    current.push(
                        previous.take().context("pending frame disappeared")?,
                        delay.min(30000) as u16,
                    )?;
                    recorder.pushed(
                        (current.duration_cs - delay) * 10000,
                        current.duration_cs * 10000,
                        current.frames,
                    );
                }
            }
            if !stop && current.samples == 0 && paused {
                c.phase.store(2, Ordering::Relaxed);
                notify(c);
                std::thread::sleep(Duration::from_millis(10));
                was_paused = true;
                continue;
            }
            if cancel {
                break;
            }
            if checkpoint_due && current.samples > 0 {
                checkpoint(root, &index, number, start_cs * 10, current, &mut recorder)?;
                last_checkpoint = Instant::now();
            }
            if disk_tick.elapsed() >= Duration::from_secs(1) {
                used = store::disk_usage(root)?;
                free = store::free_space(root)?;
                disk_tick = Instant::now();
            }
            let budget = used + reserve * 2 + recorder.metadata_reserve() * 2
                >= a.total_mib * 1024 * 1024
                || free <= 1_073_741_824 + reserve;
            let bytes = current.partial_path().metadata()?.len();
            let capacity = bytes + reserve >= a.max_mib * 1024 * 1024
                || (current.frames + 2) * u64::from(w) * u64::from(h) > 1_600_000_000
                || recorder.pressure();
            let rotate = global_cs >= boundary || (capacity && previous.is_none());
            if (stop || budget || rotate) && current.samples > 0 {
                if budget {
                    ending = "disk_budget".into()
                }
                let reason = if stop || budget {
                    ending.as_str()
                } else if capacity {
                    "segment_budget"
                } else {
                    "segment_time"
                };
                checkpoint(root, &index, number, start_cs * 10, current, &mut recorder)?;
                if let Some(f) = finalizer.take() {
                    finalizer = f.poll(&mut index, root, c, &mut notify, true)?;
                }
                let old = writer.take().context("active writer disappeared")?;
                let data = recorder.rotate(&old.snapshot_stats(0), &crate::identity::new_id()?);
                let duration = old.duration_cs;
                finalizer = Some(Finalizer::start(
                    root,
                    old,
                    data,
                    number,
                    start_cs * 10,
                    reason,
                )?);
                start_cs += duration;
                if stop || budget {
                    break;
                }
                // GIF time is quantized: a boundary can round up a few ms before the wall tick.
                // Never allocate a new empty part when the total media budget is already complete.
                if total_cs.is_some_and(|total| start_cs >= total) {
                    ending = "duration".into();
                    break;
                }
                number += 1;
                if number as usize > MAX_PARTS {
                    ending = "part_limit".into();
                    break;
                }
                writer = Some(new_writer(root, &index, number, w, h)?);
                previous = None;
                next = active;
                last_checkpoint = Instant::now();
            } else if stop || budget {
                if budget {
                    ending = "disk_budget".into()
                }
                break;
            }
            if !paused && previous.is_none() && (active >= next || was_paused) {
                let at = recorder.begin();
                let current = writer.as_mut().context("writer missing during capture")?;
                current.observe_lateness(active.saturating_sub(next), period);
                let rgb = current.capture(&grab)?;
                recorder.captured(at);
                let transform = Instant::now();
                previous = Some(crate::recording::transform(rgb, region, w, h, &a.masks));
                current.observe_transform(transform.elapsed());
                c.samples.fetch_add(1, Ordering::Relaxed);
                next = if was_paused {
                    active + period
                } else {
                    next + period
                };
                if next <= active {
                    next = active + period;
                }
            }
            c.phase.store(if paused { 2 } else { 1 }, Ordering::Relaxed);
            if let Ok(mut progress) = c.series.lock() {
                *progress = Some(
                    json!({"directory":root,"index":root.join("recording.json"),"part":number,"committed_parts":index.parts.len(),"part_ms":writer.as_ref().map_or(0,|w|w.duration_cs*10),"disk_bytes":used,"checkpoint_age_ms":last_checkpoint.elapsed().as_millis(),"next_poll_ms":3000}),
                );
            }
            was_paused = paused;
            let wait = crate::recording::sample_wait(next, active, tick.elapsed(), paused);
            if wait.is_zero() {
                std::thread::yield_now()
            } else {
                std::thread::sleep(wait)
            }
        }
        Ok(())
    });
    c.phase.store(3, Ordering::Relaxed);
    notify(c);
    // Always wait for an already submitted commit, even when the next writer failed.
    if let Some(f) = finalizer.take()
        && let Err(e) = f.poll(&mut index, root, c, &mut notify, true)
    {
        index.failure = Some(format!("{e:#}").chars().take(4096).collect());
    }
    if let Err(e) = result {
        index.failure = Some(format!("{e:#}").chars().take(4096).collect());
        ending = "capture_failure".into();
        crate::runtime::report("series.capture", &e);
    }
    let incomplete = index.failure.is_some()
        || index
            .parts
            .iter()
            .any(|p| p.recording.manifest.is_none() || !p.recording.warnings.is_empty());
    index.state = if c.cancel.load(Ordering::Relaxed) {
        "cancelled"
    } else if incomplete {
        if index.parts.is_empty() {
            "failed"
        } else {
            "partial"
        }
    } else {
        "completed"
    }
    .into();
    index.reason = ending;
    if c.cancel.load(Ordering::Relaxed)
        && let Some(w) = writer.take()
    {
        let source = w.partial_path().to_path_buf();
        drop(w);
        let cp = root
            .join(".work")
            .join(store::filename(&index.prefix, number))
            .with_extension("checkpoint.json");
        if cp.exists() {
            std::fs::remove_file(cp)?;
        }
        if source.exists() {
            std::fs::remove_file(source)?;
        }
    }
    store::save_index(root, &index)?;
    Ok(store::part_summary(root, &index))
}
