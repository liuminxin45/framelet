use super::catalog;
use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{Value, json};
use std::path::Path;
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
        .context("tool catalog is not an array")?
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
    if name == "framelet_timeline" {
        if let Some(v) = a.get("at") {
            anyhow::ensure!(v.is_number(), "at must be a number");
        }
        if let Some(v) = a.get("event_id") {
            anyhow::ensure!(
                v.as_u64().is_some(),
                "event_id must be a nonnegative integer"
            );
        }
        if let Some(v) = a.get("utc_us") {
            anyhow::ensure!(v.as_u64().is_some(), "utc_us must be a nonnegative integer");
        }
    }
    for key in ["cursor", "include_image"] {
        if let Some(v) = a.get(key) {
            anyhow::ensure!(v.is_boolean(), "{key} must be boolean");
        }
    }
    let mut args = a.clone();
    for key in ["offset", "limit"] {
        if let Some(v) = a.get(key) {
            anyhow::ensure!(v.as_u64().is_some(), "{key} must be a nonnegative integer");
        }
    }
    if args.get("output_dir").is_some() {
        args["output_dir"] = json!(paths.output_directory(string(&args, "output_dir")?)?);
    }
    for (key, is_output) in [("input", false), ("output", true), ("reference", false)] {
        if args.get(key).is_some() {
            args[key] = json!(if name == "framelet_recover" && key == "input" {
                paths.recovery_target(string(&args, key)?)?
            } else {
                paths.resolve(string(&args, key)?, is_output)?
            });
        }
    }
    if [
        "framelet_record",
        "framelet_start_recording",
        "framelet_start_series",
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
        args.as_object_mut()
            .context("arguments must be an object")?
            .remove("full_screen");
    }
    let a = &args;
    match name {
        "framelet_recover" => {
            let p = Path::new(string(a, "input")?);
            if crate::series::is_group(p) {
                crate::series::recover(p)
            } else {
                crate::transaction::recover(p)
            }
        }
        "framelet_start_series" => sessions.start_series(serde_json::from_value(a.clone())?),
        "framelet_recording_list" => crate::series::discover(
            paths.output_root(),
            a["offset"].as_u64().unwrap_or(0) as usize,
            a["limit"].as_u64().unwrap_or(16) as usize,
        ),
        "framelet_timeline" => crate::timeline::query(
            Path::new(string(a, "input")?),
            a.get("at").and_then(Value::as_f64),
            a.get("utc_us").and_then(Value::as_u64),
            a.get("event_id").and_then(Value::as_u64),
        ),
        "framelet_context" => Ok(paths.context()),
        "framelet_recording_status" => sessions.status(string(a, "session_id")?),
        "framelet_pause_recording" => sessions.action(string(a, "session_id")?, "pause"),
        "framelet_resume_recording" => sessions.action(string(a, "session_id")?, "resume"),
        "framelet_stop_recording" => sessions.action(string(a, "session_id")?, "stop"),
        "framelet_cancel_recording" => sessions.action(string(a, "session_id")?, "cancel"),
        "framelet_start_recording" => {
            let args: crate::recording::RecordArgs = serde_json::from_value(a.clone())?;
            anyhow::ensure!(args.duration <= 300.0, "MCP duration limit is 300 seconds");
            sessions.start(args)
        }
        "framelet_doctor" => Ok(crate::recording::doctor()),
        "framelet_monitors" => Ok(serde_json::to_value(crate::capture::monitors())?),
        "framelet_windows" => Ok(serde_json::to_value(crate::capture::windows())?),
        "framelet_record" => {
            sessions.idle()?;
            let args: crate::recording::RecordArgs = serde_json::from_value(a.clone())?;
            anyhow::ensure!(args.duration <= 300.0, "MCP duration limit is 300 seconds");
            Ok(serde_json::to_value(crate::recording::run(
                &args,
                &crate::recording::Control::default(),
                || false,
            )?)?)
        }
        "framelet_shot" => {
            crate::safety::extension(Path::new(string(a, "output")?), "png")?;
            crate::recording::shot(
                region(a)?,
                Path::new(string(a, "output")?),
                a["cursor"].as_bool().unwrap_or(false),
            )
        }
        "framelet_inspect" => {
            let p = Path::new(string(a, "input")?);
            if crate::series::is_group(p) {
                crate::series::inspect(
                    p,
                    a["offset"].as_u64().unwrap_or(0) as usize,
                    a["limit"].as_u64().unwrap_or(16) as usize,
                )
            } else {
                crate::encode::inspect(p)
            }
        }
        "framelet_frame_at" => {
            let output = a
                .get("output")
                .map(|_| string(a, "output").map(Path::new))
                .transpose()?;
            anyhow::ensure!(
                !a["include_image"].as_bool().unwrap_or(true) || output.is_some(),
                "output is required when include_image is true"
            );
            let threshold = match a.get("difference_threshold") {
                None => 24,
                Some(v) => v
                    .as_u64()
                    .and_then(|v| u8::try_from(v).ok())
                    .ok_or_else(|| {
                        anyhow::anyhow!("difference_threshold must be an integer 0..255")
                    })?,
            };
            crate::frames::at(
                Path::new(string(a, "input")?),
                number(a, "at", f64::NAN)?,
                output,
                crate::frames::ProbeOptions {
                    reference: a
                        .get("reference")
                        .map(|_| string(a, "reference").map(std::path::PathBuf::from))
                        .transpose()?,
                    region: a
                        .get("compare_region")
                        .map(|r| serde_json::from_value(r.clone()))
                        .transpose()?,
                    threshold,
                },
            )
        }
        "framelet_extract_frames" => {
            crate::frames::extract_value(&serde_json::from_value(a.clone())?)
        }
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
pub(super) fn call(
    name: &str,
    a: &Value,
    paths: &crate::safety::Paths,
    sessions: &mut crate::sessions::Sessions,
) -> Result<Value> {
    let mut value = call_value(name, a, paths, sessions)?;
    let image_path = match name {
        "framelet_shot" => value["output"].as_str(),
        "framelet_frame_at" if a["include_image"].as_bool().unwrap_or(true) => {
            value["output"].as_str()
        }
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
                } else {
                    value["preview"] = meta;
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
