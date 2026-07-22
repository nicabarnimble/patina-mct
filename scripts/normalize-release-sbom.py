#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import uuid
from pathlib import Path

FIXTURES = [
    {
        "name": "slate-manager",
        "version": "0.2.0",
        "classification": "exact-tag raw build output",
        "receipt": "crates/mct-daemon/tests/fixtures/slate-manager-0.2.0/PROVENANCE.md",
        "files": [
            "crates/mct-daemon/tests/fixtures/slate-manager-0.2.0/slate-manager.toml",
            "crates/mct-daemon/tests/fixtures/slate-manager-0.2.0/slate-manager.wasm",
        ],
        "patch": None,
    },
    {
        "name": "folder-watch-actor",
        "version": "0.1.0",
        "classification": "source-derived MCT security rebuild",
        "receipt": "crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/PROVENANCE.md",
        "files": [
            "crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/child.toml",
            "crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/folder-watch-actor.wasm",
        ],
        "patch": "crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/MCT-REBUILD.patch",
    },
    {
        "name": "watch-null-sink",
        "version": "0.1.0",
        "classification": "unmodified exact-tag source build",
        "receipt": "crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0/PROVENANCE.md",
        "files": [
            "crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0/child.toml",
            "crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0/watch-null-sink.wasm",
        ],
        "patch": None,
    },
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--raw", type=Path, required=True)
    parser.add_argument("--digests", type=Path, required=True)
    parser.add_argument("--sbom-output", type=Path, required=True)
    parser.add_argument("--provenance-output", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--source-epoch", type=int, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    return parser.parse_args()


def first_backtick_value(text: str, labels: tuple[str, ...]) -> str:
    for label in labels:
        match = re.search(rf"^- {re.escape(label)}: `([^`]+)`", text, re.MULTILINE)
        if match:
            return match.group(1)
    raise ValueError(f"fixture receipt missing one of {labels}")


def rust_toolchain(text: str) -> str:
    labeled = re.search(r"^- Rust: `([^`]+)`", text, re.MULTILINE)
    if labeled:
        return labeled.group(1)
    bullet = re.search(r"^- `(rustc [^`]+)`", text, re.MULTILINE)
    if bullet:
        return bullet.group(1)
    raise ValueError("fixture receipt missing Rust toolchain")


def build_command(text: str) -> str:
    bullet = re.search(r"^- Build command: `([^`]+)`", text, re.MULTILINE)
    if bullet:
        return bullet.group(1)
    block = re.search(r"Exact command[^:]*:\s*```text\s*([^\n]+)\s*```", text, re.DOTALL)
    if block:
        return block.group(1).strip()
    raise ValueError("fixture receipt missing exact build command")


def fixture_records(root: Path, digest_records: dict[str, dict]) -> list[dict]:
    output = []
    for fixture in FIXTURES:
        receipt_path = root / fixture["receipt"]
        receipt = receipt_path.read_text(encoding="utf-8")
        files = []
        for relative in fixture["files"]:
            record = digest_records.get(relative)
            if record is None:
                raise ValueError(f"missing digest record for {relative}")
            size = record["size"]
            if str(size) not in receipt and f"{size:,}" not in receipt:
                raise ValueError(f"receipt does not contain current byte size for {relative}")
            for algorithm in ("sha256", "blake3"):
                if record[algorithm] not in receipt:
                    raise ValueError(f"receipt does not contain current {algorithm} for {relative}")
            files.append(
                {
                    "path": relative,
                    "size_bytes": size,
                    "sha256": record["sha256"],
                    "blake3": record["blake3"],
                }
            )
        patch = None
        if fixture["patch"]:
            relative = fixture["patch"]
            record = digest_records.get(relative)
            if record is None:
                raise ValueError(f"missing digest record for {relative}")
            for value in (str(record["size"]), f"{record['size']:,}"):
                if value in receipt:
                    break
            else:
                raise ValueError("patch receipt does not contain current byte size")
            if record["sha256"] not in receipt or record["blake3"] not in receipt:
                raise ValueError("patch receipt does not contain current digests")
            patch = {
                "path": relative,
                "size_bytes": record["size"],
                "sha256": record["sha256"],
                "blake3": record["blake3"],
                "application": "git apply --check then git apply with zero fuzz",
            }
        output.append(
            {
                "name": fixture["name"],
                "version": fixture["version"],
                "classification": fixture["classification"],
                "receipt_path": fixture["receipt"],
                "upstream_repository": first_backtick_value(
                    receipt, ("Upstream repository", "Repository")
                ),
                "upstream_commit": first_backtick_value(receipt, ("Commit",)),
                "upstream_tag": first_backtick_value(receipt, ("Tag",)),
                "build_command": build_command(receipt),
                "rust_toolchain": rust_toolchain(receipt),
                "scope": "excluded",
                "files": files,
                "patch": patch,
            }
        )
    return output


def sort_component(component: dict) -> dict:
    for field in ("externalReferences", "licenses", "hashes", "properties"):
        if isinstance(component.get(field), list):
            component[field].sort(key=lambda item: json.dumps(item, sort_keys=True))
    if isinstance(component.get("components"), list):
        component["components"].sort(key=lambda item: item.get("bom-ref", ""))
    return component


def fixture_component(fixture: dict) -> dict:
    primary = fixture["files"][-1]
    properties = [
        {"name": "mct:fixture:proof-only", "value": "true"},
        {"name": "mct:fixture:classification", "value": fixture["classification"]},
        {"name": "mct:fixture:receipt", "value": fixture["receipt_path"]},
        {"name": "mct:fixture:blake3", "value": primary["blake3"]},
    ]
    if fixture["patch"]:
        properties.extend(
            [
                {"name": "mct:fixture:patch-sha256", "value": fixture["patch"]["sha256"]},
                {"name": "mct:fixture:patch-blake3", "value": fixture["patch"]["blake3"]},
            ]
        )
    return sort_component(
        {
            "type": "file",
            "bom-ref": f"mct-fixture:{fixture['name']}@{fixture['version']}",
            "name": fixture["name"],
            "version": fixture["version"],
            "scope": "excluded",
            "hashes": [{"alg": "SHA-256", "content": primary["sha256"]}],
            "externalReferences": [
                {"type": "vcs", "url": fixture["upstream_repository"]}
            ],
            "properties": properties,
        }
    )


def normalize(args: argparse.Namespace) -> None:
    root = Path.cwd()
    raw = json.loads(args.raw.read_text(encoding="utf-8"))
    if raw.get("bomFormat") != "CycloneDX" or raw.get("specVersion") != "1.6":
        raise ValueError("cargo-sbom output is not CycloneDX 1.6")
    digest_list = json.loads(args.digests.read_text(encoding="utf-8"))
    digest_records = {record["path"]: record for record in digest_list}
    if len(digest_records) != len(digest_list):
        raise ValueError("duplicate fixture digest paths")
    fixtures = fixture_records(root, digest_records)

    timestamp = dt.datetime.fromtimestamp(args.source_epoch, tz=dt.timezone.utc)
    raw["serialNumber"] = "urn:uuid:" + str(
        uuid.uuid5(
            uuid.NAMESPACE_URL,
            f"https://github.com/nicabarnimble/patina-mct/{args.version}/{args.target}/{args.source_commit}",
        )
    )
    metadata = raw.setdefault("metadata", {})
    metadata["timestamp"] = timestamp.strftime("%Y-%m-%dT%H:%M:%SZ")
    metadata["authors"] = [{"name": "MCT release tooling"}]
    application = metadata.setdefault("component", {})
    application.update(
        {
            "type": "application",
            "name": "mct-daemon",
            "version": args.version,
            "bom-ref": f"mct-daemon:{args.version}:{args.target}",
        }
    )
    application["properties"] = [
        {"name": "mct:release:source-commit", "value": args.source_commit},
        {"name": "mct:release:target", "value": args.target},
    ]
    if isinstance(application.get("components"), list):
        application["components"].sort(key=lambda item: item.get("bom-ref", ""))
    if isinstance(metadata.get("tools"), list):
        metadata["tools"].sort(key=lambda item: json.dumps(item, sort_keys=True))

    components = [sort_component(component) for component in raw.get("components", [])]
    components.extend(fixture_component(fixture) for fixture in fixtures)
    components.sort(key=lambda item: item.get("bom-ref", ""))
    refs = [component.get("bom-ref") for component in components]
    if None in refs or len(refs) != len(set(refs)):
        raise ValueError("SBOM contains missing or duplicate component identities")
    raw["components"] = components

    dependencies = raw.get("dependencies", [])
    for dependency in dependencies:
        if isinstance(dependency.get("dependsOn"), list):
            dependency["dependsOn"].sort()
    dependencies.sort(key=lambda item: item.get("ref", ""))
    dependency_refs = [item.get("ref") for item in dependencies]
    if None in dependency_refs or len(dependency_refs) != len(set(dependency_refs)):
        raise ValueError("SBOM contains missing or duplicate dependency identities")
    raw["dependencies"] = dependencies

    provenance = {
        "schema_version": 1,
        "scope": "release-proof inputs excluded from runtime payload",
        "source_commit": args.source_commit,
        "target_triple": args.target,
        "fixtures": fixtures,
    }
    args.sbom_output.parent.mkdir(parents=True, exist_ok=True)
    args.provenance_output.parent.mkdir(parents=True, exist_ok=True)
    args.sbom_output.write_text(
        json.dumps(raw, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    args.provenance_output.write_text(
        json.dumps(provenance, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    normalize(parse_args())
