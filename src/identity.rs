use anyhow::{Result, ensure};
use std::{io::Read, path::Path};
use windows_sys::Win32::Security::Cryptography::*;
pub fn new_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    ensure!(
        unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                16,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        } >= 0,
        "Windows random ID generation failed"
    );
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub struct Sha256 {
    algorithm: BCRYPT_ALG_HANDLE,
    hash: BCRYPT_HASH_HANDLE,
    _object: Vec<u8>,
}
impl Drop for Sha256 {
    fn drop(&mut self) {
        unsafe {
            if !self.hash.is_null() {
                BCryptDestroyHash(self.hash);
            }
            if !self.algorithm.is_null() {
                BCryptCloseAlgorithmProvider(self.algorithm, 0);
            }
        }
    }
}
pub fn sha256(path: &Path) -> Result<String> {
    let _lease = crate::file_guard::Lease::input(path)?;
    sha256_reader(std::fs::File::open(path)?)
}
// CNG hash state has no thread affinity. Ownership is exclusive; buffers stay
// heap-stable when moved, and no Sync/shared mutation is exposed.
unsafe impl Send for Sha256 {}
impl Sha256 {
    pub fn new() -> Result<Self> {
        let mut h = Self {
            algorithm: std::ptr::null_mut(),
            hash: std::ptr::null_mut(),
            _object: vec![],
        };
        ensure!(
            unsafe {
                BCryptOpenAlgorithmProvider(
                    &mut h.algorithm,
                    BCRYPT_SHA256_ALGORITHM,
                    std::ptr::null(),
                    0,
                )
            } >= 0,
            "SHA-256 provider unavailable"
        );
        let mut len = 0u32;
        let mut got = 0;
        ensure!(
            unsafe {
                BCryptGetProperty(
                    h.algorithm,
                    BCRYPT_OBJECT_LENGTH,
                    (&mut len as *mut u32).cast(),
                    4,
                    &mut got,
                    0,
                )
            } >= 0
                && len <= 1024 * 1024,
            "SHA-256 object size failed"
        );
        h._object = vec![0; len as usize];
        ensure!(
            unsafe {
                BCryptCreateHash(
                    h.algorithm,
                    &mut h.hash,
                    h._object.as_mut_ptr(),
                    len,
                    std::ptr::null(),
                    0,
                    0,
                )
            } >= 0,
            "SHA-256 initialization failed"
        );
        Ok(h)
    }
    pub fn update(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() <= u32::MAX as usize, "hash block too large");
        ensure!(
            unsafe { BCryptHashData(self.hash, bytes.as_ptr(), bytes.len() as u32, 0) } >= 0,
            "SHA-256 update failed"
        );
        Ok(())
    }
    pub fn snapshot(&self) -> Result<String> {
        struct CopyState {
            hash: BCRYPT_HASH_HANDLE,
            _object: Vec<u8>,
        }
        impl Drop for CopyState {
            fn drop(&mut self) {
                if !self.hash.is_null() {
                    unsafe {
                        BCryptDestroyHash(self.hash);
                    }
                }
            }
        }
        let mut copy = CopyState {
            hash: std::ptr::null_mut(),
            _object: vec![0; self._object.len()],
        };
        ensure!(
            unsafe {
                BCryptDuplicateHash(
                    self.hash,
                    &mut copy.hash,
                    copy._object.as_mut_ptr(),
                    copy._object.len() as u32,
                    0,
                )
            } >= 0,
            "SHA-256 snapshot failed"
        );
        let mut digest = [0u8; 32];
        ensure!(
            unsafe { BCryptFinishHash(copy.hash, digest.as_mut_ptr(), 32, 0) } >= 0,
            "SHA-256 finish failed"
        );
        Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
    }
}
pub fn sha256_reader(mut source: impl Read) -> Result<String> {
    let mut digest = Sha256::new()?;
    let mut buffer = [0u8; 65536];
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n])?;
    }
    digest.snapshot()
}
pub fn process_stamp(pid: u32) -> Result<Option<u64>> {
    use windows_sys::Win32::{Foundation::*, System::Threading::*};
    if pid == 0 {
        return Ok(None);
    }
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x00100000, 0, pid);
        if h.is_null() {
            ensure!(
                GetLastError() == ERROR_INVALID_PARAMETER,
                "cannot establish recording owner liveness"
            );
            return Ok(None);
        }
        let state = WaitForSingleObject(h, 0);
        if state == 0 {
            CloseHandle(h);
            return Ok(None);
        }
        let mut c = std::mem::zeroed::<FILETIME>();
        let mut e = c;
        let mut k = c;
        let mut u = c;
        let ok = GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u);
        CloseHandle(h);
        ensure!(
            state == 258 && ok != 0,
            "cannot read recording owner identity"
        );
        Ok(Some(
            (c.dwHighDateTime as u64) << 32 | c.dwLowDateTime as u64,
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_hash_and_ids() {
        let a = new_id().unwrap();
        let b = new_id().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        let path = std::env::temp_dir().join(format!("framelet-hash-{a}"));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_file(path).unwrap();
    }
}
