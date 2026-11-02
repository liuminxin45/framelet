mod catalog;
mod dispatch;
use anyhow::Result;
pub use catalog::catalog;
use dispatch::call;
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};
pub fn serve() -> Result<()> {
    // MCP stdio transport uses one JSON-RPC object per line. No logs on stdout.
    let paths = crate::safety::Paths::new()?;
    let mut sessions = crate::sessions::Sessions::default();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut output = io::stdout().lock();
    loop {
        let mut buf = Vec::new();
        let n = input
            .by_ref()
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        if n > 1024 * 1024 {
            // Drain the rest without allocating an unbounded request; preserve framing.
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
            writeln!(
                output,
                "{}",
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Request exceeds 1 MiB"}})
            )?;
            output.flush()?;
            continue;
        }
        let request: Value = match serde_json::from_slice(&buf) {
            Ok(v) => v,
            Err(_) => {
                writeln!(
                    output,
                    "{}",
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}})
                )?;
                output.flush()?;
                continue;
            }
        };
        let valid = request.is_object()
            && request["jsonrpc"] == "2.0"
            && request["method"].is_string()
            && request
                .get("id")
                .is_none_or(|v| v.is_string() || v.is_i64() || v.is_u64())
            && request.get("params").is_none_or(Value::is_object);
        if !valid {
            writeln!(
                output,
                "{}",
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid Request"}})
            )?;
            output.flush()?;
            continue;
        }
        let id = request.get("id").cloned();
        let Some(id) = id else {
            continue;
        };
        let method = request["method"].as_str().unwrap_or("");
        let response = match method {
            "initialize" => {
                let requested = request["params"]["protocolVersion"].as_str().unwrap_or("");
                let version = if ["2024-11-05", "2025-03-26", "2025-06-18"].contains(&requested) {
                    requested
                } else {
                    "2025-03-26"
                };
                json!({"result":{"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"framelet","version":env!("CARGO_PKG_VERSION")},"instructions":"Framelet runs locally on Windows with an interactive desktop. Call framelet_context first to discover paths, documentation and capture_guidance FPS presets. Recording saves GIF + key/mouse/frame timeline + hash-bound capture manifest by default. Query framelet_timeline with event_id, at or utc_us for unique event/sample/frame IDs and offsets. Use framelet_recover after an interrupted worker exits. Recording runs in a supervised child process. Use windows/monitors to choose a region. For UI demos use start_recording, poll until recording, operate your browser/app, then stop_recording. Relative paths use output_root. Never capture secrets; masks are fixed rectangles relative to the selected region, not automatic detection. No capture starts during initialization. For efficient GIF editing: inspect duration; save opening/closing anchor PNGs with frame_at; bisect known single-action boundaries using reference + a stable compare_region + include_image:false (output optional). Use returned frame intervals to skip repeated probes, then visually review boundary images and trim with context. For unknown/multiple actions, first extract a small overview. See framelet_context editing_recipe."}})
            }
            "ping" => json!({"result":{}}),
            "tools/list" => json!({"result":{"tools":catalog()["tools"]}}),
            "tools/call" => {
                let p = &request["params"];
                match crate::runtime::guard("mcp.tool", || {
                    call(
                        p["name"].as_str().unwrap_or(""),
                        p.get("arguments").unwrap_or(&json!({})),
                        &paths,
                        &mut sessions,
                    )
                }) {
                    Ok(v) => json!({"result":v}),
                    Err(e) => {
                        let failure = crate::runtime::report("mcp.tool", &e);
                        json!({"result":{"content":[{"type":"text","text":failure.message}],"isError":true,"structuredContent":{"ok":false,"failure":failure}}})
                    }
                }
            }
            _ => json!({"error":{"code":-32601,"message":"Method not found"}}),
        };
        let mut response = response;
        response["jsonrpc"] = json!("2.0");
        response["id"] = id;
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    Ok(())
}
