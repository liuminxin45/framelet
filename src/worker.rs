//! The host owns a kill-on-close job; unsafe native capture runs in a child process.
use crate::{
    encode::Stats,
    recording::{Control, RecordArgs},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Command, Stdio},
    sync::{Arc, atomic::Ordering, mpsc},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{JobObjects::*, Threading::CREATE_NO_WINDOW},
};
pub(crate) struct Job(HANDLE);
impl Job {
    pub(crate) fn new() -> Result<Self> {
        unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            ensure!(
                !h.is_null(),
                "create worker job: {}",
                std::io::Error::last_os_error()
            );
            let job = Self(h);
            let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
                BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                    LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    ..std::mem::zeroed()
                },
                ..std::mem::zeroed()
            };
            ensure!(
                SetInformationJobObject(
                    h,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32
                ) != 0,
                "configure worker job"
            );
            Ok(job)
        }
    }
    pub(crate) fn assign(&self, child: &std::process::Child) -> Result<()> {
        ensure!(
            unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) } != 0,
            "assign worker job: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub fn supervise(a: &RecordArgs, c: &Control, external_stop: impl Fn() -> bool) -> Result<Stats> {
    crate::recording::validate(a)?;
    let value = supervise_value(
        json!({"request":a,"paused":c.paused.load(Ordering::Relaxed)}),
        a.duration + a.delay + 35.0,
        a.events,
        c,
        external_stop,
    )?;
    Ok(serde_json::from_value(value)?)
}
pub fn supervise_series(
    plan: &crate::series::Plan,
    c: &Control,
    external_stop: impl Fn() -> bool,
) -> Result<Value> {
    supervise_value(
        json!({"series":plan,"paused":c.paused.load(Ordering::Relaxed)}),
        plan.index.request.max_wall_seconds as f64 + plan.index.request.delay + 35.0,
        plan.index.request.events,
        c,
        external_stop,
    )
}
fn supervise_value(
    request: Value,
    deadline: f64,
    events: bool,
    c: &Control,
    external_stop: impl Fn() -> bool,
) -> Result<Value> {
    let job = Job::new()?;
    let executable = std::env::current_exe()?;
    let mut child = Command::new(executable)
        .arg("worker-record")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("spawn isolated recording worker")?;
    if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) } == 0 {
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("assign worker job: {}", std::io::Error::last_os_error());
    }
    let result = (|| -> Result<(Value, std::thread::JoinHandle<()>)> {
        let mut input = child.stdin.take().context("worker stdin missing")?;
        let output = child.stdout.take().context("worker stdout missing")?;
        writeln!(input, "{}", request)?;
        input.flush()?;
        let (tx, rx) = mpsc::sync_channel(4);
        let reader = std::thread::Builder::new()
            .name("framelet-worker-pipe".into())
            .spawn(move || {
                let mut reader = std::io::BufReader::new(output);
                loop {
                    let mut line = Vec::new();
                    use std::io::Read;
                    let n = reader.by_ref().take(65537).read_until(b'\n', &mut line);
                    let message = match n {
                        Ok(0) => break,
                        Ok(n) if n <= 65536 => {
                            serde_json::from_slice::<Value>(&line).map_err(|e| e.to_string())
                        }
                        _ => Err("invalid or oversized worker response".into()),
                    };
                    if tx.send(message).is_err() {
                        break;
                    }
                }
            })?;
        let began = Instant::now();
        let mut progress_tick = Instant::now();
        let mut last = Instant::now();
        let mut prior = (false, false, false);
        let mut stopping_since = None;
        let outcome: Result<Value> = loop {
            let stop = c.stop.load(Ordering::Relaxed) || external_stop();
            let cancel = c.cancel.load(Ordering::Relaxed);
            let paused = c.paused.load(Ordering::Relaxed);
            let frame = c.single.swap(false, Ordering::Relaxed);
            if (stop, cancel, paused) != prior || frame {
                writeln!(
                    input,
                    "{}",
                    json!({"stop":stop,"cancel":cancel,"paused":paused,"frame":frame})
                )?;
                input.flush()?;
                prior = (stop, cancel, paused);
            }
            if (stop || cancel) && stopping_since.is_none() {
                stopping_since = Some(Instant::now());
            }
            ensure!(
                last.elapsed() < Duration::from_secs(30),
                "worker heartbeat timeout; recording worker terminated"
            );
            ensure!(
                began.elapsed().as_secs_f64() < deadline,
                "worker wall-clock deadline exceeded"
            );
            ensure!(
                stopping_since.is_none_or(|t: Instant| t.elapsed() < Duration::from_secs(15)),
                "worker stop timeout; recording worker terminated"
            );
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(Ok(v)) => {
                    last = Instant::now();
                    if let Some(result) = v.get("completed") {
                        if events {
                            crate::runtime::progress(json!({"event":"saved","result":result}));
                        }
                        break Ok(result.clone());
                    }
                    if let Some(error) = v.get("failed") {
                        anyhow::bail!(
                            "recording worker failed: {}",
                            error.as_str().unwrap_or("unknown failure")
                        );
                    }
                    if let Some(phase) = v["phase"].as_u64() {
                        c.phase.store(phase as u8, Ordering::Relaxed);
                    }
                    if let Some(samples) = v["samples"].as_u64() {
                        c.samples.store(samples, Ordering::Relaxed);
                    }
                    if let Some(ms) = v["recorded_ms"].as_u64() {
                        c.elapsed_ms.store(ms, Ordering::Relaxed);
                    }
                    if let Some(v) = v.get("series").filter(|v| !v.is_null())
                        && let Ok(mut state) = c.series.lock()
                    {
                        *state = Some(v.clone());
                    }
                    if events && progress_tick.elapsed() >= Duration::from_secs(1) {
                        progress_tick = Instant::now();
                        crate::runtime::progress(
                            json!({"event":"progress","samples":c.samples.load(Ordering::Relaxed),"recorded_ms":c.elapsed_ms.load(Ordering::Relaxed),"phase":c.phase.load(Ordering::Relaxed)}),
                        );
                    }
                }
                Ok(Err(error)) => anyhow::bail!("{error}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => {
                    let status = child.try_wait()?;
                    anyhow::bail!("recording worker exited without a result: {status:?}");
                }
            }
        };
        drop(input);
        drop(rx);
        // Never join a pipe reader while its writer may still be alive/hung.
        // The child is terminated and reaped below before we join on success.
        Ok((outcome?, reader))
    })();
    // Always reap the child. Job close also handles host death and exceptional returns.
    let _ = child.kill();
    let _ = child.wait();
    drop(job);
    let result = result.and_then(|(value, reader)| {
        reader
            .join()
            .map_err(|_| anyhow::anyhow!("worker pipe reader panicked"))?;
        Ok(value)
    });
    if let Err(e) = &result {
        crate::runtime::report("recording.supervisor", e);
    }
    result.context(
        "recording worker failed; inspect output/pending files or recording group for recovery",
    )
}
pub fn child_main() -> Result<()> {
    let stdin = std::io::stdin();
    let mut input = std::io::BufReader::new(stdin);
    let mut first = String::new();
    use std::io::Read;
    input.by_ref().take(65537).read_line(&mut first)?;
    ensure!(first.len() <= 65536, "worker request too large");
    let value: Value = serde_json::from_str(&first)?;
    let control = Arc::new(Control::default());
    control.paused.store(
        value["paused"].as_bool().unwrap_or(false),
        Ordering::Relaxed,
    );
    let c = control.clone();
    let series_mode = value.get("series").is_some();
    std::thread::Builder::new()
        .name("framelet-worker-control".into())
        .spawn(move || {
            loop {
                let mut line = String::new();
                let result = input.by_ref().take(8193).read_line(&mut line);
                if !matches!(result,Ok(n) if n>0 && n<=8192) {
                    // Host death may close stdin just before the job terminates us.
                    // Never turn a disconnected group into explicit discard: that
                    // race could delete its last durable checkpoint before job kill.
                    if !c.stop.load(Ordering::Relaxed) && !c.cancel.load(Ordering::Relaxed) {
                        if series_mode {
                            c.stop.store(true, Ordering::Relaxed);
                        } else {
                            c.cancel.store(true, Ordering::Relaxed);
                        }
                    }
                    break;
                }
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    c.stop
                        .store(v["stop"].as_bool().unwrap_or(false), Ordering::Relaxed);
                    c.cancel
                        .store(v["cancel"].as_bool().unwrap_or(false), Ordering::Relaxed);
                    c.paused
                        .store(v["paused"].as_bool().unwrap_or(false), Ordering::Relaxed);
                    if v["frame"].as_bool().unwrap_or(false) {
                        c.single.store(true, Ordering::Relaxed);
                    }
                } else {
                    c.cancel.store(true, Ordering::Relaxed);
                    break;
                }
            }
        })?;
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut notify = |c: &Control| {
        if last.elapsed() >= Duration::from_millis(100) {
            last = Instant::now();
            let _ = writeln!(
                std::io::stdout().lock(),
                "{}",
                json!({"phase":c.phase.load(Ordering::Relaxed),"samples":c.samples.load(Ordering::Relaxed),"recorded_ms":c.elapsed_ms.load(Ordering::Relaxed),"series":c.series.lock().ok().and_then(|v|v.clone())})
            );
        }
    };
    let result = crate::runtime::guard("worker.capture", || {
        if value.get("series").is_some() {
            crate::series::run_local(
                &serde_json::from_value(value["series"].clone())?,
                &control,
                &mut notify,
            )
        } else {
            let args: RecordArgs = serde_json::from_value(value["request"].clone())?;
            Ok(serde_json::to_value(crate::recording::run_local(
                &args,
                &control,
                || false,
                &mut notify,
            )?)?)
        }
    });
    let response = match result {
        Ok(stats) => json!({"completed":stats}),
        Err(e) => {
            crate::runtime::report("worker.capture", &e);
            json!({"failed":format!("{e:#}")})
        }
    };
    writeln!(std::io::stdout().lock(), "{response}")?;
    Ok(())
}
