use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::{Component, Path, PathBuf};

// These are mistake guards for a trusted local MCP client, not an OS security sandbox.
pub struct Paths {
    root: PathBuf,
    reads: Vec<PathBuf>,
}
impl Paths {
    pub fn output_root(&self) -> &std::path::Path {
        &self.root
    }
    pub fn output_directory(&self, value: &str) -> Result<PathBuf> {
        let path = self.resolve(value, false)?;
        ensure!(
            path.is_dir() && path.starts_with(&self.root),
            "output_dir must be a directory inside output_root"
        );
        let _lease = crate::file_guard::Lease::output(&path.join(".framelet-directory"))?;
        Ok(path)
    }
    pub fn new() -> Result<Self> {
        let root = std::env::var_os("FRAMELET_MCP_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| ".".into()))
                    .join("Framelet")
                    .join("Captures")
            });
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let mut reads = vec![root.clone()];
        if let Ok(value) = std::env::var("FRAMELET_MCP_READ_ROOTS") {
            for p in serde_json::from_str::<Vec<PathBuf>>(&value)
                .context("FRAMELET_MCP_READ_ROOTS must be a JSON array of directories")?
            {
                let p = p.canonicalize().context("read root must exist")?;
                ensure!(p.is_dir(), "read root must be a directory");
                reads.push(p);
            }
        }
        Ok(Self { root, reads })
    }
    pub fn context(&self) -> Value {
        let mut value = json!({"executable":std::env::current_exe().ok(),"working_directory":std::env::current_dir().ok(),"output_root":self.root,"read_roots":self.reads,"transport":"stdio","platform":"windows","formats":["gif","png"],"max_recording_seconds":300,"max_active_recordings":1,"network_required":false,"recovery_discovery":crate::transaction::discover(&self.root),"diagnostics":{"directory":crate::runtime::log_directory(),"format":"bounded rotating JSONL","failure_fields":["code","stage","message","unix_ms","retryable"]},"documentation":{"agent_guide":"docs/agent-guide.md","timeline":"docs/timeline.md","development":"docs/development.md","architecture":"docs/architecture.md","audit":"docs/code-audit.md","mcp":"docs/mcp.md","base":"https://github.com/liuminxin45/framelet/blob/main/","local_base":std::env::current_exe().ok().and_then(|p|p.ancestors().skip(1).find(|p|p.join("docs/agent-guide.md").is_file()).map(|p|p.to_path_buf()))},"timeline":{"default_enabled":true,"keyboard":true,"schema":2,"identity":"random session/asset IDs plus one shared observation counter","isolation":"kill-on-close recording worker process","recovery_tool":"framelet_recover","sidecar":"<GIF stem>.timeline.json","query_tool":"framelet_timeline","clocks":"UTC microseconds plus monotonic capture intervals","coordinates":"source-region-relative physical pixels; masked coordinates omitted","event_limit":100000},"capture_guidance":{
            "fps_range":{"min":1,"max":50},
            "presets":[
                {"intent":"static_state_check","fps":1,"description_en":"Check stable UI states or long waits; use shot for one final state.","description_zh":"检查稳定界面或长时间等待；只验证最终状态时优先用 shot。"},
                {"intent":"development_workflow","fps":5,"description_en":"Trace forms, clicks and state changes; use 3-5 FPS when transient details do not matter.","description_zh":"跟踪表单、点击和状态切换；不关心瞬态细节时选 3-5 FPS。"},
                {"intent":"compact_documentation","fps":12,"description_en":"Compact README GIF or step-by-step tutorial; use 10-15 FPS if smooth motion is secondary.","description_zh":"体积优先的 README GIF 或分步教程；流畅动作非重点时选 10-15 FPS。"},
                {"intent":"presentation","fps":30,"description_en":"Default for polished software demos, scrolling, dragging and transitions.","description_zh":"正式软件演示、滚动、拖拽和转场默认选 30 FPS。"},
                {"intent":"fast_motion_check","fps":50,"description_en":"Use 40-50 FPS only for fast motion or brief visual defects after checking performance.","description_zh":"快速动作或短暂视觉缺陷可选 40-50 FPS，先验证性能。"}
            ],
            "selection_rule":"Choose by the evidence needed, not merely whether the task is development. At 1 FPS, sub-second states may be missed; even 50 FPS does not guarantee every transient is captured.",
            "performance_rule":"FPS is a sampling ceiling, not a measured guarantee. Start with a short trial at max_width 960 or 1280; inspect samples / recorded elapsed time from recording_status, not encoded GIF frame count (static frames may be merged). Reduce capture area or max_width before lowering presentation FPS; if lowering FPS is necessary, report the compromise. Encoded width * height * FPS must be <= 80000000.",
            "timing_note":"GIF delays use centisecond resolution; requested 30 FPS is not a guarantee of uniform 33.33 ms frame intervals.",
            "guide":"docs/agent-guide.md"
        },"workflow":"windows/monitors -> start_recording -> operate UI -> recording_status -> stop_recording -> extract_frames -> read_image -> edit","editing_recipe":{"algorithm":"stable-region difference + bisection + frame-interval cache","steps":["inspect duration; identify a single action or extract 4-6 overview frames first","save opening and closing frame_at PNG anchors; select a stable image region that reflects page position","probe midpoint with reference, compare_region, include_image:false; omit output to avoid PNG writes","use comparison.changed_fraction for the boundary predicate; pixel threshold defaults to 24 but tune for content","reuse [timestamp_ms,timestamp_ms+duration_ms) and frame_index to skip repeated queries","stop near 200-500 ms uncertainty; visually confirm boundary images; trim with context and verify 3-4 final frames"],"assumption":"single monotonic action with stable endpoint regions; not semantic importance","decoder_note":"GIF prefix decoding remains sequential; this reduces model images, PNG writes and repeated probes"},"path_policy":"relative paths resolve under output_root; outputs and reads must stay inside configured roots"});
        value["max_recording_seconds_scope"] =
            json!("single GIF MCP calls; series have a separate bounded task lifetime");
        value["long_recording"] = json!({"start_tool":"framelet_start_series","list_tool":"framelet_recording_list","per_part_max_seconds":300,"checkpoint_target_seconds":5,"max_wall_seconds":86400,"default_wall_seconds":28800,"directory_suffix":".framelet","index":"recording.json","guide":"docs/long-recording.md","disconnect":"normal EOF stops and saves; killed workers require recovery"});
        value["documentation"]["long_recording"] = json!("docs/long-recording.md");
        value
    }
    pub fn recovery_target(&self, value: &str) -> Result<PathBuf> {
        match self.resolve(value, false) {
            Ok(p) => {
                ensure!(
                    p.starts_with(&self.root),
                    "recovery writes must remain inside output_root"
                );
                Ok(p)
            }
            Err(_) => self.resolve(value, true),
        }
    }
    pub fn resolve(&self, value: &str, output: bool) -> Result<PathBuf> {
        // canonicalize() returns extended DOS paths. Accept our own returned file paths,
        // but never extended UNC/device namespaces such as GLOBALROOT.
        let value = value
            .strip_prefix(r"\\?\")
            .filter(|s| {
                let b = s.as_bytes();
                b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\'
            })
            .unwrap_or(value);
        ensure!(
            !value.is_empty() && value.len() <= 1024,
            "path must be 1..1024 characters"
        );
        let p = Path::new(value);
        // Reject network/device paths and alternate data streams, even before canonicalization.
        ensure!(
            !value.starts_with(['\\', '/']),
            "UNC, device and root-relative paths are not supported"
        );
        for c in p.components() {
            ensure!(
                !matches!(c, Component::ParentDir),
                "parent traversal is not allowed"
            );
            if let Component::Normal(s) = c {
                let s = s.to_string_lossy();
                ensure!(
                    !s.contains(':') && !s.ends_with([' ', '.']),
                    "invalid Windows path component"
                );
                let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
                ensure!(
                    ![
                        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                        "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
                        "LPT7", "LPT8", "LPT9"
                    ]
                    .contains(&stem.as_str()),
                    "reserved Windows device name"
                );
            }
        }
        let joined = if p.is_absolute() {
            p.to_owned()
        } else {
            self.root.join(p)
        };
        let resolved = if output {
            ensure!(
                !joined.exists(),
                "output already exists: {}",
                joined.display()
            );
            let parent = joined
                .parent()
                .context("missing parent")?
                .canonicalize()
                .context("output parent must exist")?;
            parent.join(joined.file_name().context("missing file name")?)
        } else {
            joined.canonicalize().context("input must exist")?
        };
        let allowed = if output {
            std::slice::from_ref(&self.root)
        } else {
            self.reads.as_slice()
        };
        ensure!(
            allowed.iter().any(|r| resolved.starts_with(r)),
            "path is outside configured MCP roots; call framelet_context"
        );
        Ok(resolved)
    }
}

pub fn extension(path: &Path, expected: &str) -> Result<()> {
    ensure!(
        path.extension()
            .is_some_and(|s| s.eq_ignore_ascii_case(expected)),
        "output must use .{expected} extension"
    );
    Ok(())
}
