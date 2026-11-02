use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
pub fn manifest(gif: &Path) -> PathBuf {
    gif.with_extension("capture.json")
}
pub fn pending(gif: &Path) -> PathBuf {
    gif.with_extension("pending.json")
}
pub struct Pending {
    gif: PathBuf,
    path: PathBuf,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if !self.gif.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
pub fn begin(gif: &Path) -> Result<Pending> {
    ensure!(!manifest(gif).exists(), "capture manifest already exists");
    let path = pending(gif);
    let mut stage = crate::storage::StagedFile::new(&path)?;
    serde_json::to_writer(
        &mut stage.file,
        &json!({"schema":1,"state":"pending","gif":gif.file_name().map(|n|n.to_string_lossy().into_owned()),"pid":std::process::id(),"owner_created":crate::identity::process_stamp(std::process::id())?,"transaction_id":crate::identity::new_id()?}),
    )?;
    stage.file.flush()?;
    stage.publish(&path)?;
    Ok(Pending {
        gif: gif.to_path_buf(),
        path,
    })
}
pub fn complete(gif: &Path) -> Result<PathBuf> {
    let data = crate::timeline::load(gif)?
        .ok_or_else(|| anyhow::anyhow!("timeline missing; capture remains incomplete"))?;
    let timeline = crate::timeline::sidecar_path(gif);
    let target = manifest(gif);
    let value = json!({"schema":1,"state":"complete","dropped_events":data.dropped_events,"recording_warnings":data.warnings,"session_id":data.session_id,"asset_id":data.asset_id,"gif_sha256":crate::identity::sha256(gif)?,"timeline_sha256":crate::identity::sha256(&timeline)?});
    let mut stage = crate::storage::StagedFile::new(&target)?;
    serde_json::to_writer(&mut stage.file, &value)?;
    stage.file.flush()?;
    stage.publish(&target)?;
    if let Err(e) = std::fs::remove_file(pending(gif))
        && e.kind() != std::io::ErrorKind::NotFound
    {
        crate::runtime::report("transaction.cleanup", &e.into());
    }
    Ok(target)
}
pub fn verify(gif: &Path) -> Result<()> {
    let p = manifest(gif);
    if !p.exists() {
        return Ok(());
    }
    let _lease = crate::file_guard::Lease::input(&p)?;
    ensure!(p.metadata()?.len() <= 8192, "manifest too large");
    let value: Value = serde_json::from_reader(std::fs::File::open(&p)?)?;
    ensure!(
        value["schema"] == 1
            && value["state"] == "complete"
            && value["gif_sha256"].as_str() == Some(&crate::identity::sha256(gif)?)
            && value["timeline_sha256"].as_str()
                == Some(&crate::identity::sha256(&crate::timeline::sidecar_path(
                    gif
                ))?),
        "capture manifest content mismatch"
    );
    Ok(())
}
pub fn recover(gif: &Path) -> Result<Value> {
    let _lease = crate::file_guard::Lease::output(gif)?;
    let p = pending(gif);
    if manifest(gif).exists() {
        verify(gif)?;
        return Ok(json!({"state":"complete","manifest":manifest(gif)}));
    }
    ensure!(p.exists(), "no pending transaction for this GIF");
    let _pending_lease = crate::file_guard::Lease::input(&p)?;
    ensure!(p.metadata()?.len() <= 8192, "pending marker too large");
    let marker: Value = serde_json::from_reader(std::fs::File::open(&p)?)?;
    ensure!(
        marker["schema"] == 1
            && marker["gif"] == json!(gif.file_name().map(|n| n.to_string_lossy().into_owned())),
        "pending marker mismatch"
    );
    let owner = marker["pid"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| anyhow::anyhow!("invalid transaction owner PID"))?;
    let created = marker["owner_created"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("transaction owner creation time missing"))?;
    ensure!(
        owner != 0 || created == 0,
        "invalid transaction owner identity"
    );
    ensure!(
        crate::identity::process_stamp(owner)? != Some(created),
        "transaction owner is still active"
    );
    drop(_pending_lease);
    if !gif.exists() || !crate::timeline::sidecar_path(gif).exists() {
        return Ok(
            json!({"state":"incomplete","gif_present":gif.exists(),"timeline_present":crate::timeline::sidecar_path(gif).exists(),"pending":p,"note":"preserved files; missing frames/events cannot be invented"}),
        );
    }
    let manifest = complete(gif)?;
    Ok(json!({"state":"recovered","manifest":manifest}))
}

pub fn discover(root: &Path) -> Value {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) => return json!({"error":e.to_string(),"pending":[]}),
    };
    let mut pending = vec![];
    let mut scanned = 0;
    let mut truncated = false;
    for entry in entries {
        scanned += 1;
        if scanned > 512 {
            truncated = true;
            break;
        }
        if let Ok(entry) = entry
            && entry.file_type().is_ok_and(|t| t.is_file())
        {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(stem) = name.strip_suffix(".pending.json") {
                pending
                    .push(json!({"pending":entry.path(),"gif":root.join(format!("{stem}.gif"))}));
                if pending.len() >= 16 {
                    truncated = true;
                    break;
                }
            }
        }
    }
    json!({"pending":pending,"scan_limit":512,"truncated":truncated})
}
