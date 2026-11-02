//! One persistent, kill-on-close analysis process; bounded work never owns recording controls.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, Read, Write},
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
const RESPONSE_LIMIT: u64 = 8 * 1024 * 1024;
pub fn heavy(name: &str) -> bool {
    matches!(
        name,
        "framelet_inspect"
            | "framelet_timeline"
            | "framelet_frame_at"
            | "framelet_extract_frames"
            | "framelet_read_image"
            | "framelet_edit"
            | "framelet_recover"
            | "framelet_shot"
            | "framelet_doctor"
            | "framelet_windows"
            | "framelet_monitors"
            | "framelet_probe_frames"
            | "framelet_analyze"
    )
}
pub fn failure(error: &anyhow::Error) -> Value {
    let f = crate::runtime::report("mcp.tool", error);
    json!({"content":[{"type":"text","text":f.message}],"isError":true,"structuredContent":{"ok":false,"failure":f}})
}
pub struct Work {
    pub id: Value,
    pub name: String,
    pub args: Value,
    pub cancel: Arc<AtomicBool>,
    pub progress: Option<Value>,
}
pub enum Event {
    Reply(Value, Value),
    Progress(Value, f64),
}
pub struct Service {
    tx: Option<mpsc::SyncSender<Work>>,
    pub rx: mpsc::Receiver<Event>,
    pub pending: HashMap<String, Arc<AtomicBool>>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Service {
    pub fn cancel_all(&self) {
        for cancel in self.pending.values() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn new() -> Result<Self> {
        let (tx, jobs) = mpsc::sync_channel::<Work>(2);
        let (responses, rx) = mpsc::sync_channel(8);
        let shutdown = Arc::new(AtomicBool::new(false));
        let end = shutdown.clone();
        let thread = std::thread::Builder::new()
            .name("framelet-analysis-host".into())
            .spawn(move || {
                let mut worker: Option<Process> = None;
                let mut last_used = Instant::now();
                while !end.load(Ordering::Relaxed) {
                    let work = match jobs.recv_timeout(Duration::from_millis(20)) {
                        Ok(w) => w,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if last_used.elapsed() > Duration::from_secs(30) {
                                worker.take();
                            }
                            continue;
                        }
                        Err(_) => break,
                    };
                    let outcome = crate::runtime::guard("analysis.host", || {
                        ensure!(
                            !work.cancel.load(Ordering::Relaxed),
                            "analysis cancelled before execution"
                        );
                        if worker.is_none() {
                            worker = Some(Process::new()?);
                        }
                        worker
                            .as_mut()
                            .context("analysis process unavailable")?
                            .call(&work, &end, &responses)
                    });
                    let response = match outcome {
                        Ok(v) => v,
                        Err(e) => {
                            worker.take();
                            failure(&e)
                        }
                    };
                    last_used = Instant::now();
                    if responses.send(Event::Reply(work.id, response)).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            tx: Some(tx),
            rx,
            pending: HashMap::new(),
            shutdown,
            thread: Some(thread),
        })
    }
    pub fn submit(
        &mut self,
        id: Value,
        name: String,
        args: Value,
        progress: Option<Value>,
    ) -> Result<()> {
        let key = id.to_string();
        ensure!(
            !self.pending.contains_key(&key),
            "duplicate active request ID"
        );
        let cancel = Arc::new(AtomicBool::new(false));
        self.tx
            .as_ref()
            .context("analysis service closed")?
            .try_send(Work {
                id,
                name,
                args,
                cancel: cancel.clone(),
                progress,
            })
            .map_err(|_| {
                anyhow::anyhow!("analysis queue busy; at most one active and two queued requests")
            })?;
        self.pending.insert(key, cancel);
        Ok(())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        for cancel in self.pending.values() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.tx.take();
        // Drain results while joining; a full response channel must not deadlock shutdown.
        let began = Instant::now();
        while self.thread.as_ref().is_some_and(|t| !t.is_finished())
            && began.elapsed() < Duration::from_secs(3)
        {
            while self.rx.try_recv().is_ok() {}
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(thread) = self.thread.take().filter(|t| t.is_finished()) {
            let _ = thread.join();
        }
    }
}
struct Process {
    child: Child,
    input: mpsc::SyncSender<Vec<u8>>,
    writer: Option<JoinHandle<()>>,
    rx: mpsc::Receiver<Result<Value, String>>,
    reader: Option<JoinHandle<()>>,
    _job: crate::worker::Job,
}
impl Process {
    fn new() -> Result<Self> {
        let job = crate::worker::Job::new()?;
        let mut child = Command::new(std::env::current_exe()?)
            .arg("worker-analyze")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
            .spawn()?;
        if let Err(e) = job.assign(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        let mut input = child.stdin.take().context("analysis input missing")?;
        let output = child.stdout.take().context("analysis output missing")?;
        let (tx, rx) = mpsc::sync_channel(1);
        let reader = match std::thread::Builder::new()
            .name("framelet-analysis-pipe".into())
            .spawn(move || {
                let mut input = std::io::BufReader::new(output);
                loop {
                    let mut line = Vec::new();
                    let value = match input
                        .by_ref()
                        .take(RESPONSE_LIMIT + 1)
                        .read_until(b'\n', &mut line)
                    {
                        Ok(0) => break,
                        Ok(n) if n as u64 <= RESPONSE_LIMIT => {
                            serde_json::from_slice(&line).map_err(|e| e.to_string())
                        }
                        _ => Err("analysis response exceeds protocol budget".into()),
                    };
                    if tx.send(value).is_err() {
                        break;
                    }
                }
            }) {
            Ok(reader) => reader,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.into());
            }
        };
        let (write_tx, writes) = mpsc::sync_channel::<Vec<u8>>(1);
        let writer = match std::thread::Builder::new()
            .name("framelet-analysis-write".into())
            .spawn(move || {
                while let Ok(data) = writes.recv() {
                    if input.write_all(&data).and_then(|_| input.flush()).is_err() {
                        break;
                    }
                }
            }) {
            Ok(w) => w,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.into());
            }
        };
        Ok(Self {
            child,
            input: write_tx,
            writer: Some(writer),
            rx,
            reader: Some(reader),
            _job: job,
        })
    }
    fn call(
        &mut self,
        work: &Work,
        shutdown: &AtomicBool,
        events: &mpsc::SyncSender<Event>,
    ) -> Result<Value> {
        let request = json!({"name":work.name,"arguments":work.args});
        let mut encoded = serde_json::to_vec(&request)?;
        encoded.push(b'\n');
        ensure!(encoded.len() <= 1024 * 1024, "analysis request too large");
        self.input
            .try_send(encoded)
            .map_err(|_| anyhow::anyhow!("analysis input unavailable"))?;
        let start = Instant::now();
        let mut tick = Instant::now();
        loop {
            ensure!(
                !shutdown.load(Ordering::Relaxed) && !work.cancel.load(Ordering::Relaxed),
                "analysis cancelled; inspect already published outputs before retrying"
            );
            ensure!(
                start.elapsed() < Duration::from_secs(120),
                "analysis deadline exceeded"
            );
            match self.rx.recv_timeout(Duration::from_millis(10)) {
                Ok(Ok(value)) => return Ok(value),
                Ok(Err(e)) => anyhow::bail!("{e}"),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("analysis worker exited without result")
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if tick.elapsed() >= Duration::from_secs(1) {
                tick = Instant::now();
                if let Some(token) = &work.progress {
                    let _ = events.try_send(Event::Progress(
                        token.clone(),
                        start.elapsed().as_secs_f64(),
                    ));
                }
            }
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Reader cannot retain the process; do not wait on a channel send held by a dead consumer.
        while self.rx.try_recv().is_ok() {}
        if let Some(writer) = self.writer.take().filter(|w| w.is_finished()) {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take().filter(|r| r.is_finished()) {
            let _ = reader.join();
        }
    }
}
pub fn child_main() -> Result<()> {
    let paths = crate::safety::Paths::new()?;
    let mut sessions = crate::sessions::Sessions::default();
    let mut input = std::io::BufReader::new(std::io::stdin());
    loop {
        let mut line = Vec::new();
        let n = input
            .by_ref()
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            return Ok(());
        }
        ensure!(n <= 1024 * 1024, "analysis request too large");
        let request: Value = serde_json::from_slice(&line)?;
        let result = crate::runtime::guard("analysis.tool", || {
            let name = request["name"].as_str().context("analysis name missing")?;
            ensure!(heavy(name), "operation is not an analysis tool");
            super::dispatch::call(name, &request["arguments"], &paths, &mut sessions)
        })
        .unwrap_or_else(|e| failure(&e));
        writeln!(std::io::stdout().lock(), "{result}")?;
    }
}
