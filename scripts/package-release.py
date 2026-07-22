#!/usr/bin/env python3
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import re
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path


def sha256_digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(64 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def digest(path: Path, algorithm: str) -> str:
    if algorithm == "sha256":
        return sha256_digest(path)
    result = subprocess.run(
        [str(args.digest_helper), str(path)],
        check=True,
        capture_output=True,
        text=True,
    )
    record = json.loads(result.stdout)
    if len(record) != 1 or record[0].get("path") != str(path):
        raise SystemExit("release digest helper returned an invalid path record")
    value = record[0].get("blake3", "")
    if not re.fullmatch(r"[0-9a-f]{64}", value):
        raise SystemExit("release digest helper returned an invalid BLAKE3")
    return value


def tagged(path: Path, algorithm: str) -> str:
    return f"{algorithm}:{digest(path, algorithm)}"


def regular_files(root: Path) -> list[Path]:
    result: list[Path] = []
    for path in root.rglob("*"):
        if path.is_symlink():
            raise SystemExit(f"release input contains symlink: {path}")
        if path.is_file():
            result.append(path)
        elif not path.is_dir():
            raise SystemExit(f"release input has unsupported type: {path}")
    return sorted(result, key=lambda path: path.relative_to(root).as_posix())


def canonical_json(path: Path, value: object) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=True, separators=(",", ":")),
        encoding="utf-8",
        newline="\n",
    )


parser = argparse.ArgumentParser(description="Assemble one normalized MCT release archive")
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--payload", type=Path, required=True)
parser.add_argument("--notes", type=Path, required=True)
parser.add_argument("--sbom", type=Path, required=True)
parser.add_argument("--provenance", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--target", required=True)
parser.add_argument("--version", required=True)
parser.add_argument("--source-commit", required=True)
parser.add_argument("--source-epoch", type=int, required=True)
parser.add_argument("--rust-version", required=True)
parser.add_argument("--cargo-version", required=True)
parser.add_argument("--release-mode", choices=("release", "smoke"), required=True)
parser.add_argument("--signing-mode", choices=("adhoc",), default="adhoc")
parser.add_argument("--executable-relative-path", required=True)
parser.add_argument("--digest-helper", type=Path, required=True)
args = parser.parse_args()

if not re.fullmatch(r"[0-9a-f]{40}", args.source_commit):
    raise SystemExit("source commit must be 40 lower-hex characters")
if args.source_epoch <= 0 or args.source_epoch > 0xFFFFFFFF:
    raise SystemExit("source epoch cannot be represented by canonical gzip")
for path in (
    args.source,
    args.payload,
    args.notes,
    args.sbom,
    args.provenance,
    args.digest_helper,
):
    path.resolve(strict=True)
args.output.mkdir(parents=True, exist_ok=True)

root_name = f"mct-daemon-v{args.version}-{args.target}"
archive_name = f"{root_name}.tar.gz"
archive_path = args.output / archive_name
for path in (
    archive_path,
    args.output / f"{archive_name}.sha256",
    args.output / f"{archive_name}.blake3",
):
    if path.exists():
        raise SystemExit(f"refusing to replace release output: {path}")

with tempfile.TemporaryDirectory(prefix="mct-release-package.") as temp_name:
    package_root = Path(temp_name) / root_name
    shutil.copytree(args.payload, package_root / "payload", symlinks=False)
    shutil.copyfile(args.notes, package_root / "RELEASE-NOTES.md")
    shutil.copyfile(args.sbom, package_root / "SBOM.cdx.json")
    shutil.copyfile(args.provenance, package_root / "FIXTURE-PROVENANCE.json")
    shutil.copyfile(args.source / "LICENSE", package_root / "LICENSE")

    executable = package_root / args.executable_relative_path
    if not executable.is_file() or executable.is_symlink():
        raise SystemExit("target adapter did not provide the manifest-selected executable")
    toolchain_text = (args.source / "rust-toolchain.toml").read_text(encoding="utf-8")
    toolchain_match = re.search(r'^channel\s*=\s*"([^"]+)"', toolchain_text, re.MULTILINE)
    if not toolchain_match:
        raise SystemExit("rust-toolchain.toml has no pinned channel")

    manifest = {
        "schema_version": 1,
        "package_format_version": 1,
        "release_mode": args.release_mode,
        "product": "mct-daemon",
        "product_version": args.version,
        "target_triple": args.target,
        "source_commit": args.source_commit,
        "source_epoch": args.source_epoch,
        "rust_toolchain": toolchain_match.group(1),
        "rust_version": args.rust_version,
        "cargo_version": args.cargo_version,
        "lockfile_sha256": tagged(args.source / "Cargo.lock", "sha256"),
        "executable_relative_path": args.executable_relative_path,
        "executable_sha256": tagged(executable, "sha256"),
        "executable_blake3": tagged(executable, "blake3"),
        "release_notes_sha256": tagged(package_root / "RELEASE-NOTES.md", "sha256"),
        "sbom_sha256": tagged(package_root / "SBOM.cdx.json", "sha256"),
        "fixture_provenance_sha256": tagged(
            package_root / "FIXTURE-PROVENANCE.json", "sha256"
        ),
        "distribution_license": "MIT",
        "signing_mode": args.signing_mode,
    }
    canonical_json(package_root / "RELEASE-MANIFEST.json", manifest)

    checksum_lines: list[str] = []
    for path in regular_files(package_root):
        relative = path.relative_to(package_root).as_posix()
        checksum_lines.extend(
            (f"blake3 {digest(path, 'blake3')} {relative}",
             f"sha256 {digest(path, 'sha256')} {relative}")
        )
    checksum_lines.sort()
    (package_root / "CHECKSUMS").write_text(
        "\n".join(checksum_lines) + "\n", encoding="utf-8", newline="\n"
    )

    all_paths = list(package_root.rglob("*"))
    if any(path.is_symlink() for path in all_paths):
        raise SystemExit("release package contains a symlink")
    directories = [package_root] + sorted(
        (path for path in all_paths if path.is_dir()),
        key=lambda path: (len(path.relative_to(package_root).parts), path.as_posix()),
    )
    files = regular_files(package_root)

    with archive_path.open("xb") as archive_output:
        with gzip.GzipFile(filename="", mode="wb", fileobj=archive_output, mtime=args.source_epoch) as gz:
            with tarfile.open(fileobj=gz, mode="w", format=tarfile.GNU_FORMAT) as tar:
                for path in directories + files:
                    archive_relative = Path(root_name)
                    if path != package_root:
                        archive_relative /= path.relative_to(package_root)
                    info = tarfile.TarInfo(archive_relative.as_posix())
                    info.uid = 0
                    info.gid = 0
                    info.uname = ""
                    info.gname = ""
                    info.mtime = args.source_epoch
                    if path.is_dir():
                        info.type = tarfile.DIRTYPE
                        info.mode = 0o755
                        info.size = 0
                        tar.addfile(info)
                    else:
                        info.type = tarfile.REGTYPE
                        info.mode = 0o755 if path == executable else 0o644
                        info.size = path.stat().st_size
                        with path.open("rb") as handle:
                            tar.addfile(info, handle)

for algorithm in ("sha256", "blake3"):
    sidecar = args.output / f"{archive_name}.{algorithm}"
    sidecar.write_text(
        f"{digest(archive_path, algorithm)}  {archive_name}\n",
        encoding="utf-8",
        newline="\n",
    )
print(archive_path)
