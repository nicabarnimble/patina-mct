use anyhow::{Context as _, Result, bail};
use sha2::{Digest as _, Sha256};
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let paths = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        bail!("usage: release-digests <file>...");
    }
    let mut records = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        records.push(serde_json::json!({
            "path": path,
            "size": bytes.len(),
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
            "blake3": blake3::hash(&bytes).to_hex().to_string(),
        }));
    }
    println!("{}", serde_json::to_string(&records)?);
    Ok(())
}
