#![cfg_attr(target_arch = "wasm32", no_std)]
extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec::Vec,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const INPUT_LIMIT: usize = 1024 * 1024;
const OUTPUT_LIMIT: usize = 32768;
const PACKAGE_LIMIT: usize = 8192;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportRequestV1 {
    sha256: String,
}

#[derive(Serialize)]
struct ReportV1 {
    schema: &'static str,
    sha256: String,
    package_count: usize,
    registry_count: usize,
    git_count: usize,
    local_count: usize,
    duplicates: BTreeMap<String, Vec<String>>,
    selected: BTreeMap<String, Vec<String>>,
}

fn request(input: &str) -> Result<ReportRequestV1, &'static str> {
    if input.len() > 256 {
        return Err("invalid request");
    }
    let r: ReportRequestV1 = serde_json::from_str(input).map_err(|_| "invalid request")?;
    if r.sha256.len() != 64
        || !r
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid digest");
    }
    Ok(r)
}

#[derive(Default)]
struct Package {
    name: Option<String>,
    version: Option<String>,
    source: Option<String>,
    checksum: bool,
    dependencies: bool,
}
fn value(line: &str) -> Result<String, &'static str> {
    let s: String = serde_json::from_str(line).map_err(|_| "invalid lockfile")?;
    if s.is_empty() || s.len() > 512 || s.chars().any(char::is_control) {
        return Err("invalid lockfile");
    }
    Ok(s)
}
fn finish(
    p: Package,
    packages: &mut Vec<(String, String, Option<String>)>,
) -> Result<(), &'static str> {
    if packages.len() >= PACKAGE_LIMIT {
        return Err("too many packages");
    }
    packages.push((
        p.name.ok_or("invalid lockfile")?,
        p.version.ok_or("invalid lockfile")?,
        p.source,
    ));
    Ok(())
}

// Cargo-generated lockfiles use this deliberately narrow subset of TOML.
fn parse(bytes: &[u8]) -> Result<Vec<(String, String, Option<String>)>, &'static str> {
    let text = core::str::from_utf8(bytes).map_err(|_| "invalid lockfile")?;
    let mut packages = Vec::new();
    let mut current: Option<Package> = None;
    let mut version = false;
    let mut array = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if array {
            if line == "]" {
                array = false;
                continue;
            }
            let item = line.strip_suffix(',').ok_or("invalid lockfile")?.trim();
            value(item)?;
            continue;
        }
        if line == "[[package]]" {
            if !version {
                return Err("invalid lockfile");
            }
            if let Some(p) = current.take() {
                finish(p, &mut packages)?;
            }
            current = Some(Package::default());
            continue;
        }
        let (key, val) = line.split_once('=').ok_or("invalid lockfile")?;
        let (key, val) = (key.trim(), val.trim());
        if let Some(p) = current.as_mut() {
            match key {
                "name" if p.name.is_none() => p.name = Some(value(val)?),
                "version" if p.version.is_none() => p.version = Some(value(val)?),
                "source" if p.source.is_none() => p.source = Some(value(val)?),
                "checksum" if !p.checksum => {
                    value(val)?;
                    p.checksum = true;
                }
                "dependencies" if !p.dependencies && val == "[" => {
                    p.dependencies = true;
                    array = true;
                }
                "dependencies" if !p.dependencies && val == "[]" => p.dependencies = true,
                _ => return Err("invalid lockfile"),
            }
        } else if key == "version" && !version && matches!(val, "3" | "4") {
            version = true;
        } else {
            return Err("invalid lockfile");
        }
    }
    if array || !version {
        return Err("invalid lockfile");
    }
    if let Some(p) = current {
        finish(p, &mut packages)?;
    }
    if packages.is_empty() {
        return Err("invalid lockfile");
    }
    Ok(packages)
}

pub fn inspect_bytes(input: &str, bytes: &[u8]) -> Result<String, &'static str> {
    let req = request(input)?;
    if bytes.len() > INPUT_LIMIT {
        return Err("input too large");
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    if digest != req.sha256 {
        return Err("digest mismatch");
    }
    let packages = parse(bytes)?;
    let mut versions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let (mut registry, mut git, mut local) = (0, 0, 0);
    for (name, version, source) in &packages {
        match source.as_deref() {
            None => local += 1,
            Some(s) if s.starts_with("registry+") || s.starts_with("sparse+") => registry += 1,
            Some(s) if s.starts_with("git+") => git += 1,
            _ => return Err("unknown source"),
        }
        versions
            .entry(name.clone())
            .or_default()
            .insert(version.clone());
    }
    let mut duplicates = BTreeMap::new();
    let mut selected = BTreeMap::new();
    for (name, values) in versions {
        let v: Vec<String> = values.into_iter().collect();
        if v.len() > 1 {
            duplicates.insert(name.clone(), v.clone());
        }
        if matches!(
            name.as_str(),
            "iroh" | "rusqlite" | "wasmtime" | "wasmtime-wasi"
        ) {
            selected.insert(name, v);
        }
    }
    let report = ReportV1 {
        schema: "mct-repo-report/v1",
        sha256: digest,
        package_count: packages.len(),
        registry_count: registry,
        git_count: git,
        local_count: local,
        duplicates,
        selected,
    };
    let json = serde_json::to_string(&report).map_err(|_| "report error")?;
    let envelope = serde_json::json!({"results": [{"ok": &json}]});
    if serde_json::to_vec(&envelope)
        .map_err(|_| "report error")?
        .len()
        >= OUTPUT_LIMIT
    {
        return Err("output too large");
    }
    Ok(json)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn inspect_in(root: &std::path::Path, input: &str) -> Result<String, &'static str> {
    use std::io::Read;
    request(input)?;
    let file = std::fs::File::open(root.join("Cargo.lock")).map_err(|_| "input unavailable")?;
    let mut bytes = Vec::new();
    file.take((INPUT_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "input unavailable")?;
    inspect_bytes(input, &bytes)
}

#[cfg(target_arch = "wasm32")]
mod component;
#[cfg(test)]
mod tests;
