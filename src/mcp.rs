use anyhow::Result;
use base64::Engine;
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Read, Write},
    path::Path,
};

pub fn catalog() -> Value {
    let region = json!({"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"},"width":{"type":"integer","minimum":1},"height":{"type":"integer","minimum":1}},"required":["x","y","width","height"],"additionalProperties":false});
    let mut tools = json!([
        {"name":"framelet_doctor","description":"Check local screen capture readiness. No network.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"framelet_monitors","description":"List monitors in physical screen pixels, including negative coordinates.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"framelet_windows","description":"List visible windows, titles and screen bounds. Captures visible pixels, not occluded window contents.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"framelet_record","description":"Record visible screen pixels into a GIF; blocking, maximum 300 seconds. Output must not exist. No audio.","inputSchema":{"type":"object","properties":{"output":{"type":"string"},"region":region,"duration":{"type":"number","exclusiveMinimum":0,"maximum":300,"default":5},"fps":{"type":"integer","minimum":1,"maximum":50,"default":10},"cursor":{"type":"boolean","default":false},"quality":{"type":"integer","minimum":1,"maximum":30,"default":10}},"required":["output"],"additionalProperties":false}},
        {"name":"framelet_shot","description":"Capture visible screen pixels as a PNG. Output must not exist.","inputSchema":{"type":"object","properties":{"output":{"type":"string"},"region":region,"cursor":{"type":"boolean"}},"required":["output"],"additionalProperties":false}},
        {"name":"framelet_inspect","description":"Read GIF size, frame count and duration.","inputSchema":{"type":"object","properties":{"input":{"type":"string"}},"required":["input"],"additionalProperties":false}},
        {"name":"framelet_edit","description":"Trim a GIF and change playback speed. Seconds; output must not exist.","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"output":{"type":"string"},"start":{"type":"number","minimum":0,"default":0},"end":{"type":"number","exclusiveMinimum":0},"speed":{"type":"number","minimum":0.01,"maximum":100,"default":1}},"required":["input","output"],"additionalProperties":false}},
        {"name":"framelet_extract_frames","description":"Extract composited PNG frames from a GIF, return a contact-sheet image and timestamp manifest. Scene mode ranks visual change, NOT semantic importance. Review images then use framelet_edit to cut important ranges. Output directory must not exist.","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"output":{"type":"string"},"count":{"type":"integer","minimum":1,"maximum":64,"default":8},"mode":{"type":"string","enum":["uniform","scene"],"default":"scene"},"min_gap":{"type":"number","minimum":0,"maximum":3600,"default":0.5},"min_change":{"type":"number","minimum":0,"maximum":1,"default":0.04},"padding":{"type":"number","minimum":0,"maximum":60,"default":0.75}},"required":["input","output"],"additionalProperties":false}},
        {"name":"framelet_read_image","description":"Read a selected PNG frame as an MCP image block, resized to at most 960px and 1 MiB. Use after reviewing an extraction contact sheet. Local PNG only.","inputSchema":{"type":"object","properties":{"input":{"type":"string"}},"required":["input"],"additionalProperties":false}}
    ]);
    let list = tools.as_array_mut().unwrap();
    let props = json!({"max_width":{"type":"integer","minimum":1,"maximum":8192},"masks":{"type":"array","maxItems":32,"items":region},"delay":{"type":"number","minimum":0,"maximum":30,"default":0},"max_mib":{"type":"integer","minimum":1,"maximum":1024,"default":256},"full_screen":{"type":"boolean","default":false}});
    for t in list.iter_mut().filter(|t| t["name"] == "framelet_record") {
        t["inputSchema"]["properties"]
            .as_object_mut()
            .unwrap()
            .extend(props.as_object().unwrap().clone());
        t["description"] = json!(
            "Blocking GIF capture, maximum 300 seconds. Prefer start_recording for UI workflows. Explicit region or full_screen:true required. Output is inside the configured root; never overwrite."
        );
    }
    list.iter_mut()
        .find(|t| t["name"] == "framelet_shot")
        .unwrap()["inputSchema"]["properties"]["full_screen"] =
        json!({"type":"boolean","default":false});
    let mut start = list
        .iter()
        .find(|t| t["name"] == "framelet_record")
        .unwrap()
        .clone();
    start["name"] = json!("framelet_start_recording");
    start["description"] = json!(
        "Start a bounded background GIF recording, return session_id immediately. Poll recording_status until recording, then operate the UI. Pause excludes waiting time; stop saves; cancel discards unfinished output. Only one recording per MCP server. EOF cancels active recording."
    );
    list.push(start);
    list.push(json!({"name":"framelet_frame_at","description":"Return one composited PNG and an MCP image block at a GIF playback timestamp in seconds. Use midpoint probes to bisect a known static-to-action boundary instead of sending every frame. GIF prefix decoding is still sequential; only image/token output is reduced. Output must be a new .png inside output_root.","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"at":{"type":"number","minimum":0,"maximum":3600},"output":{"type":"string"}},"required":["input","at","output"],"additionalProperties":false}}));
    list.push(json!({"name":"framelet_context","description":"Discover executable path, working directory, allowed output/read roots and the recommended recording workflow. Call first; relative paths resolve under output_root.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}));
    for (name, description) in [
        (
            "framelet_recording_status",
            "Read recording state, sampled frames and captured time. Terminal result contains saved GIF stats or error.",
        ),
        (
            "framelet_pause_recording",
            "Pause sampling; paused time is excluded from the GIF, while the wall-clock deadline remains active.",
        ),
        ("framelet_resume_recording", "Resume a paused recording."),
        (
            "framelet_stop_recording",
            "Stop and finalize the GIF. Waits up to 10 seconds; poll status if still finalizing. Safe to repeat.",
        ),
        (
            "framelet_cancel_recording",
            "Cancel and discard only this session's unfinished output. Completed files are never deleted. Safe to repeat.",
        ),
    ] {
        list.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":{"session_id":{"type":"string","maxLength":80}},"required":["session_id"],"additionalProperties":false}}));
    }
    json!({"schema_version":"1.3","name":"framelet","version":env!("CARGO_PKG_VERSION"),"tools":tools})
}
fn string<'a>(a: &'a Value, k: &str) -> Result<&'a str> {
    a.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing string: {k}"))
}
fn number(a: &Value, k: &str, d: f64) -> Result<f64> {
    match a.get(k) {
        None => Ok(d),
        Some(v) => v
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("invalid number: {k}")),
    }
}
fn region(a: &Value) -> Result<Option<crate::capture::Region>> {
    a.get("region")
        .map(|r| serde_json::from_value(r.clone()))
        .transpose()
        .map_err(Into::into)
}
fn call_value(
    name: &str,
    a: &Value,
    paths: &crate::safety::Paths,
    sessions: &mut crate::sessions::Sessions,
) -> Result<Value> {
    let cat = catalog();
    let tool = cat["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .ok_or_else(|| anyhow::anyhow!("unknown tool: {name}"))?;
    let obj = a
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("arguments must be an object"))?;
    for k in obj.keys() {
        anyhow::ensure!(
            tool["inputSchema"]["properties"].get(k).is_some(),
            "unknown argument: {k}"
        );
    }
    if let Some(v) = a.get("cursor") {
        anyhow::ensure!(v.is_boolean(), "cursor must be boolean");
    }
    let mut args = a.clone();
    for (key, is_output) in [("input", false), ("output", true)] {
        if args.get(key).is_some() {
            args[key] = json!(paths.resolve(string(&args, key)?, is_output)?);
        }
    }
    if [
        "framelet_record",
        "framelet_start_recording",
        "framelet_shot",
    ]
    .contains(&name)
    {
        let full = match args.get("full_screen") {
            None => false,
            Some(v) => v
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("full_screen must be boolean"))?,
        };
        anyhow::ensure!(
            args.get("region").is_some_and(|v| !v.is_null()) ^ full,
            "choose exactly one explicit region or full_screen:true"
        );
        args.as_object_mut().unwrap().remove("full_screen");
    }
    let a = &args;
    match name {
        "framelet_context" => Ok(paths.context()),
        "framelet_recording_status" => sessions.status(string(a, "session_id")?),
        "framelet_pause_recording" => sessions.action(string(a, "session_id")?, "pause"),
        "framelet_resume_recording" => sessions.action(string(a, "session_id")?, "resume"),
        "framelet_stop_recording" => sessions.action(string(a, "session_id")?, "stop"),
        "framelet_cancel_recording" => sessions.action(string(a, "session_id")?, "cancel"),
        "framelet_start_recording" => {
            let args: crate::cli::RecordArgs = serde_json::from_value(a.clone())?;
            anyhow::ensure!(args.duration <= 300.0, "MCP duration limit is 300 seconds");
            sessions.start(args)
        }
        "framelet_doctor" => Ok(crate::cli::doctor()),
        "framelet_monitors" => Ok(serde_json::to_value(crate::capture::monitors())?),
        "framelet_windows" => Ok(serde_json::to_value(crate::capture::windows())?),
        "framelet_record" => {
            sessions.idle()?;
            let args: crate::cli::RecordArgs = serde_json::from_value(a.clone())?;
            anyhow::ensure!(args.duration <= 300.0, "MCP duration limit is 300 seconds");
            Ok(serde_json::to_value(crate::cli::record(&args, None)?)?)
        }
        "framelet_shot" => {
            crate::safety::extension(Path::new(string(a, "output")?), "png")?;
            crate::cli::shot(
                region(a)?,
                Path::new(string(a, "output")?),
                a["cursor"].as_bool().unwrap_or(false),
            )
        }
        "framelet_inspect" => crate::encode::inspect(Path::new(string(a, "input")?)),
        "framelet_frame_at" => crate::frames::at(
            Path::new(string(a, "input")?),
            number(a, "at", f64::NAN)?,
            Path::new(string(a, "output")?),
        ),
        "framelet_extract_frames" => Ok(serde_json::to_value(crate::frames::extract(
            &serde_json::from_value(a.clone())?,
        )?)?),
        "framelet_read_image" => Ok(json!({"input":string(a,"input")?})),
        "framelet_edit" => Ok(serde_json::to_value(crate::encode::edit(
            Path::new(string(a, "input")?),
            Path::new(string(a, "output")?),
            number(a, "start", 0.0)?,
            a.get("end").map(|_| number(a, "end", 0.0)).transpose()?,
            number(a, "speed", 1.0)?,
        )?)?),
        _ => anyhow::bail!("unknown tool"),
    }
}
fn call(
    name: &str,
    a: &Value,
    paths: &crate::safety::Paths,
    sessions: &mut crate::sessions::Sessions,
) -> Result<Value> {
    let mut value = call_value(name, a, paths, sessions)?;
    let image_path = match name {
        "framelet_shot" => value["output"].as_str(),
        "framelet_frame_at" => value["output"].as_str(),
        "framelet_extract_frames" => value["contact_sheet"].as_str(),
        "framelet_read_image" => value["input"].as_str(),
        _ => None,
    }
    .map(str::to_owned);
    let mut content = Vec::new();
    if let Some(path) = image_path {
        match crate::frames::preview_png(Path::new(&path), 960) {
            Ok((meta, png)) => {
                if name == "framelet_read_image" {
                    value = meta;
                }
                content.push(json!({"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(png)}));
            }
            Err(error) if name != "framelet_read_image" => {
                value["preview_error"] = json!(error.to_string());
            }
            Err(error) => return Err(error),
        }
    }
    let result = json!({"ok":true,"result":value});
    content.insert(
        0,
        json!({"type":"text","text":serde_json::to_string(&result)?}),
    );
    Ok(json!({"content":content,"structuredContent":result,"isError":false}))
}
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
        if id.is_none() {
            continue;
        }
        let method = request["method"].as_str().unwrap_or("");
        let response = match method {
            "initialize" => {
                let requested = request["params"]["protocolVersion"].as_str().unwrap_or("");
                let version = if ["2024-11-05", "2025-03-26", "2025-06-18"].contains(&requested) {
                    requested
                } else {
                    "2025-03-26"
                };
                json!({"result":{"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"framelet","version":env!("CARGO_PKG_VERSION")},"instructions":"Framelet runs locally on Windows with an interactive desktop. Call framelet_context first to discover paths. Use windows/monitors to choose a region. For UI demos use start_recording, poll until recording, operate your browser/app, then stop_recording. Relative paths use output_root. Never capture secrets; masks are fixed rectangles relative to the selected region, not automatic detection. No capture starts during initialization."}})
            }
            "ping" => json!({"result":{}}),
            "tools/list" => json!({"result":{"tools":catalog()["tools"]}}),
            "tools/call" => {
                let p = &request["params"];
                match call(
                    p["name"].as_str().unwrap_or(""),
                    p.get("arguments").unwrap_or(&json!({})),
                    &paths,
                    &mut sessions,
                ) {
                    Ok(v) => json!({"result":v}),
                    Err(e) => {
                        json!({"result":{"content":[{"type":"text","text":format!("{e:#}")}],"isError":true}})
                    }
                }
            }
            _ => json!({"error":{"code":-32601,"message":"Method not found"}}),
        };
        let mut response = response;
        response["jsonrpc"] = json!("2.0");
        response["id"] = id.unwrap();
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    Ok(())
}
