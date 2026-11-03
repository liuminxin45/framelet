//! Short human/agent guidance without duplicating the full structured payload.
use serde_json::Value;

pub fn text(name: &str, value: &Value) -> String {
    if value["terminal"] == true && value["result"]["state"] != "completed" {
        return format!(
            "Recording ended: {}. Inspect structuredContent.result for failure, saved paths and recovery.",
            value["result"]["state"].as_str().unwrap_or("unknown")
        );
    }
    let recording = if value["result"]["recording"].is_object() {
        &value["result"]["recording"]
    } else if value["recording"].is_object() {
        &value["recording"]
    } else {
        value
    };
    let quality = &value["quality"];
    let saved = recording.get("output").is_some()
        && matches!(
            name,
            "framelet_edit"
                | "framelet_shot"
                | "framelet_prepare_demo"
                | "framelet_recording_status"
                | "framelet_wait_recording"
                | "framelet_stop_recording"
                | "framelet_cancel_recording"
        );
    let mut text = if saved {
        "File saved; content has not been visually approved. ".to_owned()
    } else if quality.is_object() {
        "Analysis complete; visual review required. ".to_owned()
    } else if value["terminal"] == true {
        "Recording group saved; content has not been visually approved. ".to_owned()
    } else {
        format!("{name} completed. ")
    };
    if let Some(samples) = recording["capture_metrics"]["source_samples"].as_u64() {
        text.push_str(&format!("Capture samples: {samples}. "));
    }
    if let Some(frames) = recording["frames"].as_u64() {
        text.push_str(&format!("Encoded GIF images: {frames}; this is not capture FPS. Identical images may be merged without shortening playback. "));
    }
    let sampling = &quality["sampling"];
    if let Some(target) = sampling["requested_fps"].as_u64() {
        text.push_str(&format!("Requested: {target} FPS. "));
    }
    if let Some(rate) = sampling["measured_action_fps"].as_f64() {
        text.push_str(&format!("Measured in action windows: {rate:.1} FPS. "));
    }
    if let Some(gap) = sampling["longest_action_gap_us"].as_u64() {
        text.push_str(&format!(
            "Longest action sampling gap: {:.1} ms. ",
            gap as f64 / 1000.0
        ));
    }
    let warnings = recording["warnings"].as_array().map_or(0, Vec::len);
    let findings = quality["findings"].as_array().map_or(0, Vec::len);
    if warnings > 0
        || findings > 0
        || value.get("preview_error").is_some()
        || value.get("review_error").is_some()
    {
        text.push_str(&format!("Attention: {warnings} recording warnings, {findings} quality findings; inspect warnings/findings/preview_error/review_error. "));
    }
    text.push_str("Read structuredContent.result.");
    text
}
