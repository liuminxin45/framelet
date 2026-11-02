//! Embedded, version-matched workflow references; never read arbitrary paths.
use serde_json::{Value, json};
const GUIDES: [(&str, &str, &str); 5] = [
    (
        "workflow",
        "Efficient recording/review / 高效录制剪辑",
        include_str!("../../docs/performance.md"),
    ),
    (
        "installation",
        "Installation and lifecycle / 安装与生命周期",
        include_str!("../../docs/installation.md"),
    ),
    (
        "testing",
        "Software test evidence / 软件验收",
        include_str!("../../skills/framelet/references/testing.md"),
    ),
    (
        "demos",
        "Product demonstration / 产品演示",
        include_str!("../../skills/framelet/references/demos.md"),
    ),
    (
        "editing",
        "Efficient GIF editing / 高效剪辑",
        include_str!("../../skills/framelet/references/editing.md"),
    ),
];
pub fn list() -> Value {
    json!({"resources":GUIDES.iter().map(|(key,title,_)|json!({"uri":format!("framelet://guides/{key}"),"name":key,"title":title,"mimeType":"text/markdown","description":"Version-matched local Framelet instructions; no capture or network request."})).collect::<Vec<_>>()})
}
pub fn read(uri: &str) -> Option<Value> {
    GUIDES
        .iter()
        .find(|(key, _, _)| uri == format!("framelet://guides/{key}"))
        .map(
            |(_, _, body)| json!({"contents":[{"uri":uri,"mimeType":"text/markdown","text":body}]}),
        )
}
