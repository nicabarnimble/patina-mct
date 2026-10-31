//! Production macOS Git confinement. H1 remains open for Linux and review.
//!
//! Keep repository metadata untrusted. Rather than move it or promote it into
//! authority, constrain the native Git interpreter before it reads any of it.
//! The OS policy, executable selection, argv and environment are host-owned.
//! See docs/H1-git-toy-evidence.md for compatibility and assurance limitations.
use super::{Command, GitToyError, Path, PathBuf};
use std::{io, process::Stdio};

// No process-fork, network, Mach IPC, tracing or general process-exec grant.
// Only the initial, explicitly selected Git executable may exec. Git's ordinary
// helper paths require another process and cannot acquire that permission from
// config, attributes, environment, a gitfile, or a concurrent metadata rewrite.
// SBPL path values are parameters, NEVER interpolated into executable policy.
const POLICY: &str = r#"
(version 1)
(deny default)
(allow process-exec (literal (param "GIT")))
(allow file-read*
    (literal "/")
    (literal (param "GIT"))
    (subpath (param "REPO"))
    (subpath "/System/Library")
    (subpath "/System/Volumes/Preboot/Cryptexes/OS")
    (subpath "/usr/lib")
    (subpath "/usr/share")
    (subpath "/opt/homebrew/Cellar/git")
    (subpath "/opt/homebrew/Cellar/pcre2")
    (subpath "/opt/homebrew/Cellar/gettext")
    (subpath "/Library/Developer/CommandLineTools/usr/lib")
    (literal "/private/etc/localtime")
    (literal "/dev/null")
    (literal "/dev/urandom")
    (literal "/dev/random"))
(allow file-read-metadata (subpath "/opt/homebrew") (literal "/opt"))
(allow file-write* (subpath (param "REPO")) (literal "/dev/null"))
(allow sysctl-read)
"#;

fn unavailable(source: io::Error) -> GitToyError {
    GitToyError::RunGit { source }
}

pub(super) fn executable() -> Result<PathBuf, GitToyError> {
    // Supported host installations, in priority order. Never resolve Git from
    // ambient PATH (which may contain a checkout). A selected installation that
    // cannot run under this policy fails closed; there is no unconfined retry.
    for candidate in [
        "/opt/homebrew/bin/git",
        "/Library/Developer/CommandLineTools/usr/bin/git",
    ] {
        if Path::new(candidate).is_file() {
            return std::fs::canonicalize(candidate).map_err(unavailable);
        }
    }
    Err(unavailable(io::Error::new(
        io::ErrorKind::NotFound,
        "confined Git requires Homebrew arm64 Git or Command Line Tools Git",
    )))
}

// Compatibility restriction, not the security boundary: OS confinement must
// still hold if metadata changes after this check. No current grant authorizes
// gitfiles, linked worktrees, or symlinked metadata.
pub(super) fn validate_repository_layout(repo: &Path) -> Result<(), GitToyError> {
    let metadata =
        std::fs::symlink_metadata(repo.join(".git")).map_err(|_| GitToyError::GitFailed)?;
    if !metadata.is_dir() {
        return Err(GitToyError::RepositoryLayoutUnsupported);
    }
    Ok(())
}

pub(super) fn git_command(repo: &Path) -> Result<Command, GitToyError> {
    let root = std::fs::canonicalize(repo).map_err(unavailable)?;
    let git = executable()?;
    // Never make a trusted executable/library tree writable by selecting it as
    // the repository. This is a runtime-path guard, not an enrollment mechanism.
    for trusted in [
        Path::new("/System"),
        Path::new("/usr"),
        Path::new("/opt/homebrew"),
        Path::new("/Library/Developer"),
        Path::new("/dev"),
        Path::new("/private/etc/localtime"),
        git.as_path(),
    ] {
        if root.starts_with(trusted) || trusted.starts_with(&root) {
            return Err(unavailable(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "repository overlaps trusted runtime paths",
            )));
        }
    }
    validate_repository_layout(&root)?;

    let mut command = Command::new("/usr/bin/sandbox-exec");
    command.arg("-D").arg(format!("GIT={}", git.display()));
    command.arg("-D").arg(format!("REPO={}", root.display()));
    let mut policy = POLICY.to_owned();
    // Git canonicalizes ancestor directories. Grant metadata on those exact
    // ancestors, not access to their contents or to arbitrary host file metadata.
    policy.push_str("\n(allow file-read-metadata");
    for (i, ancestor) in root.ancestors().skip(1).enumerate() {
        command
            .arg("-D")
            .arg(format!("ANCESTOR_{i}={}", ancestor.display()));
        policy.push_str(&format!(" (literal (param \"ANCESTOR_{i}\"))"));
    }
    policy.push_str(")\n");
    command
        .args(["-p", &policy])
        .arg(git)
        .current_dir(&root)
        .stdin(Stdio::null())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["--no-pager", "--literal-pathspecs", "--git-dir"])
        .arg(root.join(".git"))
        .arg("--work-tree")
        .arg(&root)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "tag.gpgSign=false",
            "-c",
            "tag.forceSignAnnotated=false",
            "-c",
            "log.showSignature=false",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
        ]);
    Ok(command)
}
