//! Count serialized bytes without allocating a second copy of untrusted data.
use std::io::{self, Write};

pub(crate) fn serialized_size(value: &impl serde::Serialize, limit: usize) -> io::Result<usize> {
    struct Counter {
        used: usize,
        limit: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit - self.used {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "serialized byte budget exceeded",
                ));
            }
            self.used += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { used: 0, limit };
    serde_json::to_writer(&mut counter, value).map_err(io::Error::other)?;
    Ok(counter.used)
}
