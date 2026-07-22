use anyhow::{Context as _, Result, bail};
use mct_daemon::verify_and_extract_daemon_release_archive;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let archive = PathBuf::from(args.next().context("missing release archive path")?);
    let destination = PathBuf::from(args.next().context("missing extraction destination")?);
    let target = args.next().context("missing expected target triple")?;
    if args.next().is_some() {
        bail!("usage: verify-release <archive> <destination> <expected-target>");
    }
    let verified =
        verify_and_extract_daemon_release_archive(&archive, &destination, None, &target)?;
    println!(
        "verified release={} version={} target={} archive_sha256={} archive_blake3={} executable={}",
        verified.manifest.product,
        verified.manifest.product_version,
        verified.manifest.target_triple,
        verified.archive_sha256,
        verified.archive_blake3,
        verified.executable_path.display()
    );
    Ok(())
}
