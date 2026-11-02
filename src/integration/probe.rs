//! A bounded handshake with our own executable, without recording or shell execution.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Read, Write},
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};
pub fn run(entry: &Value) -> Result<Value> {
    let exe = entry["command"]
        .as_str()
        .context("integration command missing")?;
    ensure!(
        std::path::Path::new(exe).canonicalize()? == std::env::current_exe()?.canonicalize()?,
        "check must run using the configured Framelet executable; other commands are never executed"
    );
    let job = crate::worker::Job::new()?;
    let mut command = Command::new(exe);
    command
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    if let Some(env) = entry["env"].as_object() {
        for (k, v) in env {
            command.env(k, v.as_str().context("environment value must be a string")?);
        }
    }
    let mut child = command.spawn()?;
    if let Err(e) = job.assign(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let outcome = (|| -> Result<_> {
        let mut input = child.stdin.take().context("probe stdin missing")?;
        let mut output =
            std::io::BufReader::new(child.stdout.take().context("probe stdout missing")?);
        let (tx, rx) = mpsc::sync_channel(4);
        let reader = std::thread::Builder::new()
            .name("framelet-install-probe".into())
            .spawn(move || {
                loop {
                    let mut bytes = Vec::new();
                    let reply = match output
                        .by_ref()
                        .take(1024 * 1024 + 1)
                        .read_until(b'\n', &mut bytes)
                    {
                        Ok(0) => break,
                        Ok(n) if n <= 1024 * 1024 => {
                            serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())
                        }
                        _ => Err("probe response exceeds byte budget".into()),
                    };
                    if tx.send(reply).is_err() {
                        break;
                    }
                }
            })?;
        let mut responses = Vec::new();
        for (index,(method,params)) in [("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"framelet-integration-check","version":env!("CARGO_PKG_VERSION")}})),("tools/list",json!({})),("tools/call",json!({"name":"framelet_context","arguments":{}}))].into_iter().enumerate(){
            writeln!(input,"{}",json!({"jsonrpc":"2.0","id":index+1,"method":method,"params":params}))?;input.flush()?;
            let response=rx.recv_timeout(Duration::from_secs(5)).context("MCP handshake deadline exceeded")?.map_err(anyhow::Error::msg)?;
            ensure!(response["id"]==json!(index+1)&&response.get("error").is_none(),"MCP handshake returned an error");responses.push(response["result"].clone());
            if index==0{writeln!(input,"{}",json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;input.flush()?;}
        }
        ensure!(
            responses[0]["protocolVersion"] == "2025-11-25",
            "unexpected MCP protocol"
        );
        ensure!(
            responses[0]["serverInfo"]["version"] == env!("CARGO_PKG_VERSION"),
            "unexpected server version"
        );
        let tools = responses[1]["tools"]
            .as_array()
            .context("MCP tool list missing")?;
        let names: Vec<_> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        let expected: Vec<_> = crate::mcp::catalog()["tools"]
            .as_array()
            .context("catalog missing")?
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        ensure!(names == expected, "MCP tool catalog mismatch");
        ensure!(
            responses[2]["structuredContent"]["ok"] == true,
            "context check failed"
        );
        drop(input);
        drop(rx);
        Ok((
            reader,
            json!({"ok":true,"protocol":"2025-11-25","version":env!("CARGO_PKG_VERSION"),"tools":tools.len(),"capture_started":false,"transport":"stdio"}),
        ))
    })();
    let _ = child.kill();
    let _ = child.wait();
    drop(job);
    match outcome {
        Ok((reader, value)) => {
            reader
                .join()
                .map_err(|_| anyhow::anyhow!("probe reader panicked"))?;
            Ok(value)
        }
        Err(e) => Err(e),
    }
}
