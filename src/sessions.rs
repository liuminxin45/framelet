use crate::recording::{self, Control, RecordArgs};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, atomic::Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};

struct Session {
    series_root: Option<std::path::PathBuf>,
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
    pub fn start_series(&mut self, a: crate::series::SeriesArgs) -> Result<Value> {
        a.validate()?;
        self.reap();
        if let Some(root) = self.active.as_ref().and_then(|s| s.series_root.clone()) {
            let old = crate::series::read_index(&root)?;
            if a.request_id.is_some() && a.request_id == old.request_id {
                let mut normalized = a.clone();
                normalized.output_dir = a.output_dir.canonicalize()?;
                ensure!(
                    serde_json::to_value(normalized)? == serde_json::to_value(old.request)?,
                    "request_id parameters conflict"
                );
                let id = self
                    .active
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("session disappeared"))?
                    .id
                    .clone();
                return self.status(&id);
            }
        }
        self.idle()?;
        let plan = crate::series::create(&a)?;
        let id = plan.index.session_id.clone();
        if plan.index.state != "starting" || plan.index.owner_pid != std::process::id() {
            let terminal = matches!(
                plan.index.state.as_str(),
                "completed" | "partial" | "failed" | "cancelled"
            );
            return Ok(
                json!({"session_id":id,"terminal":terminal,"existing":true,"owned_by_this_connection":false,"result":crate::series::inspect(&plan.directory,0,16)?,"next":"inspect this group; control remains with its original connection, or recover after the owner exits"}),
            );
        }
        let control = Arc::new(Control::default());
        *control
            .series
            .lock()
            .map_err(|_| anyhow::anyhow!("series progress lock poisoned"))? = Some(
            json!({"directory":plan.directory,"index":plan.directory.join("recording.json"),"next_poll_ms":1000}),
        );
        let root = plan.directory.clone();
        let c = control.clone();
        let result = Arc::new(Mutex::new(None));
        let target = result.clone();
        let worker=std::thread::Builder::new().name("framelet-series".into()).spawn(move||{
            let outcome=match crate::runtime::guard("series.host",||crate::series::run(&plan,&c,||false)){
                Ok(value)=>json!({"state":value["state"],"recording":value}),
                Err(e)=>{let failure=crate::runtime::report("series.host",&e);json!({"state":"failed","directory":plan.directory,"index":plan.directory.join("recording.json"),"error":format!("{e:#}"),"failure":failure,"next":"recover the group after worker exits"})}
            };
            if let Ok(mut slot)=target.lock(){*slot=Some(outcome)}else{crate::runtime::report("series.result",&anyhow::anyhow!("result lock poisoned"));}
        })?;
        self.active = Some(Session {
            series_root: Some(root.clone()),
            id: id.clone(),
            control,
            result,
            worker,
        });
        Ok(
            json!({"session_id":id,"terminal":false,"state":"starting","directory":root,"index":root.join("recording.json"),"next":"poll status until recording; stop saves all valid parts"}),
        )
    }
    fn reap(&mut self) {
        if self.active.as_ref().is_some_and(|s| s.worker.is_finished()) {
            let Some(s) = self.active.take() else {
                return;
            };
            let joined = s.worker.join();
            let result = if joined.is_err() {
                let failure = crate::runtime::report(
                    "recording.join",
                    &anyhow::anyhow!("recording worker panicked"),
                );
                json!({"state":"failed","error":"recording worker panicked","failure":failure})
            } else {
                s.result
                    .lock()
                    .ok()
                    .and_then(|mut value| value.take())
                    .unwrap_or_else(
                        || json!({"state":"failed","error":"recording result unavailable"}),
                    )
            };
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
            let outcome=match crate::runtime::guard("recording.worker",||recording::run(&a,&c,||false)) {
                Ok(stats)=>json!({"state":"completed","recording":stats}),
                Err(e)=>{let failure=crate::runtime::report("recording.worker",&e);json!({"output":a.output,"pending":crate::transaction::pending(&a.output),"failure":failure,"state":if c.cancel.load(Ordering::Relaxed){"cancelled"}else{"failed"},"error":format!("{e:#}")})},
            };
            if let Ok(mut value)=target.lock(){*value=Some(outcome);}else{crate::runtime::report("recording.result",&anyhow::anyhow!("result lock poisoned"));}
        })?;
        self.active = Some(Session {
            series_root: None,
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
                json!({"session_id":id,"terminal":false,"state":state,"samples":s.control.samples.load(Ordering::Relaxed),"recorded_ms":s.control.elapsed_ms.load(Ordering::Relaxed),"series":s.control.series.lock().ok().and_then(|v|v.clone())}),
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
        if (action == "stop" || action == "cancel") && s.series_root.is_none() {
            let wait = Instant::now();
            while self
                .active
                .as_ref()
                .is_some_and(|s| !s.worker.is_finished())
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
            if s.series_root.is_some() {
                s.control.stop.store(true, Ordering::Relaxed);
            } else {
                s.control.cancel.store(true, Ordering::Relaxed);
            }
            let wait = Instant::now();
            while !s.worker.is_finished() && wait.elapsed() < Duration::from_secs(17) {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.reap();
    }
}
