//! Shared Unix storage boundary for the ledger and Mother host adapters.
//! No process-wide umask changes, implicit chmod repairs, or symlink resolution.
//! Root-owned sticky ancestors may contain private storage; executable ancestry
//! is stricter and never permits a group/world-writable directory.
use rustix::fs::{self as rfs, Mode, OFlags};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

fn refused(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
pub fn current_uid() -> u32 {
    rustix::process::geteuid().as_raw()
}

fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(refused("parent traversal is not a trusted storage path"));
    }
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn directory(path: &Path, create: bool, private: bool, executable: bool) -> io::Result<File> {
    let path = absolute(path)?;
    let mut fd: File = rfs::open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?
    .into();
    let root = fd.metadata()?;
    if root.uid() != 0 || root.mode() & 0o022 != 0 {
        return Err(refused("untrusted filesystem root"));
    }
    let parts = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p),
            _ => None,
        })
        .collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let next = match rfs::openat(&fd, *part, flags, Mode::empty()) {
            Ok(next) => next,
            Err(rustix::io::Errno::NOENT) if create => {
                match rfs::mkdirat(&fd, *part, Mode::from_raw_mode(0o700)) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                    Err(e) => return Err(e.into()),
                }
                // Persist each new directory entry, not just the eventual file's
                // immediate parent, before acknowledging durable storage below it.
                fd.sync_all()?;
                rfs::openat(&fd, *part, flags, Mode::empty())?
            }
            Err(e) => return Err(e.into()),
        };
        fd = next.into();
        let meta = fd.metadata()?;
        let mode = meta.mode();
        let final_private = private && index + 1 == parts.len();
        if final_private {
            if meta.uid() != current_uid() || mode & 0o7777 != 0o700 {
                return Err(refused(
                    "Mother storage directory must be current-UID owned and mode 0700",
                ));
            }
        } else if (meta.uid() != current_uid() && meta.uid() != 0)
            || (mode & 0o022 != 0 && (executable || meta.uid() != 0 || mode & 0o1000 == 0))
        {
            return Err(refused("untrusted owner or writable ancestor directory"));
        }
    }
    if parts.is_empty() && private {
        return Err(refused("filesystem root is not a private Mother directory"));
    }
    Ok(fd)
}

pub fn ensure_publication_directory(path: &Path) -> io::Result<()> {
    directory(path, true, false, true)?.sync_all()
}

pub fn ensure_directory(path: &Path) -> io::Result<()> {
    directory(path, true, true, false)?.sync_all()
}

fn parent(
    path: &Path,
    create: bool,
    private: bool,
    executable: bool,
) -> io::Result<(File, OsString)> {
    let path = absolute(path)?;
    let name = path
        .file_name()
        .ok_or_else(|| refused("file path has no name"))?
        .to_owned();
    Ok((
        directory(
            path.parent()
                .ok_or_else(|| refused("file path has no parent"))?,
            create,
            private,
            executable,
        )?,
        name,
    ))
}

fn private_file_facts(owner: u32, mode: u32, expected_uid: u32) -> bool {
    owner == expected_uid && mode & 0o7777 == 0o600
}

fn trusted_file_facts(owner: u32, mode: u32, expected_uid: u32) -> bool {
    (owner == expected_uid || owner == 0) && mode & 0o022 == 0
}

fn verify_file(file: File, private: bool, executable: bool) -> io::Result<File> {
    let meta = file.metadata()?;
    if !meta.is_file() || meta.nlink() != 1 {
        return Err(refused("expected a regular file with one hard link"));
    }
    if private {
        if !private_file_facts(meta.uid(), meta.mode(), current_uid()) {
            return Err(refused(
                "Mother storage file must be current-UID owned and mode 0600",
            ));
        }
    } else if !trusted_file_facts(meta.uid(), meta.mode(), current_uid()) {
        return Err(refused("untrusted file owner or writable host file"));
    }
    if executable && (meta.mode() & 0o111 == 0 || meta.mode() & 0o6000 != 0) {
        return Err(refused("host executable must be executable and not set-ID"));
    }
    Ok(file)
}
fn at(
    dir: &File,
    name: &OsStr,
    flags: OFlags,
    private: bool,
    executable: bool,
) -> io::Result<File> {
    let fd = rfs::openat(
        dir,
        name,
        flags | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::from_raw_mode(0o600),
    )?;
    verify_file(fd.into(), private, executable)
}

