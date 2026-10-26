//! Bounded immutable acquisition shared by inventory and execution.
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

pub(crate) const MAX_COMPONENT_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_SIDECAR_BYTES: usize = 256;

pub(crate) fn read_bounded_file(path: &Path, limit: usize) -> io::Result<Box<[u8]>> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "child file is not a bounded regular file",
        ));
    }
    read_bounded(&mut file, limit)
}

fn read_bounded(reader: &mut impl Read, limit: usize) -> io::Result<Box<[u8]>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 16 * 1024];
    loop {
        // Probe one byte past the cap, never reserve based on mutable metadata.
        let capacity = chunk.len().min(limit - bytes.len() + 1);
        let count = reader.read(&mut chunk[..capacity])?;
        if count == 0 {
            return Ok(bytes.into_boxed_slice());
        }
        if count > limit - bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "child file byte limit exceeded",
            ));
        }
        bytes.try_reserve_exact(count).map_err(io::Error::other)?;
        bytes.extend_from_slice(&chunk[..count]);
    }
}

/// Private bytes, no backing pathname/handle and no mutable accessor. This is
/// not publisher authentication: the expected digest comes from admitted policy.
pub(crate) struct ComponentBytes(Box<[u8]>);

impl ComponentBytes {
    pub(crate) fn read(path: &Path) -> io::Result<Self> {
        read_bounded_file(path, MAX_COMPONENT_BYTES).map(Self)
    }

    pub(crate) fn verify(path: &Path, expected_artifact: &str) -> io::Result<Self> {
        let expected = expected_artifact
            .strip_prefix("sha256:")
            .filter(|digest| {
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "expected content-addressed WASM artifact",
                )
            })?;
        let snapshot = Self::read(path)?;
        if format!("{:x}", Sha256::digest(snapshot.as_bytes())) != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "WASM artifact digest mismatch",
            ));
        }
        Ok(snapshot)
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_reader_accepts_exact_cap_and_rejects_growth_without_stat_trust() {
        assert_eq!(&*read_bounded(&mut &b"abcd"[..], 4).unwrap(), b"abcd");
        assert!(read_bounded(&mut &b"abcde"[..], 4).is_err());
        assert!(read_bounded(&mut &b"x"[..], 0).is_err());
        assert!(read_bounded(&mut &b""[..], 0).unwrap().is_empty());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sparse.wasm");
        File::create(&path)
            .unwrap()
            .set_len(MAX_COMPONENT_BYTES as u64 + 1)
            .unwrap();
        assert!(ComponentBytes::read(&path).is_err());
        assert!(read_bounded_file(dir.path(), 100).is_err());
    }
}
