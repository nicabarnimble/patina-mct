use anyhow::{Context, Result};
use mct_kernel::MctCallPayloadHandle;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

/// Maximum bytes accepted by the local content-addressed blob store.
pub const MCT_BLOB_MAX_BYTES: usize = 8 * 1024 * 1024;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct MctLocalBlobStore {
    root: PathBuf,
}

#[derive(Debug, Error)]
pub enum MctLocalBlobStoreError {
    #[error("invalid blob digest")]
    InvalidDigest,
    #[error("blob too large")]
    BlobTooLarge,
    #[error("blob size mismatch")]
    BlobSizeMismatch,
    #[error("blob digest mismatch")]
    BlobDigestMismatch,
    #[error("payload blob unavailable")]
    PayloadBlobUnavailable,
    #[error("blob store I/O failed")]
    Io {
        #[source]
        source: io::Error,
    },
}

impl MctLocalBlobStoreError {
    pub fn safe_message(&self) -> &'static str {
        match self {
            Self::InvalidDigest => "invalid blob digest",
            Self::BlobTooLarge => "blob too large",
            Self::BlobSizeMismatch => "blob size mismatch",
            Self::BlobDigestMismatch => "blob digest mismatch",
            Self::PayloadBlobUnavailable => "payload blob unavailable",
            Self::Io { .. } => "blob store unavailable",
        }
    }
}

impl MctLocalBlobStore {
    pub fn for_state_path(state_path: impl AsRef<Path>) -> Self {
        let state_path = state_path.as_ref();
        let state_dir = state_path.parent().unwrap_or_else(|| Path::new("."));
        Self {
            root: state_dir.join("blobs"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn visible_path(&self, digest: &str) -> Result<PathBuf, MctLocalBlobStoreError> {
        ensure_blake3_digest_hex(digest)?;
        Ok(self.blob_path_unchecked(digest))
    }

    pub fn ingest_reader<R: Read>(
        &self,
        digest: &str,
        size_bytes: u64,
        content_type: &str,
        mut reader: R,
    ) -> Result<MctCallPayloadHandle, MctLocalBlobStoreError> {
        ensure_blake3_digest_hex(digest)?;
        if size_bytes > MCT_BLOB_MAX_BYTES as u64 {
            return Err(MctLocalBlobStoreError::BlobTooLarge);
        }
        if content_type.trim().is_empty() {
            return Err(MctLocalBlobStoreError::InvalidDigest);
        }

        fs::create_dir_all(self.root.join("tmp")).map_err(io_error)?;
        let tmp_path = self.temp_path();
        let ingest_result =
            self.write_verify_and_publish(&tmp_path, digest, size_bytes, content_type, &mut reader);
        if ingest_result.is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
        ingest_result
    }

    pub fn fetch(&self, handle: &MctCallPayloadHandle) -> Result<Vec<u8>, MctLocalBlobStoreError> {
        let MctCallPayloadHandle::ContentAddressedBlob {
            digest, size_bytes, ..
        } = handle
        else {
            return Ok(Vec::new());
        };
        ensure_blake3_digest_hex(digest)?;
        if *size_bytes > MCT_BLOB_MAX_BYTES as u64 {
            return Err(MctLocalBlobStoreError::BlobTooLarge);
        }
        let path = self.blob_path_unchecked(digest);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(MctLocalBlobStoreError::PayloadBlobUnavailable);
            }
            Err(source) => return Err(MctLocalBlobStoreError::Io { source }),
        };
        let mut bytes = Vec::new();
        let read = Read::by_ref(&mut file)
            .take(MCT_BLOB_MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if read > MCT_BLOB_MAX_BYTES {
            return Err(MctLocalBlobStoreError::BlobTooLarge);
        }
        Ok(bytes)
    }

    fn write_verify_and_publish<R: Read>(
        &self,
        tmp_path: &Path,
        digest: &str,
        size_bytes: u64,
        content_type: &str,
        reader: &mut R,
    ) -> Result<MctCallPayloadHandle, MctLocalBlobStoreError> {
        let mut tmp = File::create(tmp_path).map_err(io_error)?;
        let mut hasher = blake3::Hasher::new();
        let mut observed_size = 0_u64;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = reader.read(&mut buffer).map_err(io_error)?;
            if read == 0 {
                break;
            }
            observed_size = observed_size
                .checked_add(read as u64)
                .ok_or(MctLocalBlobStoreError::BlobTooLarge)?;
            if observed_size > MCT_BLOB_MAX_BYTES as u64 {
                return Err(MctLocalBlobStoreError::BlobTooLarge);
            }
            hasher.update(&buffer[..read]);
            tmp.write_all(&buffer[..read]).map_err(io_error)?;
        }
        tmp.flush().map_err(io_error)?;
        drop(tmp);

        if observed_size != size_bytes {
            return Err(MctLocalBlobStoreError::BlobSizeMismatch);
        }
        let observed_digest = hasher.finalize().to_hex().to_string();
        if observed_digest != digest {
            return Err(MctLocalBlobStoreError::BlobDigestMismatch);
        }

        let final_path = self.blob_path_unchecked(digest);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::rename(tmp_path, &final_path).map_err(io_error)?;
        Ok(MctCallPayloadHandle::ContentAddressedBlob {
            digest: digest.to_owned(),
            blob_ref: format!("blake3:{digest}"),
            content_type: content_type.to_owned(),
            size_bytes,
        })
    }

    fn temp_path(&self) -> PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::SeqCst);
        self.root
            .join("tmp")
            .join(format!("ingest-{}-{id}.tmp", std::process::id()))
    }

    fn blob_path_unchecked(&self, digest: &str) -> PathBuf {
        self.root
            .join("blake3")
            .join(&digest[..2])
            .join(format!("{digest}.blob"))
    }
}

