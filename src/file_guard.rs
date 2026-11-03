//! Pin ancestor directories against rename/delete for the operation's lifetime.
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    os::windows::{
        ffi::OsStringExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::*;

/// Resolve where this process actually writes, including packaged-app redirection.
/// The empty probe is owned, create-new and delete-on-close, even on process death.
pub fn writable_directory(directory: &Path) -> Result<PathBuf> {
    use std::os::windows::io::AsRawHandle;
    let probe = directory.join(format!(".framelet-root-{}.tmp", crate::identity::new_id()?));
    let _lease = Lease::output(&probe)?;
    let file = OpenOptions::new()
        .write(true)
        .access_mode(
            windows_sys::Win32::Foundation::GENERIC_READ
                | windows_sys::Win32::Foundation::GENERIC_WRITE
                | DELETE,
        )
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_DELETE_ON_CLOSE | FILE_FLAG_OPEN_REPARSE_POINT)
        .create_new(true)
        .open(&probe)
        .context("verify output directory is writable")?;
    let mut name = vec![0u16; 32768];
    let n = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            name.as_mut_ptr(),
            name.len() as u32,
            0,
        )
    };
    ensure!(
        n > 0 && (n as usize) < name.len(),
        "cannot resolve actual output directory: {}",
        std::io::Error::last_os_error()
    );
    let path = PathBuf::from(std::ffi::OsString::from_wide(&name[..n as usize]));
    Ok(path
        .parent()
        .context("output probe has no parent")?
        .to_owned())
}
pub struct Lease {
    _handles: Vec<File>,
    input: bool,
}
impl Lease {
    pub fn reader(&self) -> Result<File> {
        ensure!(self.input, "output lease has no input reader");
        Ok(self
            ._handles
            .last()
            .context("input handle missing")?
            .try_clone()?)
    }
    pub fn output(path: &Path) -> Result<Self> {
        Self::open(path, false)
    }
    pub fn input(path: &Path) -> Result<Self> {
        Self::open(path, true)
    }
    fn open(path: &Path, input: bool) -> Result<Self> {
        let absolute = std::path::absolute(path)?;
        let parent = absolute.parent().context("path requires a parent")?;
        let mut directories: Vec<_> = parent.ancestors().collect();
        directories.reverse();
        let mut handles = vec![];
        for directory in directories {
            let file = OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(directory)
                .with_context(|| format!("pin directory {}", directory.display()))?;
            ensure!(
                file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
                "reparse directories are not allowed for recording I/O"
            );
            handles.push(file);
        }
        if input {
            let file = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&absolute)?;
            ensure!(
                file.metadata()?.is_file()
                    && file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
                "input must be a regular, non-reparse file"
            );
            handles.push(file);
        }
        Ok(Self {
            _handles: handles,
            input,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writable_root_round_trip_leaves_no_probe() {
        let root = std::env::temp_dir().join(format!(
            "framelet-root-test-{}",
            crate::identity::new_id().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        let actual = writable_directory(&root).unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::write(actual.join("roundtrip"), b"test").unwrap();
        assert_eq!(
            root.join("roundtrip").canonicalize().unwrap(),
            actual.join("roundtrip")
        );
        std::fs::remove_file(actual.join("roundtrip")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn pins_reject_directory_swap_and_input_mutation() {
        let root = std::env::temp_dir().join(format!(
            "framelet-lease-{}",
            crate::identity::new_id().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        let directory = root.join("parent");
        std::fs::create_dir(&directory).unwrap();
        let input = directory.join("input");
        std::fs::write(&input, b"abc").unwrap();
        let lease = Lease::input(&input).unwrap();
        assert!(std::fs::rename(&directory, root.join("moved")).is_err());
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(&input)
                .is_err()
        );
        drop(lease);
        std::fs::rename(&directory, root.join("moved")).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
