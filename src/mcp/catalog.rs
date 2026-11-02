use serde_json::{Value, json};
pub fn catalog() -> Value {
    let region = json!({"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"},"width":{"type":"integer","minimum":1},"height":{"type":"integer","minimum":1}},"required":["x","y","width","height"],"additionalProperties":false});
    let mut tools = json!([
        {"name":"framelet_doctor","description":"Check local screen capture readiness. No network.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"framelet_monitors","description":"List monitors in physical screen pixels, including negative coordinates.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"framelet_windows","description":"List visible windows, titles and screen bounds. Captures visible pixels, not occluded window contents.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"framelet_record","description":"Record visible screen pixels into a GIF; blocking, maximum 300 seconds. Output must not exist. No audio.","inputSchema":{"type":"object","properties":{"output":{"type":"string"},"region":region,"duration":{"type":"number","exclusiveMinimum":0,"maximum":300,"default":5},"fps":{"type":"integer","minimum":1,"maximum":60,"default":10},"cursor":{"type":"boolean","default":false},"quality":{"type":"integer","minimum":1,"maximum":30,"default":10}},"required":["output"],"additionalProperties":false}},
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
    list.push(json!({"name":"framelet_frame_at","description":"Probe one GIF playback timestamp. By default return a composited PNG and image block (output required). For efficient bisection use reference PNG + compare_region + include_image:false; output may then be omitted, returning numeric region difference and frame interval without writing PNG or sending an image. Cache intervals to skip repeated frames. GIF decoding is still sequential. Not semantic highlight detection.","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"at":{"type":"number","minimum":0,"maximum":3600},"output":{"type":"string"},"reference":{"type":"string"},"compare_region":region,"difference_threshold":{"type":"integer","minimum":0,"maximum":255,"default":24},"include_image":{"type":"boolean","default":true}},"required":["input","at"],"additionalProperties":false}}));
    list.push(json!({"name":"framelet_context","description":"Discover executable path, working directory, allowed output/read roots and the recommended recording workflow/FPS presets. Call first; relative paths resolve under output_root.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}));
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
    list.push(json!({"name":"framelet_timeline","description":"Query adjacent recording timeline: original UTC microseconds, capture intervals, region-relative keyboard down/up, mouse clicks, globally unique observation IDs, sample/frame links, time offsets and positional dwell candidates. Default returns a bounded overview; at selects GIF seconds; utc_us selects original computer time. Use frame_at on mapped media time. Recordings capture this by default, including during low-FPS recording. No semantic UI target inference.","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"at":{"type":"number","minimum":0},"utc_us":{"type":"integer","minimum":0},"event_id":{"type":"integer","minimum":0}},"required":["input"],"additionalProperties":false}}));
    list.push(json!({"name":"framelet_recover","description":"Verify a completed capture or finalize a pending GIF/timeline pair after its worker has exited. Never invent missing events or delete incomplete data. The path and derived files must remain in output_root; an absent GIF reports incomplete data.","inputSchema":{"type":"object","properties":{"input":{"type":"string"}},"required":["input"],"additionalProperties":false}}));
    let mut series = list
        .iter()
        .find(|t| t["name"] == "framelet_start_recording")
        .unwrap()
        .clone();
    series["name"] = json!("framelet_start_series");
    series["description"] = json!(
        "Start a reliable rotating recording group; each GIF is at most 300 seconds. Returns immediately. Choose total duration OR until_stop:true. Parts share a session identity and numbered names. Stop saves, EOF saves, cancel preserves committed parts. Call context for paths."
    );
    let properties = series["inputSchema"]["properties"].as_object_mut().unwrap();
    properties.remove("output");
    properties.insert("output_dir".into(), json!({"type":"string"}));
    properties.insert(
        "name".into(),
        json!({"type":"string","default":"recording","maxLength":48}),
    );
    properties.insert(
        "duration".into(),
        json!({"type":"number","minimum":0.01,"maximum":86400}),
    );
    properties.insert(
        "until_stop".into(),
        json!({"type":"boolean","default":false}),
    );
    properties.insert(
        "segment_seconds".into(),
        json!({"type":"integer","minimum":1,"maximum":300,"default":300}),
    );
    properties.insert(
        "max_wall_seconds".into(),
        json!({"type":"integer","minimum":1,"maximum":86400,"default":28800}),
    );
    properties.insert(
        "total_mib".into(),
        json!({"type":"integer","minimum":1,"maximum":1048576,"default":2048}),
    );
    properties.insert(
        "max_mib".into(),
        json!({"type":"integer","minimum":1,"maximum":256,"default":256}),
    );
    properties.insert(
        "request_id".into(),
        json!({"type":"string","minLength":1,"maxLength":80}),
    );
    series["inputSchema"]["required"] = json!(["output_dir"]);
    list.push(series);
    list.push(json!({"name":"framelet_recording_list","description":"Discover recording groups in output_root, including interrupted groups. Bounded and paginated; no recursive scan.","inputSchema":{"type":"object","properties":{"offset":{"type":"integer","minimum":0,"default":0},"limit":{"type":"integer","minimum":1,"maximum":100,"default":16}},"additionalProperties":false}}));
    let inspect = list
        .iter_mut()
        .find(|t| t["name"] == "framelet_inspect")
        .unwrap();
    inspect["inputSchema"]["properties"]["offset"] =
        json!({"type":"integer","minimum":0,"default":0});
    inspect["inputSchema"]["properties"]["limit"] =
        json!({"type":"integer","minimum":1,"maximum":100,"default":16});
    list.iter_mut()
        .find(|t| t["name"] == "framelet_frame_at")
        .unwrap()["inputSchema"]["properties"]["at"]["maximum"] = json!(86400);
    json!({"schema_version":"1.8","name":"framelet","version":env!("CARGO_PKG_VERSION"),"tools":tools})
}
