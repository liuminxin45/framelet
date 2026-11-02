//! Durable intent precedes each edit. Retries accept only the exact before/after values.
use super::{Client, Layout, config::Config, hash, read_optional};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema: u32,
    pub client: Client,
    pub config: PathBuf,
    pub skill: Option<PathBuf>,
    pub entry: Option<Value>,
    pub original: Option<Value>,
    pub files: BTreeMap<String, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema: u32,
    before: Option<Receipt>,
    after: Option<Receipt>,
    entry_before: Option<Value>,
    entry_after: Option<Value>,
}
pub(super) struct Lock(windows_sys::Win32::Foundation::HANDLE);
impl Lock {
    pub fn acquire(path: &Path) -> Result<Self> {
        use windows_sys::Win32::{Foundation::*, System::Threading::*};
        let key = hash(path.to_string_lossy().to_lowercase().as_bytes())?;
        let name: Vec<_> = format!("Local\\Framelet-Integration-{key}\0")
            .encode_utf16()
            .collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            ensure!(!handle.is_null(), "integration lock creation failed");
            let state = WaitForSingleObject(handle, 5000);
            if state != WAIT_OBJECT_0 && state != WAIT_ABANDONED {
                CloseHandle(handle);
                anyhow::bail!("integration is busy; retry later")
            };
            Ok(Self(handle))
        }
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.0);
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
fn validate(layout: &Layout, r: &Receipt) -> Result<()> {
    ensure!(
        r.schema == 1
            && r.client == layout.client
            && r.config == layout.config
            && r.skill == layout.skill,
        "integration receipt identity/version mismatch"
    );
    ensure!(
        r.entry.as_ref().is_none_or(Value::is_object)
            && r.original.as_ref().is_none_or(Value::is_object),
        "invalid receipt server entry"
    );
    ensure!(
        r.files.len() <= 6 && (r.skill.is_some() || r.files.is_empty()),
        "invalid receipt files"
    );
    for (name, text) in &r.files {
        ensure!(
            [
                "SKILL.md",
                "references/testing.md",
                "references/demos.md",
                "references/editing.md",
                "references/installation.json",
                "agents/openai.yaml"
            ]
            .contains(&name.as_str())
                && text.len() <= 65536,
            "unexpected receipt file or size"
        );
    }
    Ok(())
}
pub(super) fn receipt(layout: &Layout) -> Result<Option<Receipt>> {
    read_optional(&layout.state, 2 * 1024 * 1024)?
        .map(|bytes| {
            let r = serde_json::from_slice(&bytes)?;
            validate(layout, &r)?;
            Ok(r)
        })
        .transpose()
}
fn names(before: Option<&Receipt>, after: Option<&Receipt>) -> BTreeSet<String> {
    before
        .into_iter()
        .chain(after)
        .flat_map(|r| r.files.keys().cloned())
        .collect()
}
pub(super) fn validate_files(
    layout: &Layout,
    before: Option<&Receipt>,
    after: Option<&Receipt>,
) -> Result<()> {
    if let Some(root) = &layout.skill {
        for name in names(before, after) {
            let actual = read_optional(&root.join(&name), 65536)?;
            let expected = before
                .and_then(|r| r.files.get(&name))
                .map(|s| s.as_bytes());
            ensure!(
                actual.as_deref() == expected,
                "skill file is unmanaged or modified: {}; preserve it before retrying",
                name
            );
        }
    }
    Ok(())
}
pub(super) fn begin(
    layout: &Layout,
    before: Option<Receipt>,
    after: Option<Receipt>,
    entry_before: Option<Value>,
) -> Result<()> {
    ensure!(
        !layout.journal.try_exists()?,
        "integration journal already exists"
    );
    let entry_after = after
        .as_ref()
        .map(|r| r.entry.clone())
        .unwrap_or_else(|| before.as_ref().and_then(|r| r.original.clone()));
    let plan = Plan {
        schema: 1,
        before,
        after,
        entry_before,
        entry_after,
    };
    let bytes = serde_json::to_vec(&plan)?;
    ensure!(
        bytes.len() <= 2 * 1024 * 1024,
        "integration plan exceeds byte budget"
    );
    // A foreign journal is never replaced during begin.
    let mut stage = crate::storage::StagedFile::new(&layout.journal)?;
    stage.file.write_all(&bytes)?;
    stage.publish(&layout.journal)?;
    Ok(())
}
pub(super) fn recover(layout: &Layout) -> Result<()> {
    let Some(bytes) = read_optional(&layout.journal, 2 * 1024 * 1024)? else {
        return Ok(());
    };
    let plan: Plan = serde_json::from_slice(&bytes)?;
    ensure!(
        plan.schema == 1 && (plan.before.is_some() || plan.after.is_some()),
        "invalid integration journal"
    );
    for r in plan.before.iter().chain(&plan.after) {
        validate(layout, r)?;
    }
    ensure!(
        plan.entry_after
            == plan
                .after
                .as_ref()
                .map(|r| r.entry.clone())
                .unwrap_or_else(|| plan.before.as_ref().and_then(|r| r.original.clone())),
        "journal target entry mismatch"
    );
    let config = Config::load(&layout.config, layout.client)?;
    let current = config.entry()?;
    ensure!(
        current == plan.entry_before || current == plan.entry_after,
        "server entry changed during interrupted integration; preserve edits and inspect the private journal"
    );
    if let Some(root) = &layout.skill {
        for name in names(plan.before.as_ref(), plan.after.as_ref()) {
            let target = root.join(&name);
            let before = plan
                .before
                .as_ref()
                .and_then(|r| r.files.get(&name))
                .map(|s| s.as_bytes());
            let after = plan
                .after
                .as_ref()
                .and_then(|r| r.files.get(&name))
                .map(|s| s.as_bytes());
            let actual = read_optional(&target, 65536)?;
            ensure!(
                actual.as_deref() == before || actual.as_deref() == after,
                "skill changed during interrupted integration: {name}"
            );
            if actual.as_deref() == after {
                continue;
            }
            if let Some(after) = after {
                std::fs::create_dir_all(target.parent().context("skill parent")?)?;
                ensure!(
                    read_optional(&target, 65536)? == actual,
                    "skill changed before publication"
                );
                replace(&target, after, 65536)?;
            } else {
                std::fs::remove_file(&target)?;
            }
        }
    }
    // Re-read the live document so unrelated fields added during an interruption survive.
    let config = Config::load(&layout.config, layout.client)?;
    let current = config.entry()?;
    ensure!(
        current == plan.entry_before || current == plan.entry_after,
        "server entry changed before config publication"
    );
    if current != plan.entry_after {
        let before = config.before.clone();
        let data = config.render(plan.entry_after.as_ref())?;
        if let Some(before) = &before {
            std::fs::create_dir_all(&layout.backup)?;
            ensure!(
                std::fs::read_dir(&layout.backup)?.take(129).count() < 128,
                "backup budget reached; archive private integration backups before retrying"
            );
            let file =
                layout
                    .backup
                    .join(format!("{}-{}.backup", layout.client.name(), hash(before)?));
            if !file.try_exists()? {
                let mut stage = crate::storage::StagedFile::new(&file)?;
                stage.file.write_all(before)?;
                stage.publish(&file)?;
            }
        }
        std::fs::create_dir_all(layout.config.parent().context("config parent")?)?;
        ensure!(
            read_optional(&layout.config, 8 * 1024 * 1024)? == before,
            "client config changed concurrently; retry to preserve those changes"
        );
        replace(&layout.config, &data, 8 * 1024 * 1024)?;
    }
    if let Some(after) = &plan.after {
        crate::storage::replace_json_bounded(&layout.state, after, 2 * 1024 * 1024)?;
    } else if layout.state.try_exists()? {
        std::fs::remove_file(&layout.state)?;
    }
    if plan.after.is_none()
        && let Some(root) = &layout.skill
    {
        for dir in [root.join("references"), root.join("agents"), root.clone()] {
            if let Err(e) = std::fs::remove_dir(dir)
                && !matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                )
            {
                return Err(e.into());
            }
        }
    }
    std::fs::remove_file(&layout.journal)?;
    Ok(())
}
fn replace(path: &Path, bytes: &[u8], limit: usize) -> Result<()> {
    ensure!(
        bytes.len() <= limit,
        "integration output exceeds byte budget"
    );
    if path.try_exists()? {
        ensure!(
            path.symlink_metadata()?.file_type().is_file(),
            "integration target must be a regular file"
        );
    }
    crate::storage::replace_bytes(path, bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn replay_each_install_and_uninstall_boundary_preserves_user_data() {
        for boundary in 0..4 {
            let root = std::env::temp_dir().join(format!(
                "framelet-integration-replay-{}",
                crate::identity::new_id().unwrap()
            ));
            std::fs::create_dir(&root).unwrap();
            let args = crate::integration::Args {
                action: crate::integration::Action::Install,
                client: Client::Cursor,
                config: Some(root.join("config.json")),
                skill_dir: Some(root.join("skill")),
                output_root: None,
                adopt_existing: false,
            };
            let layout = Layout::new(&args).unwrap();
            std::fs::create_dir_all(layout.state.parent().unwrap()).unwrap();
            std::fs::write(&layout.config, b"{\"other\":true}").unwrap();
            let target = Receipt {
                schema: 1,
                client: layout.client,
                config: layout.config.clone(),
                skill: layout.skill.clone(),
                entry: Some(json!({"command":"self","args":["mcp"]})),
                original: None,
                files: BTreeMap::from([("SKILL.md".into(), "owned skill".into())]),
            };
            begin(&layout, None, Some(target.clone()), None).unwrap();
            if boundary >= 1 {
                std::fs::create_dir_all(layout.skill.as_ref().unwrap()).unwrap();
                std::fs::write(
                    layout.skill.as_ref().unwrap().join("SKILL.md"),
                    "owned skill",
                )
                .unwrap();
            }
            if boundary >= 2 {
                let cfg = Config::load(&layout.config, layout.client).unwrap();
                replace(
                    &layout.config,
                    &cfg.render(target.entry.as_ref()).unwrap(),
                    65536,
                )
                .unwrap();
            }
            if boundary >= 3 {
                crate::storage::replace_json(&layout.state, &target).unwrap();
            }
            recover(&layout).unwrap();
            recover(&layout).unwrap();
            assert_eq!(
                Config::load(&layout.config, layout.client)
                    .unwrap()
                    .entry()
                    .unwrap(),
                target.entry
            );
            let note = layout.skill.as_ref().unwrap().join("user-note.txt");
            std::fs::write(&note, "keep me").unwrap();
            begin(&layout, Some(target.clone()), None, target.entry.clone()).unwrap();
            if boundary >= 1 {
                std::fs::remove_file(layout.skill.as_ref().unwrap().join("SKILL.md")).unwrap();
            }
            if boundary >= 2 {
                let cfg = Config::load(&layout.config, layout.client).unwrap();
                replace(&layout.config, &cfg.render(None).unwrap(), 65536).unwrap();
            }
            if boundary >= 3 {
                std::fs::remove_file(&layout.state).unwrap();
            }
            recover(&layout).unwrap();
            recover(&layout).unwrap();
            assert_eq!(std::fs::read_to_string(note).unwrap(), "keep me");
            assert!(
                Config::load(&layout.config, layout.client)
                    .unwrap()
                    .entry()
                    .unwrap()
                    .is_none()
            );
            let value: Value =
                serde_json::from_slice(&std::fs::read(&layout.config).unwrap()).unwrap();
            assert_eq!(value["other"], true);
            assert!(root.starts_with(std::env::temp_dir()));
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
