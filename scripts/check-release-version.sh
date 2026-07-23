#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

require_tag=false
if [[ ${1:-} == "--require-tag" ]]; then
  require_tag=true
  shift
fi
if (($# != 0)); then
  printf 'usage: %s [--require-tag]\n' "$0" >&2
  exit 2
fi

python3 - "$require_tag" <<'PY'
from __future__ import annotations

import re
import subprocess
import sys
import tomllib
from pathlib import Path

require_tag = sys.argv[1] == "true"
root = Path.cwd()
errors: list[str] = []

with (root / "Cargo.toml").open("rb") as handle:
    workspace = tomllib.load(handle)
version = workspace.get("workspace", {}).get("package", {}).get("version")
if not isinstance(version, str):
    errors.append("Cargo.toml [workspace.package].version is missing")
    version = "<missing>"

members = workspace.get("workspace", {}).get("members", [])
if not members:
    errors.append("Cargo.toml workspace has no members")
for member in members:
    manifest_path = root / member / "Cargo.toml"
    with manifest_path.open("rb") as handle:
        manifest = tomllib.load(handle)
    package_version = manifest.get("package", {}).get("version")
    if package_version != {"workspace": True}:
        errors.append(
            f"{manifest_path.relative_to(root)} package.version must inherit workspace version"
        )

if version != "<missing>":
    with (root / "Cargo.lock").open("rb") as handle:
        lock = tomllib.load(handle)
    local_names = {Path(member).name for member in members}
    lock_versions = {
        package["name"]: package["version"]
        for package in lock.get("package", [])
        if package.get("name") in local_names
    }
    for name in sorted(local_names):
        if lock_versions.get(name) != version:
            errors.append(
                f"Cargo.lock {name} version is {lock_versions.get(name)!r}, expected {version}"
            )

changelog_path = root / "CHANGELOG.md"
if not changelog_path.is_file():
    errors.append("CHANGELOG.md is missing")
else:
    changelog = changelog_path.read_text(encoding="utf-8")
    headings = re.findall(
        r"^## \[(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)\](?: - \d{4}-\d{2}-\d{2})?$",
        changelog,
        flags=re.MULTILINE,
    )
    if version != "<missing>" and version not in headings:
        errors.append(f"CHANGELOG.md has no released [{version}] heading")
    if version != "<missing>":
        def semver_key(value: str) -> tuple[int, int, int, int, str]:
            core, separator, prerelease = value.partition("-")
            major, minor, patch = (int(part) for part in core.split("."))
            return major, minor, patch, 0 if separator else 1, prerelease

        higher = [item for item in headings if semver_key(item) > semver_key(version)]
        if higher:
            errors.append(
                "CHANGELOG.md contains released versions newer than workspace version: "
                + ", ".join(higher)
            )

release_tags = subprocess.run(
    ["git", "tag", "--points-at", "HEAD", "--list", "v[0-9]*"],
    check=True,
    text=True,
    stdout=subprocess.PIPE,
).stdout.splitlines()
expected_tag = f"v{version}"
if require_tag and expected_tag not in release_tags:
    errors.append(f"release build requires exact annotated tag {expected_tag} at HEAD")
for tag in release_tags:
    if tag != expected_tag:
        errors.append(f"release tag {tag} at HEAD disagrees with workspace version {version}")
    object_type = subprocess.run(
        ["git", "cat-file", "-t", f"refs/tags/{tag}"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()
    if object_type != "tag":
        errors.append(f"release tag {tag} must be annotated, found object type {object_type}")

if errors:
    for error in errors:
        print(f"release-version: {error}", file=sys.stderr)
    raise SystemExit(1)

print(f"release-version: workspace, lockfile, changelog, and tag state agree at {version}")
PY