fn ensure_blake3_digest_hex(value: &str) -> Result<(), MctLocalBlobStoreError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(MctLocalBlobStoreError::InvalidDigest)
    }
}

fn io_error(source: io::Error) -> MctLocalBlobStoreError {
    MctLocalBlobStoreError::Io { source }
}

pub fn local_blob_store_for_state_path(state_path: impl AsRef<Path>) -> MctLocalBlobStore {
    MctLocalBlobStore::for_state_path(state_path)
}

pub fn content_addressed_blob_handle(
    digest: impl Into<String>,
    content_type: impl Into<String>,
    size_bytes: u64,
) -> MctCallPayloadHandle {
    let digest = digest.into();
    MctCallPayloadHandle::ContentAddressedBlob {
        blob_ref: format!("blake3:{digest}"),
        digest,
        content_type: content_type.into(),
        size_bytes,
    }
}

pub fn ingest_blob_from_path(
    store: &MctLocalBlobStore,
    digest: &str,
    size_bytes: u64,
    content_type: &str,
    path: &Path,
) -> Result<MctCallPayloadHandle> {
    let file = File::open(path).with_context(|| format!("open blob input {}", path.display()))?;
    store
        .ingest_reader(digest, size_bytes, content_type, file)
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn digest(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    #[test]
    fn ingest_rejects_digest_mismatch_without_visible_blob() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctLocalBlobStore::for_state_path(dir.path().join("state.sqlite"));
        let bytes = b"blob bytes";
        let declared = digest(b"different");
        let result = store.ingest_reader(
            &declared,
            bytes.len() as u64,
            "application/octet-stream",
            Cursor::new(bytes),
        );
        assert!(matches!(
            result,
            Err(MctLocalBlobStoreError::BlobDigestMismatch)
        ));
        assert!(!store.visible_path(&declared).unwrap().exists());
    }

    #[test]
    fn ingest_rejects_oversized_input_before_visibility() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctLocalBlobStore::for_state_path(dir.path().join("state.sqlite"));
        let oversized = vec![b'x'; MCT_BLOB_MAX_BYTES + 1];
        let declared = digest(&oversized);
        let result = store.ingest_reader(
            &declared,
            oversized.len() as u64,
            "application/octet-stream",
            Cursor::new(&oversized),
        );
        assert!(matches!(result, Err(MctLocalBlobStoreError::BlobTooLarge)));
        assert!(!store.visible_path(&declared).unwrap().exists());
    }

    #[test]
    fn fetch_absent_digest_is_typed_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctLocalBlobStore::for_state_path(dir.path().join("state.sqlite"));
        let bytes = b"missing";
        let handle = content_addressed_blob_handle(
            digest(bytes),
            "application/octet-stream",
            bytes.len() as u64,
        );
        let result = store.fetch(&handle);
        assert!(matches!(
            result,
            Err(MctLocalBlobStoreError::PayloadBlobUnavailable)
        ));
    }

    #[test]
    fn fetch_returns_tampered_bytes_for_kernel_integrity_decision() {
        let dir = tempfile::tempdir().unwrap();
        let store = MctLocalBlobStore::for_state_path(dir.path().join("state.sqlite"));
        let bytes = b"trusted blob";
        let declared = digest(bytes);
        let handle = store
            .ingest_reader(
                &declared,
                bytes.len() as u64,
                "application/octet-stream",
                Cursor::new(bytes),
            )
            .unwrap();
        fs::write(store.visible_path(&declared).unwrap(), b"tampered!!!!").unwrap();
        let fetched = store.fetch(&handle).unwrap();
        assert_eq!(fetched, b"tampered!!!!");
    }
}
