#!/usr/bin/env python3
"""Exercise the packaged resident through its owner-authenticated UDS surface."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
from datetime import datetime, timedelta, timezone


def fail(message: str) -> None:
    raise RuntimeError(message)


parser = argparse.ArgumentParser()
parser.add_argument("--binary", required=True, type=Path)
parser.add_argument("--harness", required=True, type=Path)
parser.add_argument("--digest-helper", required=True, type=Path)
parser.add_argument("--root", required=True, type=Path)
parser.add_argument("--fixtures", required=True, type=Path)
parser.add_argument("--primary-archive", required=True, type=Path)
args = parser.parse_args()

root = args.root.resolve()
socket_path = root / "control.sock"
config = root / "config.json"
children = root / "children"
state = root / "state.sqlite"
ledger = root / "observations.jsonl"


def request(method: str, path: str, value: object) -> tuple[int, object]:
    body = json.dumps(value, separators=(",", ":")).encode()
    wire = (
        f"{method} {path} HTTP/1.1\r\n"
        f"Host: local\r\nContent-Type: application/json\r\n"
        f"Content-Length: {len(body)}\r\n\r\n"
    ).encode() + body
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(20)
    client.connect(str(socket_path))
    client.sendall(wire)
    client.shutdown(socket.SHUT_WR)
    chunks = []
    while True:
        chunk = client.recv(65536)
        if not chunk:
            break
        chunks.append(chunk)
    client.close()
    response = b"".join(chunks)
    headers, response_body = response.split(b"\r\n\r\n", 1)
    status = int(headers.split()[1])
    decoded = json.loads(response_body) if response_body else None
    return status, decoded


def expect(status: int, body: object, wanted: int, label: str) -> object:
    if status != wanted:
        fail(f"{label}: expected HTTP {wanted}, got {status}: {body!r}")
    return body


def digest_bytes(data: bytes) -> str:
    with tempfile.NamedTemporaryFile(dir=root, delete=False) as handle:
        handle.write(data)
        path = Path(handle.name)
    try:
        output = subprocess.check_output([args.digest_helper, path], text=True)
        return json.loads(output)[0]["blake3"]
    finally:
        path.unlink(missing_ok=True)


def submission(suffix: str, namespace: str, interface: str, function: str, payload: object) -> dict:
    payload_bytes = json.dumps(payload, separators=(",", ":")).encode()
    digest = digest_bytes(payload_bytes)
    return {
        "protocol_request_id": f"proto-release-smoke-{suffix}",
        "call_id": f"call-release-smoke-{suffix}",
        "target": {
            "namespace": namespace,
            "interface_name": interface,
            "function_name": function,
        },
        "payload_metadata": {
            "data_classification": "public",
            "size_bytes": len(payload_bytes),
            "contains_secret_scoped_material": False,
        },
        "authority_context": {
            "policy_revision": 1,
            "grants_revision": 1,
            "vision_policy_revision": 1,
        },
        "deadline": "2099-01-01T00:00:00Z",
        "trace_context": {
            "trace_id": f"trace-release-smoke-{suffix}",
            "span_id": f"span-release-smoke-{suffix}",
        },
        "payload": {
            "payload_kind": "inline_payload",
            "inline_payload_ref": f"payload-release-smoke-{suffix}",
            "content_type": "application/json",
            "size_bytes": len(payload_bytes),
            "blake3_digest_hex": digest,
        },
        "inline_payload_base64": base64.b64encode(payload_bytes).decode(),
        "idempotency_key": f"release-smoke-{suffix}",
    }


def slate_submission(suffix: str) -> dict:
    return submission(
        f"slate-{suffix}",
        "patina:slate",
        "control@0.1.0",
        "list-work",
        [{"project": "/project", "status": None, "kind": None}],
    )


def stage(child: str, version: str, source: Path, manifest: str, component: str) -> str:
    value = {
        "source_root": str(source),
        "manifest_path": manifest,
        "component_path": component,
        "claimed_child_name": child,
        "claimed_artifact_version": version,
        "expected_digest": None,
        "standing_source_authority_id": None,
        "claimed_publisher": None,
        "require_source_sidecars": False,
        "children_dir": str(children),
        "state_path": str(state),
    }
    body = expect(*request("POST", "/artifacts/stage", value), 200, f"stage {child}")
    if body.get("verification_outcome") != "verified":
        fail(f"stage {child} was not verified: {body!r}")
    return body["artifact_id"]


def approve(child: str, artifact_id: str) -> None:
    value = {
        "expected_config_path": str(config),
        "expected_children_dir": str(children),
        "expected_state_path": str(state),
        "expected_artifact_id": artifact_id,
        "child_name": child,
        "strict_integrity": True,
    }
    body = expect(*request("POST", "/children/approve", value), 200, f"approve {child}")
    if body.get("approval_state") != "approved" or body.get("assignment_state") != "active":
        fail(f"approval {child} was incomplete: {body!r}")


def summary() -> dict:
    output = subprocess.check_output(
        [args.binary, "state", "summary", "--state", state, "--json"], text=True
    )
    return json.loads(output)


def lifecycle(action: str) -> None:
    subprocess.run(
        [args.harness, "release-smoke-internal", action, "--root", root], check=True
    )


# Give the installed bytes immutable daemon-release evidence without replacing them.
primary_bytes = args.primary_archive.read_bytes()
primary_sha256 = hashlib.sha256(primary_bytes).hexdigest()
primary = expect(
    *request("POST", "/releases/acquire", {
        "source_path": str(args.primary_archive.resolve()),
        "expected_archive_identity": f"sha256:{primary_sha256}",
        "target_triple": "aarch64-apple-darwin",
        "releases_dir": str(root / "releases"),
        "state_path": str(state),
        "ledger_path": str(ledger),
        "authenticated_uid": os.getuid(),
        "policy_revision": 1,
    }),
    200,
    "Primary daemon release acquisition",
)
if primary.get("artifact", {}).get("release_artifact_id") != f"sha256:{primary_sha256}":
    fail(f"primary daemon release identity mismatch: {primary!r}")
if summary().get("daemon_release_artifacts") != 1 or summary().get("artifacts") != 0:
    fail(f"daemon release acquisition projected Child authority: {summary()!r}")

# Copy proof inputs so the resident never reads the repository fixtures in place.
source_root = root / "fixture-sources"
source_root.mkdir()
fixture_copies: dict[str, Path] = {}
fixture_copy_hashes: dict[Path, str] = {}
for child, version, manifest, component in [
    ("slate-manager", "0.2.0", "slate-manager.toml", "slate-manager.wasm"),
    ("folder-watch-actor", "0.1.0", "child.toml", "folder-watch-actor.wasm"),
    ("watch-null-sink", "0.1.0", "child.toml", "watch-null-sink.wasm"),
]:
    source = source_root / child
    source.mkdir()
    fixture = args.fixtures / f"{child}-{version}"
    shutil.copy2(fixture / manifest, source / manifest)
    shutil.copy2(fixture / component, source / component)
    for copied in [source / manifest, source / component]:
        fixture_copy_hashes[copied] = hashlib.sha256(copied.read_bytes()).hexdigest()
    fixture_copies[child] = source

project = root / "slate-project"
work = project / "layer" / "slate" / "work" / "fixture-work"
(project / ".patina").mkdir(parents=True)
work.mkdir(parents=True)
(work / "work.toml").write_text(
    'id = "fixture-work"\ntitle = "Packaged release smoke"\nkind = "build"\nstatus = "active"\n'
)
subprocess.run(["git", "init", "-q", project], check=True)

slate_artifact = stage(
    "slate-manager", "0.2.0", fixture_copies["slate-manager"],
    "slate-manager.toml", "slate-manager.wasm"
)
body = expect(*request("POST", "/calls", slate_submission("before-approval")), 200, "Slate pre-approval")
if body.get("outcome") != "denied":
    fail(f"Slate was not denied before approval: {body!r}")
approve("slate-manager", slate_artifact)
body = expect(*request("POST", "/calls", slate_submission("before-grants")), 200, "Slate pre-grant")
if body.get("outcome") != "denied":
    fail(f"Slate was not denied before ToyGrants: {body!r}")
body = expect(
    *request("POST", "/toys/authorize-slate", {
        "expected_config_path": str(config),
        "expected_children_dir": str(children),
        "expected_state_path": str(state),
        "child_name": "slate-manager",
        "project_root": str(project),
    }),
    200,
    "Slate ToyGrants",
)
if body.get("grants") != 4:
    fail(f"Slate did not receive the exact four grants: {body!r}")
body = expect(*request("POST", "/calls", slate_submission("allowed")), 200, "Slate allowed call")
if body.get("outcome") != "completed":
    fail(f"Slate call did not complete: {body!r}")
result = json.loads(base64.b64decode(body["inline_result_payload_base64"]))
if "fixture-work" not in json.dumps(result):
    fail(f"Slate result omitted fixture work: {result!r}")

watch_artifacts = {}
for child, component in [
    ("folder-watch-actor", "folder-watch-actor.wasm"),
    ("watch-null-sink", "watch-null-sink.wasm"),
]:
    artifact = stage(child, "0.1.0", fixture_copies[child], "child.toml", component)
    approve(child, artifact)
    watch_artifacts[child] = artifact

watch_root = root / "watch-input"
watch_root.mkdir()
body = expect(
    *request("POST", "/watch/grant", {
        "expected_config_path": str(config),
        "expected_children_dir": str(children),
        "expected_state_path": str(state),
        "child_name": "folder-watch-actor",
        "watch_scope_id": "scope:release-smoke-watch",
        "canonical_root": str(watch_root),
        "scope_mode": "constrained",
        "traversal_scope": "recursive",
        "event_classes": ["created", "modified", "deleted"],
        "max_events_per_batch": 16,
        "coalescing_policy": "none",
        "starts_at": "2020-01-01T00:00:00Z",
        "expires_at": "2099-01-01T00:00:00Z",
    }),
    200,
    "Watch grant",
)
if body.get("scope", {}).get("authority_state") != "active":
    fail(f"Watch scope was not active: {body!r}")

for grant in [
    {"kind": "directory_read", "canonical_root": str(watch_root)},
    {"kind": "keyvalue", "bucket_name": "default"},
    {"kind": "observability", "logging": True, "measure": True},
]:
    expect(
        *request("POST", "/watch/supporting-grant", {
            "expected_config_path": str(config),
            "expected_children_dir": str(children),
            "expected_state_path": str(state),
            "child_name": "folder-watch-actor",
            "expires_at": "2099-01-01T00:00:00Z",
            "grant": grant,
        }),
        200,
        "Watch supporting grant",
    )
expect(
    *request("POST", "/watch/supporting-grant", {
        "expected_config_path": str(config),
        "expected_children_dir": str(children),
        "expected_state_path": str(state),
        "child_name": "watch-null-sink",
        "expires_at": "2099-01-01T00:00:00Z",
        "grant": {"kind": "observability", "logging": True, "measure": True},
    }),
    200,
    "Watch sink grant",
)

configure = submission(
    "watch-configure", "patina:watch", "control@0.1.0", "configure",
    [{"watch-path": "/input", "stream-name": "patina:watch/events@0.1.0.emit",
      "recursive": True, "include-hidden": False, "emit-existing-on-start": True,
      "extensions": []}, True],
)
body = expect(*request("POST", "/calls", configure), 200, "Watch configure")
if body.get("outcome") != "completed":
    fail(f"Watch configure failed: {body!r}")
(watch_root / "fixture-created.txt").write_text("watch fixture content")
scan = submission("watch-scan", "patina:watch", "control@0.1.0", "scan-now", [])
body = expect(*request("POST", "/calls", scan), 200, "Watch scan")
if body.get("outcome") != "completed":
    fail(f"Watch scan failed: {body!r}")
if summary().get("watch_event_deliveries") != 1:
    fail(f"first Watch delivery count was not one: {summary()!r}")

(watch_root / "trigger-created.txt").write_text("temporal watch fixture content")
trigger_payload = b"[]"
trigger_digest = digest_bytes(trigger_payload)
blob = expect(
    *request("POST", "/blobs", {
        "digest": trigger_digest,
        "size_bytes": len(trigger_payload),
        "content_type": "application/json",
        "classification": "trigger-static",
        "bytes_base64": base64.b64encode(trigger_payload).decode(),
    }),
    201,
    "Trigger payload blob",
)
now = datetime.now(timezone.utc)
anchor = now + timedelta(milliseconds=750)
timestamp = lambda value: value.isoformat(timespec="microseconds").replace("+00:00", "Z")
trigger = {
    "expected_config_path": str(config),
    "expected_state_path": str(state),
    "scope": {
        "trigger_authority_id": "trigger:release-smoke-watch",
        "target": {
            "namespace": "patina:watch",
            "interface_name": "control@0.1.0",
            "function_name": "scan-now",
        },
        "payload_constraint": blob["payload"],
        "trigger_source": {
            "source_kind": "temporal",
            "anchor_at": timestamp(anchor),
            "interval_ms": 60000,
        },
        "missed_fire_policy": "skip",
        "overlap_policy": "refuse",
        "starts_at": timestamp(now),
        "expires_at": "2099-01-01T00:00:00Z",
    },
}
expect(*request("POST", "/triggers/create", trigger), 200, "Temporal trigger create")
deadline = time.monotonic() + 12
while time.monotonic() < deadline and summary().get("watch_event_deliveries") != 2:
    time.sleep(0.1)
if summary().get("watch_event_deliveries") != 2:
    fail(f"temporal Watch delivery did not complete: {summary()!r}")
expect(
    *request("POST", "/triggers/revoke", {
        "expected_config_path": str(config),
        "expected_state_path": str(state),
        "trigger_authority_id": "trigger:release-smoke-watch",
        "expected_revision": 1,
    }),
    200,
    "Temporal trigger revoke",
)
expect(
    *request("POST", "/watch/revoke", {
        "expected_config_path": str(config),
        "expected_state_path": str(state),
        "watch_scope_id": "scope:release-smoke-watch",
        "expected_revision": 1,
    }),
    200,
    "Watch scope revoke",
)
(watch_root / "after-revoke.txt").write_text("must not observe")
body = expect(
    *request("POST", "/calls", submission(
        "watch-after-revoke", "patina:watch", "control@0.1.0", "scan-now", []
    )),
    200,
    "Watch call after revoke",
)
if body.get("outcome") != "denied" or summary().get("watch_event_deliveries") != 2:
    fail(f"revoked Watch authority still had effect: {body!r} {summary()!r}")

expect(
    *request("POST", "/children/revoke", {
        "expected_config_path": str(config),
        "child_name": "slate-manager",
    }),
    200,
    "Slate revoke",
)
body = expect(*request("POST", "/calls", slate_submission("revoked")), 200, "Slate revoked call")
if body.get("outcome") != "denied":
    fail(f"revoked Slate authority still called: {body!r}")

# Prove revoked authority and terminal Watch counts survive a real clean launchd restart.
lifecycle("stop")
lifecycle("start")
body = expect(*request("POST", "/calls", slate_submission("restart-revoked")), 200, "Slate restart denial")
if body.get("outcome") != "denied":
    fail(f"Slate revocation did not survive restart: {body!r}")
time.sleep(1)
final_summary = summary()
if final_summary.get("watch_event_deliveries") != 2:
    fail(f"Watch revocation did not survive restart: {final_summary!r}")
if final_summary.get("artifacts") != 3:
    fail(f"three fixture artifacts were not preserved: {final_summary!r}")
for copied, before_hash in fixture_copy_hashes.items():
    if hashlib.sha256(copied.read_bytes()).hexdigest() != before_hash:
        fail(f"operator-pointed acquisition mutated fixture source: {copied}")

print(json.dumps({
    "slate_artifact": slate_artifact,
    "watch_artifacts": watch_artifacts,
    "watch_event_deliveries": 2,
    "fixture_acquisitions": 3,
    "revocation_survived_restart": True,
}, sort_keys=True))
