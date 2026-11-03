pub(crate) mod analysis;
mod catalog;
mod dispatch;
mod resources;
mod wait;
use anyhow::Result;
pub use catalog::catalog;
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Read, Write},
    sync::{atomic::Ordering, mpsc},
    time::Duration,
};
enum Input {
    Request(Vec<u8>),
    Oversized,
    Failed(String),
    Eof,
}
fn input_reader(tx: mpsc::SyncSender<Input>) {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let result = (|| -> Result<()> {
        loop {
            let mut buf = Vec::new();
            let n = input
                .by_ref()
                .take(1024 * 1024 + 1)
                .read_until(b'\n', &mut buf)?;
            if n == 0 {
                let _ = tx.send(Input::Eof);
                return Ok(());
            }
            if n > 1024 * 1024 {
                if buf.last() != Some(&b'\n') {
                    loop {
                        let chunk = input.fill_buf()?;
                        if chunk.is_empty() {
                            break;
                        }
                        let end = chunk.iter().position(|b| *b == b'\n');
                        let used = end.map_or(chunk.len(), |i| i + 1);
                        input.consume(used);
                        if end.is_some() {
                            break;
                        }
                    }
                }
                if tx.send(Input::Oversized).is_err() {
                    return Ok(());
                }
            } else if tx.send(Input::Request(buf)).is_err() {
                return Ok(());
            }
        }
    })();
    if let Err(e) = result {
        let _ = tx.send(Input::Failed(e.to_string()));
    }
}
fn error(code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":null,"error":{"code":code,"message":message}})
}
fn request(
    request: Value,
    paths: &crate::safety::Paths,
    sessions: &mut crate::sessions::Sessions,
    analysis: &mut analysis::Service,
    waits: &mut wait::Waits,
) -> Option<Value> {
    let valid = request.is_object()
        && request["jsonrpc"] == "2.0"
        && request["method"].is_string()
        && request
            .get("id")
            .is_none_or(|v| v.is_string() || v.is_i64() || v.is_u64())
        && request.get("params").is_none_or(Value::is_object);
    if !valid {
        return Some(error(-32600, "Invalid Request"));
    }
    let method = request["method"].as_str().unwrap_or("");
    if request.get("id").is_none() {
        if method == "notifications/cancelled" {
            waits.cancel(&request["params"]["requestId"]);
            if let Some(cancel) = analysis
                .pending
                .get(&request["params"]["requestId"].to_string())
            {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        return None;
    }
    let id = request["id"].clone();
    if analysis.pending.contains_key(&id.to_string()) || waits.contains(&id) {
        let mut v = error(-32600, "Duplicate active request ID");
        v["id"] = id;
        return Some(v);
    }
    if method == "tools/call" && request["params"]["name"] == "framelet_wait_recording" {
        return waits
            .start(
                id.clone(),
                request["params"].get("arguments").unwrap_or(&json!({})),
                sessions,
            )
            .err()
            .map(|e| json!({"jsonrpc":"2.0","id":id,"result":analysis::failure(&e)}));
    }
    if method == "tools/call" && analysis::heavy(request["params"]["name"].as_str().unwrap_or("")) {
        let p = &request["params"];
        let progress = p["_meta"]["progressToken"]
            .as_str()
            .filter(|v| v.len() <= 256)
            .map(|_| p["_meta"]["progressToken"].clone())
            .or_else(|| {
                p["_meta"]["progressToken"]
                    .is_number()
                    .then(|| p["_meta"]["progressToken"].clone())
            });
        let result = analysis.submit(
            id.clone(),
            p["name"].as_str().unwrap_or("").to_owned(),
            p.get("arguments").cloned().unwrap_or_else(|| json!({})),
            progress,
        );
        return result
            .err()
            .map(|e| json!({"jsonrpc":"2.0","id":id,"result":analysis::failure(&e)}));
    }
    let response = match method {
        "initialize" => {
            let version = "2025-11-25";
            json!({"result":{"protocolVersion":version,"capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"framelet","version":env!("CARGO_PKG_VERSION")},"instructions":"Framelet runs locally on Windows with an interactive desktop. Call framelet_context first to discover paths, documentation and capture_guidance FPS presets. Recording saves GIF + key/mouse/frame timeline + hash-bound capture manifest by default. Query framelet_timeline with event_id, at or utc_us for unique event/sample/frame IDs and offsets. Use framelet_recover after an interrupted worker exits. Recording runs in a supervised child process. Use windows/monitors to choose a region. For human-facing demos explicitly choose 30 FPS. Prepare the app before start_recording, wait_recording until ready, operate your browser/app, stop promptly after the intended response settles, then wait_recording until terminal. Native mark_recording begin/end/point anchors never inject input and require recorded acknowledgement in status. After save analyze quality and prepare_demo a separate candidate; batch-probe opening, every main action and closing result. Saved is not visually approved; never deliver an unreviewed raw demo merely because capture succeeded. Waits and analysis never own the control loop. Relative paths use output_root. Never capture secrets; masks are fixed rectangles relative to the selected region, not automatic detection. No capture starts during initialization. For efficient GIF editing: query filtered timeline events first, analyze once for candidate intervals, then probe_frames with multiple timestamps in one pass. Default to numeric structured output; request a contact sheet only for visual review. Bisect only a known single monotonic boundary with a fixed reference/compare_region, and skip repeated frame intervals. Review opening and closing context before editing. See framelet_context efficient_review and docs/performance.md."}})
        }
        "ping" => json!({"result":{}}),
        "resources/list" => json!({"result":resources::list()}),
        "resources/templates/list" => json!({"result":{"resourceTemplates":[]}}),
        "resources/read" => {
            match resources::read(request["params"]["uri"].as_str().unwrap_or("")) {
                Some(value) => json!({"result":value}),
                None => json!({"error":{"code":-32602,"message":"Unknown Framelet guide URI"}}),
            }
        }
        "tools/list" => json!({"result":{"tools":catalog()["tools"]}}),
        "tools/call" => {
            let p = &request["params"];
            match crate::runtime::guard("mcp.tool", || {
                dispatch::call(
                    p["name"].as_str().unwrap_or(""),
                    p.get("arguments").unwrap_or(&json!({})),
                    paths,
                    sessions,
                )
            }) {
                Ok(v) => json!({"result":v}),
                Err(e) => json!({"result":analysis::failure(&e)}),
            }
        }
        _ => json!({"error":{"code":-32601,"message":"Method not found"}}),
    };
    let mut response = response;
    response["jsonrpc"] = json!("2.0");
    response["id"] = id;
    Some(response)
}
pub fn serve() -> Result<()> {
    let paths = crate::safety::Paths::new()?;
    let mut sessions = crate::sessions::Sessions::default();
    let mut analysis = analysis::Service::new()?;
    let mut waits = wait::Waits::default();
    let (tx, rx) = mpsc::sync_channel(8);
    std::thread::Builder::new()
        .name("framelet-mcp-input".into())
        .spawn(move || input_reader(tx))?;
    let mut output = io::stdout().lock();
    let mut eof = false;
    loop {
        for (id, result) in waits.poll(&mut sessions) {
            writeln!(
                output,
                "{}",
                json!({"jsonrpc":"2.0","id":id,"result":result})
            )?;
            output.flush()?;
        }
        while let Ok(event) = analysis.rx.try_recv() {
            let value = match event {
                analysis::Event::Reply(id, result) => {
                    analysis.pending.remove(&id.to_string());
                    json!({"jsonrpc":"2.0","id":id,"result":result})
                }
                analysis::Event::Progress(token, elapsed) => {
                    json!({"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":token,"progress":elapsed,"message":"analysis elapsed seconds"}})
                }
            };
            writeln!(output, "{value}")?;
            output.flush()?;
        }
        if eof && analysis.pending.is_empty() && waits.is_empty() {
            return Ok(());
        }
        if eof {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        let response = match rx.recv_timeout(Duration::from_millis(2)) {
            Ok(Input::Request(buf)) => match serde_json::from_slice(&buf) {
                Ok(v) => request(v, &paths, &mut sessions, &mut analysis, &mut waits),
                Err(_) => Some(error(-32700, "Parse error")),
            },
            Ok(Input::Oversized) => Some(error(-32600, "Request exceeds 1 MiB")),
            Ok(Input::Failed(e)) => return Err(anyhow::anyhow!(e)),
            Ok(Input::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                eof = true;
                waits.cancel_all();
                analysis.cancel_all();
                drop(std::mem::take(&mut sessions));
                None
            }
            Err(mpsc::RecvTimeoutError::Timeout) => None,
        };
        if let Some(value) = response {
            writeln!(output, "{value}")?;
            output.flush()?;
        }
    }
}
