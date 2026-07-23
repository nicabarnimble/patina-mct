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

import json
import re
import subprocess
import sys
from pathlib import Path

require_tag = sys.argv[1] == "true"
root = Path.cwd()
errors: list[str] = []


def table_body(path: Path, table: str):
    text = path.read_text(encoding="utf-8")
    match = re.search(
        rf"(?ms)^\[{re.escape(table)}\][ \t]*(?:#.*)?$\n(.*?)(?=^\[|\Z)",
        text,
    )
    return match.group(1) if match else None


workspace_package = table_body(root / "Cargo.toml", "workspace.package")
version_match = (
    re.search(
        r'^\s*version\s*=\s*"([^"\r\n]+)"\s*(?:#.*)?$',
        workspace_package,
        flags=re.MULTILINE,
    )
    if workspace_package is not None
    else None
)
version = version_match.group(1) if version_match else "<missing>"
if version == "<missing>":
    errors.append("Cargo.toml [workspace.package].version is missing")

metadata_process = subprocess.run(
    ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
if metadata_process.returncode != 0:
    errors.append(
        "cargo metadata --locked rejected workspace/lockfile state: "
        + metadata_process.stderr.strip()
    )
    workspace_packages = []
else:
    metadata = json.loads(metadata_process.stdout)
    workspace_members = set(metadata.get("workspace_members", []))
    workspace_packages = [
        package
        for package in metadata.get("packages", [])
        if package.get("id") in workspace_members
    ]
    if not workspace_packages:
        errors.append("Cargo.toml workspace has no members")

for package in workspace_packages:
    manifest_path = Path(package["manifest_path"])
    package_table = table_body(manifest_path, "package")
    inherits_version = package_table is not None and re.search(
        r"^\s*version\.workspace\s*=\s*true\s*(?:#.*)?$",
        package_table,
        flags=re.MULTILINE,
    )
    if not inherits_version:
        errors.append(
            f"{manifest_path.relative_to(root)} package.version must inherit workspace version"
        )
    if version != "<missing>" and package.get("version") != version:
        errors.append(
            f"Cargo.lock {package.get('name')} version is {package.get('version')!r}, "
            f"expected {version}"
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
