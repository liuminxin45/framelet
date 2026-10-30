use crate::{
    cli::RecordArgs,
    recording::{self, Control},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, atomic::Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};

struct Session {
    id: String,
    control: Arc<Control>,
    result: Arc<Mutex<Option<Value>>>,
    worker: JoinHandle<()>,
}
#[derive(Default)]
pub struct Sessions {
    active: Option<Session>,
    history: VecDeque<Value>,
    counter: u64,
}
impl Sessions {
    fn reap(&mut self) {
        if self.active.as_ref().is_some_and(|s| s.worker.is_finished()) {
            let s = self.active.take().unwrap();
            let _ = s.worker.join();
            let result = s.result.lock().unwrap().clone().unwrap_or_else(
                || json!({"state":"failed","error":"recording worker ended unexpectedly"}),
            );
            self.history
                .push_back(json!({"session_id":s.id,"terminal":true,"result":result}));
            if self.history.len() > 16 {
                self.history.pop_front();
            }
        }
    }
    pub fn idle(&mut self) -> Result<()> {
        self.reap();
        ensure!(
            self.active.is_none(),
            "a recording is active; stop or cancel it first"
        );
        Ok(())
    }
    pub fn start(&mut self, a: RecordArgs) -> Result<Value> {
        self.idle()?;
        recording::validate(&a)?;
        self.counter += 1;
        let id = format!("{}-{}", std::process::id(), self.counter);
        let control = Arc::new(Control::default());
        let c = control.clone();
        let result = Arc::new(Mutex::new(None));
        let target = result.clone();
        let worker=std::thread::Builder::new().name("framelet-capture".into()).spawn(move||{
            let outcome=match recording::run(&a,&c,||false) {
                Ok(stats)=>json!({"state":"completed","recording":stats}),
                Err(e)=>json!({"state":if c.cancel.load(Ordering::Relaxed){"cancelled"}else{"failed"},"error":format!("{e:#}")}),
            };
            *target.lock().unwrap()=Some(outcome);
        })?;
        self.active = Some(Session {
            id: id.clone(),
            control,
            result,
            worker,
        });
        Ok(
            json!({"session_id":id,"state":"starting","terminal":false,"next":"poll recording_status until recording, then operate the UI; stop_recording saves; cancel_recording discards unfinished output"}),
        )
    }
    pub fn status(&mut self, id: &str) -> Result<Value> {
        self.reap();
        if let Some(s) = self.active.as_ref().filter(|s| s.id == id) {
            let state = match s.control.phase.load(Ordering::Relaxed) {
                0 => "starting",
                1 => "recording",
                2 => "paused",
                _ => "finalizing",
            };
            return Ok(
                json!({"session_id":id,"terminal":false,"state":state,"samples":s.control.samples.load(Ordering::Relaxed),"recorded_ms":s.control.elapsed_ms.load(Ordering::Relaxed)}),
            );
        }
        self.history
            .iter()
            .find(|s| s["session_id"] == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown or expired session_id"))
    }
    pub fn action(&mut self, id: &str, action: &str) -> Result<Value> {
        self.reap();
        // Terminal stop/cancel is idempotent; never delete an already published recording.
        if self.history.iter().any(|s| s["session_id"] == id) {
            return self.status(id);
        }
        let s = self
            .active
            .as_ref()
            .filter(|s| s.id == id)
            .ok_or_else(|| anyhow::anyhow!("unknown session_id"))?;
        match action {
            "pause" => s.control.paused.store(true, Ordering::Relaxed),
            "resume" => s.control.paused.store(false, Ordering::Relaxed),
            "stop" => s.control.stop.store(true, Ordering::Relaxed),
            "cancel" => s.control.cancel.store(true, Ordering::Relaxed),
            _ => anyhow::bail!("unknown action"),
        }
        if action == "stop" || action == "cancel" {
            let wait = Instant::now();
            while !self.active.as_ref().unwrap().worker.is_finished()
                && wait.elapsed() < Duration::from_secs(10)
            {
                std::thread::sleep(Duration::from_millis(10));
            }
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
        self.status(id)
    }
}
impl Drop for Sessions {
    fn drop(&mut self) {
        if let Some(s) = &self.active {
            s.control.cancel.store(true, Ordering::Relaxed);
            let wait = Instant::now();
            while !s.worker.is_finished() && wait.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.reap();
    }
}
