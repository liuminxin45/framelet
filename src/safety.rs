use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::{Component, Path, PathBuf};

// These are mistake guards for a trusted local MCP client, not an OS security sandbox.
pub struct Paths {
    root: PathBuf,
    reads: Vec<PathBuf>,
}
impl Paths {
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
        json!({"executable":std::env::current_exe().ok(),"working_directory":std::env::current_dir().ok(),"output_root":self.root,"read_roots":self.reads,"transport":"stdio","platform":"windows","formats":["gif","png"],"max_recording_seconds":300,"max_active_recordings":1,"network_required":false,"workflow":"windows/monitors -> start_recording -> operate UI -> recording_status -> stop_recording -> extract_frames -> read_image -> edit","editing_recipe":{"algorithm":"stable-region difference + bisection + frame-interval cache","steps":["inspect duration; identify a single action or extract 4-6 overview frames first","save opening and closing frame_at PNG anchors; select a stable image region that reflects page position","probe midpoint with reference, compare_region, include_image:false; omit output to avoid PNG writes","use comparison.changed_fraction for the boundary predicate; pixel threshold defaults to 24 but tune for content","reuse [timestamp_ms,timestamp_ms+duration_ms) and frame_index to skip repeated queries","stop near 200-500 ms uncertainty; visually confirm boundary images; trim with context and verify 3-4 final frames"],"assumption":"single monotonic action with stable endpoint regions; not semantic importance","decoder_note":"GIF prefix decoding remains sequential; this reduces model images, PNG writes and repeated probes"},"path_policy":"relative paths resolve under output_root; outputs and reads must stay inside configured roots"})
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
