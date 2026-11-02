//! Active requests wait without owning the control dispatcher or an analysis worker.
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    session_id: String,
    #[serde(default)]
    until: Until,
    #[serde(default = "timeout")]
    timeout_ms: u64,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Until {
    Ready,
    #[default]
    Terminal,
}
fn timeout() -> u64 {
    10000
}
struct Wait {
    id: Value,
    args: Args,
    began: Instant,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct Waits {
    items: HashMap<String, Wait>,
}
impl Waits {
    pub fn contains(&self, id: &Value) -> bool {
        self.items.contains_key(&id.to_string())
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn start(
        &mut self,
        id: Value,
        args: &Value,
        sessions: &mut crate::sessions::Sessions,
    ) -> Result<()> {
        ensure!(self.items.len() < 16, "at most 16 active waits");
        let args: Args = serde_json::from_value(args.clone())?;
        ensure!(
            (1..=30000).contains(&args.timeout_ms),
            "timeout_ms must be 1..30000"
        );
        sessions.status(&args.session_id)?;
        self.items.insert(
            id.to_string(),
            Wait {
                id,
                args,
                began: Instant::now(),
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(())
    }
    pub fn cancel(&self, id: &Value) {
        if let Some(w) = self.items.get(&id.to_string()) {
            w.cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn cancel_all(&self) {
        for w in self.items.values() {
            w.cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn poll(&mut self, sessions: &mut crate::sessions::Sessions) -> Vec<(Value, Value)> {
        let mut done = Vec::new();
        self.items.retain(|_,w|{
            let value=if w.cancel.load(Ordering::Relaxed){Some(super::analysis::failure(&anyhow::anyhow!("recording wait cancelled; recording state unchanged")))}else{
                match sessions.status(&w.args.session_id) {
                    Err(e)=>Some(super::analysis::failure(&e)),
                    Ok(mut status)=>{
                        let terminal=status["terminal"].as_bool()==Some(true);
                        let ready=status["ready"].as_bool()==Some(true);
                        let timed_out=w.began.elapsed()>=Duration::from_millis(w.args.timeout_ms);
                        if terminal||matches!(w.args.until,Until::Ready)&&ready||timed_out {
                            status["wait_timed_out"]=json!(timed_out&&!terminal&&!(matches!(w.args.until,Until::Ready)&&ready));
                            Some(json!({"content":[{"type":"text","text":"Recording wait completed; read structuredContent.result."}],"structuredContent":{"ok":true,"result":status},"isError":false}))
                        }else{None}
                    }
                }
            };
            if let Some(value)=value {done.push((w.id.clone(),value));false}else{true}
        });
        done
    }
}
