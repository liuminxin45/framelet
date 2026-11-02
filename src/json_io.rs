//! Bounded callers own file leases/limits; buffered JSON never hides a flush failure.
use anyhow::Result;
use std::io::{BufReader, BufWriter, Read, Write};
pub fn read<T: serde::de::DeserializeOwned>(input: impl Read) -> Result<T> {
    Ok(serde_json::from_reader(BufReader::with_capacity(
        65536, input,
    ))?)
}
pub fn write(output: impl Write, value: &impl serde::Serialize) -> Result<()> {
    let mut buffered = BufWriter::with_capacity(65536, output);
    serde_json::to_writer(&mut buffered, value)?;
    buffered.flush()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Counted<T> {
        inner: T,
        calls: usize,
    }
    impl<T: Read> Read for Counted<T> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.inner.read(out)
        }
    }
    impl<T: Write> Write for Counted<T> {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.inner.write(data)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }
    #[test]
    fn large_json_uses_bounded_io_and_preserves_values() {
        let data: Vec<_> = (0..20000)
            .map(|id| serde_json::json!({"id":id,"point":[2,3],"kind":"mouse"}))
            .collect();
        let mut sink = Counted {
            inner: Vec::new(),
            calls: 0,
        };
        write(&mut sink, &data).unwrap();
        assert!(sink.calls < 32, "small writes regressed: {}", sink.calls);
        let mut source = Counted {
            inner: sink.inner.as_slice(),
            calls: 0,
        };
        let decoded: Vec<serde_json::Value> = read(&mut source).unwrap();
        assert_eq!(decoded, data);
        assert!(
            source.calls < 32,
            "bytewise reads regressed: {}",
            source.calls
        );
    }
    #[test]
    fn flush_failure_is_returned() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk disconnected"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(write(Broken, &serde_json::json!({"small":true})).is_err());
    }
}
