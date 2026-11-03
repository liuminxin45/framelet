//! Bounded, acknowledged intent markers. No OS input is injected.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const LIMIT: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Begin,
    End,
    Point,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request_id: String,
    pub kind: Kind,
    #[serde(default)]
    pub label: String,
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.request_id.is_empty()
                && self.request_id.len() <= 80
                && self
                    .request_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "marker request_id must be 1..80 ASCII letters, digits, hyphens or underscores"
        );
        ensure!(
            self.label.len() <= 80 && !self.label.chars().any(char::is_control),
            "marker label must be at most 80 UTF-8 bytes without control characters"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub request: Request,
    pub state: String,
    pub observation_id: Option<u64>,
    pub utc_us: Option<u64>,
    pub mono_us: Option<u64>,
    pub error: Option<String>,
}
#[derive(Default)]
pub struct Ledger {
    entries: Vec<Receipt>,
    open: bool,
}
impl Ledger {
    pub fn existing(&self, request: &Request) -> Result<Option<Value>> {
        if let Some(old) = self
            .entries
            .iter()
            .find(|e| e.request.request_id == request.request_id)
        {
            ensure!(
                old.request == *request,
                "marker request_id parameters conflict"
            );
            return Ok(Some(serde_json::to_value(old)?));
        }
        Ok(None)
    }
    pub fn submit(&mut self, request: Request) -> Result<Value> {
        request.validate()?;
        if let Some(v) = self.existing(&request)? {
            return Ok(v);
        }
        ensure!(
            self.entries.len() < LIMIT,
            "marker limit reached (64 per recording)"
        );
        ensure!(
            self.entries
                .iter()
                .filter(|e| e.state == "queued" || e.state == "forwarded")
                .count()
                < 8,
            "marker queue full; inspect status before submitting more"
        );
        ensure!(
            request.kind != Kind::Begin || !self.open,
            "a marked interval is already open"
        );
        ensure!(
            request.kind != Kind::End || self.open,
            "end marker has no matching begin"
        );
        if request.kind == Kind::Begin {
            self.open = true;
        }
        if request.kind == Kind::End {
            self.open = false;
        }
        let entry = Receipt {
            request,
            state: "queued".into(),
            observation_id: None,
            utc_us: None,
            mono_us: None,
            error: None,
        };
        let value = serde_json::to_value(&entry)?;
        self.entries.push(entry);
        Ok(value)
    }
    pub fn drain(&mut self) -> Vec<Request> {
        self.entries
            .iter_mut()
            .filter(|e| e.state == "queued")
            .map(|e| {
                e.state = "forwarded".into();
                e.request.clone()
            })
            .collect()
    }
    pub fn finish(&mut self, request: &Request, result: Result<(u64, u64, u64)>) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.request == *request) {
            match result {
                Ok((id, utc, mono)) => {
                    entry.state = "recorded".into();
                    entry.observation_id = Some(id);
                    entry.utc_us = Some(utc);
                    entry.mono_us = Some(mono);
                }
                Err(e) => {
                    entry.state = "rejected".into();
                    entry.error = Some(format!("{e:#}").chars().take(256).collect());
                }
            }
        }
    }
    pub fn accept(&mut self, receipts: &[Receipt]) -> Result<()> {
        ensure!(receipts.len() <= LIMIT, "worker marker receipt limit");
        for remote in receipts {
            remote.request.validate()?;
            let local = self
                .entries
                .iter_mut()
                .find(|e| e.request == remote.request)
                .ok_or_else(|| anyhow::anyhow!("unknown worker marker receipt"))?;
            ensure!(
                ["queued", "forwarded", "recorded", "rejected"].contains(&remote.state.as_str())
                    && remote.error.as_ref().is_none_or(|s| s.len() <= 1024),
                "invalid worker marker receipt"
            );
            if remote.state == "recorded" {
                ensure!(
                    remote.observation_id.is_some()
                        && remote.utc_us.is_some()
                        && remote.mono_us.is_some(),
                    "incomplete marker acknowledgement"
                );
            }
            if local.state == "forwarded"
                && ["recorded", "rejected"].contains(&remote.state.as_str())
            {
                *local = remote.clone();
            }
        }
        Ok(())
    }
    pub fn terminal(&mut self, reason: &str) {
        for e in &mut self.entries {
            if e.state == "queued" || e.state == "forwarded" {
                e.state = "unconfirmed".into();
                e.error = Some(reason.chars().take(256).collect());
            }
        }
    }
    pub fn snapshot(&self) -> Value {
        json!(self.entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(id: &str, kind: Kind) -> Request {
        Request {
            request_id: id.into(),
            kind,
            label: String::new(),
        }
    }
    #[test]
    fn ordered_bounded_idempotent_and_interrupted() {
        let mut l = Ledger::default();
        assert!(l.submit(r("end", Kind::End)).is_err());
        l.submit(r("begin", Kind::Begin)).unwrap();
        assert!(l.submit(r("other", Kind::Begin)).is_err());
        assert_eq!(l.drain().len(), 1);
        l.submit(r("begin", Kind::Begin)).unwrap();
        assert!(l.submit(r("begin", Kind::Point)).is_err());
        l.submit(r("end", Kind::End)).unwrap();
        l.terminal("worker exited");
        assert!(
            l.snapshot()
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["state"] == "unconfirmed")
        );
        let mut l = Ledger::default();
        for i in 0..8 {
            l.submit(r(&format!("p{i}"), Kind::Point)).unwrap();
        }
        assert!(l.submit(r("full", Kind::Point)).is_err());
        for req in l.drain() {
            l.finish(&req, Ok((1, 2, 3)));
        }
        assert_eq!(l.snapshot()[0]["state"], "recorded");
        assert!(r("bad\n", Kind::Point).validate().is_err());
        for i in 8..LIMIT {
            let request = r(&format!("p{i}"), Kind::Point);
            l.submit(request.clone()).unwrap();
            l.drain();
            l.finish(&request, Ok((i as u64, 2, 3)));
        }
        assert!(l.submit(r("over-limit", Kind::Point)).is_err());
    }
    #[test]
    fn native_ack_cannot_invent_or_regress_an_anchor() {
        let request = r("one", Kind::Point);
        let mut parent = Ledger::default();
        parent.submit(request.clone()).unwrap();
        parent.drain();
        let mut child = Ledger::default();
        child.submit(request.clone()).unwrap();
        child.drain();
        child.finish(&request, Ok((5, 100, 20)));
        let ack: Vec<Receipt> = serde_json::from_value(child.snapshot()).unwrap();
        parent.accept(&ack).unwrap();
        assert_eq!(parent.snapshot()[0]["observation_id"], 5);
        parent.terminal("ended");
        assert_eq!(parent.snapshot()[0]["state"], "recorded");
        let mut bad = ack.clone();
        bad[0].request.request_id = "foreign".into();
        assert!(parent.accept(&bad).is_err());
        let mut bad = ack;
        bad[0].mono_us = None;
        assert!(parent.accept(&bad).is_err());
    }
}
