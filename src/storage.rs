//! No-overwrite publication and cleanup. A published file is never reported as lost.
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
pub struct StagedFile {
    pub path: PathBuf,
    pub file: File,
    _lease: crate::file_guard::Lease,
}
pub struct StagedDirectory {
    pub path: PathBuf,
    target: PathBuf,
    _parent: crate::file_guard::Lease,
    pinned: Option<crate::file_guard::Lease>,
    committed: bool,
}
impl StagedDirectory {
    pub fn new(target: &Path) -> Result<Self> {
        ensure!(!target.try_exists()?, "output directory already exists");
        let target = std::path::absolute(target)?;
        let parent = crate::file_guard::Lease::output(&target)?;
        let path = target
            .parent()
            .context("output parent missing")?
            .join(format!(
                ".framelet-review-{}.partial",
                crate::identity::new_id()?
            ));
        std::fs::create_dir(&path)?;
        let pinned = crate::file_guard::Lease::output(&path.join("manifest.json"))?;
        Ok(Self {
            path,
            target,
            _parent: parent,
            pinned: Some(pinned),
            committed: false,
        })
    }
    pub fn publish(mut self) -> Result<()> {
        self.pinned.take();
        use std::os::windows::fs::OpenOptionsExt;
        let directory = std::fs::OpenOptions::new()
            .access_mode(
                windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
                    | windows_sys::Win32::Storage::FileSystem::DELETE,
            )
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS
                    | windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            )
            .open(&self.path)?;
        rename_file(&directory, &self.target, false)
            .context("publish review directory without overwrite")?;
        self.committed = true;
        Ok(())
    }
}
impl Drop for StagedDirectory {
    fn drop(&mut self) {
        if !self.committed {
            if let Ok(entries) = std::fs::read_dir(&self.path) {
                for entry in entries.flatten() {
                    if entry.file_type().is_ok_and(|f| f.is_file()) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
            self.pinned.take();
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}
impl StagedFile {
    pub fn new(target: &Path) -> Result<Self> {
        let lease = crate::file_guard::Lease::output(target)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let name = target.file_name().context("output requires a filename")?;
        let path = target.with_file_name(format!(
            "{}.{}-{stamp}.partial",
            name.to_string_lossy(),
            std::process::id()
        ));
        use std::os::windows::fs::OpenOptionsExt;
        let file = OpenOptions::new()
            .write(true)
            .access_mode(
                windows_sys::Win32::Foundation::GENERIC_READ
                    | windows_sys::Win32::Foundation::GENERIC_WRITE
                    | windows_sys::Win32::Storage::FileSystem::DELETE,
            )
            .create_new(true)
            .open(&path)
            .context("create staged output")?;
        Ok(Self {
            path,
            file,
            _lease: lease,
        })
    }
    pub fn publish(mut self, target: &Path) -> Result<Vec<String>> {
        self.file.flush()?;
        self.file.sync_all()?;
        publish_file(&self.file, target)?;
        Ok(vec![])
    }
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
pub fn copy(source: &Path, target: &Path) -> Result<Vec<String>> {
    ensure!(!target.exists(), "output already exists");
    let _lease = crate::file_guard::Lease::input(source)?;
    let mut input = File::open(source)?;
    let mut stage = StagedFile::new(target)?;
    std::io::copy(&mut input, &mut stage.file)?;
    stage.publish(target)
}
pub fn publish_file(file: &File, target: &Path) -> Result<()> {
    rename_file(file, target, false)
}
pub fn replace_json(target: &Path, value: &impl serde::Serialize) -> Result<()> {
    replace_json_bounded(target, value, 64 * 1024 * 1024)
}
pub fn replace_bytes(target: &Path, bytes: &[u8]) -> Result<()> {
    let mut stage = StagedFile::new(target)?;
    stage.file.write_all(bytes)?;
    stage.file.flush()?;
    stage.file.sync_all()?;
    rename_file(&stage.file, target, true)
}
pub fn replace_json_bounded(
    target: &Path,
    value: &impl serde::Serialize,
    limit: u64,
) -> Result<()> {
    if target.try_exists()? {
        ensure!(
            target.symlink_metadata()?.file_type().is_file(),
            "index must be a regular file"
        );
    }
    let mut stage = StagedFile::new(target)?;
    crate::json_io::write(&mut stage.file, value)?;
    stage.file.flush()?;
    stage.file.sync_all()?;
    ensure!(
        stage.file.metadata()?.len() <= limit,
        "JSON publication exceeds byte budget"
    );
    rename_file(&stage.file, target, true)
}
fn rename_file(file: &File, target: &Path, replace: bool) -> Result<()> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::*;
    let _lease = crate::file_guard::Lease::output(target)?;
    let absolute = std::path::absolute(target)?;
    let text = absolute.to_string_lossy();
    let normalized = std::ffi::OsStr::new(text.strip_prefix(r"\\?\").unwrap_or(&text));
    let name: Vec<u16> = normalized.encode_wide().chain(Some(0)).collect();
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let bytes = offset + name.len() * 2;
    let mut buffer = vec![0u64; bytes.div_ceil(8)];
    unsafe {
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = ((name.len() - 1) * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
        ensure!(
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileRenameInfo,
                info.cast(),
                bytes as u32
            ) != 0,
            "publish rename without overwrite: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_publication_preserves_a_racing_empty_directory() {
        let root = std::env::temp_dir().join(format!(
            "framelet-dir-test-{}",
            crate::identity::new_id().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        let target = root.join("result");
        let stage = StagedDirectory::new(&target).unwrap();
        let partial = stage.path.clone();
        std::fs::write(stage.path.join("owned"), b"test").unwrap();
        std::fs::create_dir(&target).unwrap();
        assert!(stage.publish().is_err());
        assert!(target.is_dir());
        assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
        assert!(!partial.exists());
        std::fs::remove_dir(target).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn collision_and_unwind_preserve_existing_outputs() {
        let root = std::env::temp_dir().join(format!(
            "framelet-storage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let target = root.join("target");
        std::fs::write(&target, b"original").unwrap();
        let mut stage = StagedFile::new(&target).unwrap();
        stage.file.write_all(b"replacement").unwrap();
        let partial = stage.path.clone();
        assert!(stage.publish(&target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert!(!partial.exists());
        let fresh = root.join("fresh");
        let result: Result<()> = crate::runtime::guard("test.staged.unwind", || {
            let mut stage = StagedFile::new(&fresh)?;
            stage.file.write_all(b"unfinished")?;
            panic!("controlled write-stage fault");
        });
        assert!(result.is_err());
        assert!(!fresh.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