/// Read authority storage without creating or repairing anything.
pub fn read_file(path: &Path) -> io::Result<File> {
    let (dir, name) = parent(path, false, true, false)?;
    at(&dir, &name, OFlags::RDONLY, true, false)
}
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    read_file(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}
pub fn open_existing(path: &Path, append: bool) -> io::Result<File> {
    let (dir, name) = parent(path, false, true, false)?;
    at(
        &dir,
        &name,
        OFlags::RDWR
            | if append {
                OFlags::APPEND
            } else {
                OFlags::empty()
            },
        true,
        false,
    )
}
pub fn create_new(path: &Path) -> io::Result<File> {
    let (dir, name) = parent(path, true, true, false)?;
    let file = at(
        &dir,
        &name,
        OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::APPEND,
        true,
        false,
    )?;
    dir.sync_all()?;
    Ok(file)
}
pub fn open_or_create(path: &Path) -> io::Result<(File, bool)> {
    match create_new(path) {
        Ok(file) => Ok((file, true)),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            Ok((open_existing(path, true)?, false))
        }
        Err(e) => Err(e),
    }
}
pub fn verify_optional(path: &Path) -> io::Result<()> {
    match read_file(path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

// SQLite uses process-associated POSIX locks. Opening/closing an extra descriptor
// to a live DB or SHM inode can release those locks. Its preflight MUST use statat,
// never read_file/verify_optional, even if the descriptor would be read-only.
#[allow(
    clippy::unnecessary_cast,
    reason = "Unix stat field widths differ across targets"
)]
fn stat_private_at(dir: &File, name: &OsStr) -> io::Result<(u64, u64)> {
    let stat = rfs::statat(dir, name, rfs::AtFlags::SYMLINK_NOFOLLOW)?;
    if rfs::FileType::from_raw_mode(stat.st_mode) != rfs::FileType::RegularFile
        || stat.st_nlink != 1
        || !private_file_facts(stat.st_uid, stat.st_mode as u32, current_uid())
    {
        return Err(refused(
            "SQLite storage must be a single-link current-UID regular file, mode 0600",
        ));
    }
    Ok((stat.st_dev as u64, stat.st_ino as u64))
}
pub fn private_file_identity(path: &Path) -> io::Result<(u64, u64)> {
    let (dir, name) = parent(path, false, true, false)?;
    stat_private_at(&dir, &name)
}
pub fn verify_optional_metadata(path: &Path) -> io::Result<()> {
    match private_file_identity(path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Publish a closed, private empty inode without replacing any existing file.
/// Never close an auxiliary descriptor to an inode already visible to SQLite.
pub fn ensure_private_file(path: &Path) -> io::Result<(u64, u64)> {
    let (dir, name) = parent(path, true, true, false)?;
    match stat_private_at(&dir, &name) {
        Ok(identity) => return Ok(identity),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let temp = nonce_name("mother-create")?;
    let file = at(
        &dir,
        OsStr::new(&temp),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
        true,
        false,
    )?;
    let result = file.sync_all();
    drop(file);
    let result = result.and_then(|_| {
        match rfs::renameat_with(&dir, &temp, &dir, &name, rfs::RenameFlags::NOREPLACE) {
            Ok(()) => dir.sync_all(),
            Err(rustix::io::Errno::EXIST) => Ok(()),
            Err(e) => Err(e.into()),
        }
    });
    let _ = rfs::unlinkat(&dir, &temp, rfs::AtFlags::empty());
    result?;
    stat_private_at(&dir, &name)
}

pub fn nonce_name(prefix: &str) -> io::Result<String> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(format!(
        ".{prefix}-{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

/// Same-directory exclusive temporary creation and durable publication. A sync
/// failure after rename is an uncertain acknowledgement, never a rollback.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (dir, name) = parent(path, true, true, false)?;
    verify_optional(path)?;
    let temp = nonce_name("mother-write")?;
    let mut file = at(
        &dir,
        OsStr::new(&temp),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
        true,
        false,
    )?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        verify_optional(path)?;
        rfs::renameat(&dir, &temp, &dir, &name)?;
        dir.sync_all()
    })();
    if result.is_err() {
        let _ = rfs::unlinkat(&dir, &temp, rfs::AtFlags::empty());
    }
    result
}

/// Host executable validation happens before canonicalization can erase links.
pub fn executable(path: &Path) -> io::Result<File> {
    if !path.is_absolute() {
        return Err(refused("host executable must be absolute"));
    }
    let (dir, name) = parent(path, false, false, true)?;
    at(&dir, &name, OFlags::RDONLY, false, true)
}

/// Registry inputs may be readable, but cannot be symlinks or other-user writable.
pub fn trusted_input(path: &Path) -> io::Result<File> {
    let (dir, name) = parent(path, false, false, false)?;
    at(&dir, &name, OFlags::RDONLY, false, false)
}
pub fn trusted_directory(path: &Path) -> io::Result<()> {
    directory(path, false, false, false).map(|_| ())
}

#[cfg(test)]
mod tests;
