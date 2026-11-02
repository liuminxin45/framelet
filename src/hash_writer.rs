//! Hashes only bytes successfully written; checkpoints clone state instead of rereading prefixes.
use std::{
    fs::File,
    io::{self, Write},
};
pub struct HashWriter {
    pub file: File,
    digest: crate::identity::Sha256,
}
impl HashWriter {
    pub fn new(file: File) -> anyhow::Result<Self> {
        Ok(Self {
            file,
            digest: crate::identity::Sha256::new()?,
        })
    }
    pub fn snapshot(&self) -> anyhow::Result<String> {
        self.digest.snapshot()
    }
}
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.file.write(bytes)?;
        self.digest.update(&bytes[..n]).map_err(io::Error::other)?;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_survive_more_writes_and_thread_transfer() {
        let p = std::env::temp_dir().join(format!(
            "framelet-stream-hash-{}",
            crate::identity::new_id().unwrap()
        ));
        let mut writer = HashWriter::new(File::create(&p).unwrap()).unwrap();
        writer.write_all(b"abc").unwrap();
        assert_eq!(
            writer.snapshot().unwrap(),
            crate::identity::sha256_reader(File::open(&p).unwrap()).unwrap()
        );
        let writer = std::thread::spawn(move || {
            let mut writer = writer;
            writer.write_all(b"more").unwrap();
            writer
        })
        .join()
        .unwrap();
        assert_eq!(
            writer.snapshot().unwrap(),
            crate::identity::sha256_reader(File::open(&p).unwrap()).unwrap()
        );
        drop(writer);
        std::fs::remove_file(p).unwrap();
    }
}
