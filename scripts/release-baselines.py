#!/usr/bin/env python3
"""Capture no-threshold performance evidence from one packaged resident."""

import argparse
import base64
import copy
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import re
import socket
import statistics
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=Path, required=True)
parser.add_argument("--harness", type=Path, required=True)
parser.add_argument("--digest-helper", type=Path, required=True)
parser.add_argument("--root", type=Path, required=True)
parser.add_argument("--fixture", type=Path, required=True)
args = parser.parse_args()
root = args.root.resolve()
uds = root / "control.sock"
state = root / "state.sqlite"
config = root / "config.json"
children = root / "children"
ledger = root / "observations.jsonl"


def request(method: str, path: str, value: object) -> tuple[int, object]:
    body = json.dumps(value, separators=(",", ":")).encode()
    wire = (
        f"{method} {path} HTTP/1.1\r\nHost: local\r\n"
        f"Content-Type: application/json\r\nContent-Length: {len(body)}\r\n\r\n"
    ).encode() + body
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(30)
    client.connect(str(uds))
    client.sendall(wire)
    client.shutdown(socket.SHUT_WR)
    chunks = []
    while True:
        chunk = client.recv(65536)
        if not chunk:
            break
        chunks.append(chunk)
    client.close()
    headers, response_body = b"".join(chunks).split(b"\r\n\r\n", 1)
    return int(headers.split()[1]), json.loads(response_body)


def expect(method: str, path: str, value: object, wanted: int = 200) -> object:
    status, body = request(method, path, value)
    if status != wanted:
        raise RuntimeError(f"{method} {path}: HTTP {status}: {body!r}")
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


def call_template() -> dict:
    payload = [{
        "watcher": "release-baseline",
        "stream-name": "patina:watch/events@0.1.0.emit",
        "change-kind": "created",
        "absolute-path": "baseline.txt",
        "relative-path": "baseline.txt",
        "size-bytes": 0,
        "modified-unix-ms": 1,
        "sha256": "sha256:" + "a" * 64,
        "detected-at": "2026-07-22T00:00:00Z",
    }]
    payload_bytes = json.dumps(payload, separators=(",", ":")).encode()
    return {
        "protocol_request_id": "proto-baseline-template",
        "call_id": "call-baseline-template",
        "target": {
            "namespace": "patina:watch",
            "interface_name": "events@0.1.0",
            "function_name": "emit",
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
        "trace_context": {"trace_id": "trace-baseline", "span_id": "span-baseline"},
        "payload": {
            "payload_kind": "inline_payload",
            "inline_payload_ref": "payload-baseline",
            "content_type": "application/json",
            "size_bytes": len(payload_bytes),
            "blake3_digest_hex": digest_bytes(payload_bytes),
        },
        "inline_payload_base64": base64.b64encode(payload_bytes).decode(),
    }


def unique_call(template: dict, suffix: str) -> dict:
    value = copy.deepcopy(template)
    value["protocol_request_id"] = f"proto-baseline-{suffix}"
    value["call_id"] = f"call-baseline-{suffix}"
    value["trace_context"] = {"trace_id": f"trace-baseline-{suffix}", "span_id": f"span-{suffix}"}
    value["payload"]["inline_payload_ref"] = f"payload-baseline-{suffix}"
    return value


def lifecycle(action: str) -> None:
    subprocess.run([args.harness, "release-smoke-internal", action, "--root", root], check=True,
                   stdout=subprocess.DEVNULL)


def percentile(values: list[float], percent: float) -> float:
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, int((len(ordered) - 1) * percent)))
    return ordered[index]


def resident_pid() -> int:
    output = subprocess.check_output([
        "/bin/launchctl", "print", f"gui/{subprocess.check_output(['/usr/bin/id', '-u'], text=True).strip()}/io.patina.mct.mother"
    ], text=True)
    match = re.search(r"^\s*pid = (\d+)\s*$", output, re.MULTILINE)
    if not match:
        raise RuntimeError("launchd status omitted resident PID")
    return int(match.group(1))


def rss_bytes(pid: int) -> int:
    return int(subprocess.check_output(["/bin/ps", "-o", "rss=", "-p", str(pid)], text=True).strip()) * 1024


def cpu_seconds(pid: int) -> float:
    raw = subprocess.check_output(["/bin/ps", "-o", "time=", "-p", str(pid)], text=True).strip()
    days = 0
    if "-" in raw:
        day, raw = raw.split("-", 1)
        days = int(day)
    fields = [float(value) for value in raw.split(":")]
    if len(fields) == 3:
        hours, minutes, seconds = fields
    else:
        hours, (minutes, seconds) = 0, fields
    return days * 86400 + hours * 3600 + minutes * 60 + seconds


# Five exact real-launchd start/ready/stop samples.
startup_ms = []
for _ in range(5):
    started = time.monotonic_ns()
    lifecycle("start")
    startup_ms.append((time.monotonic_ns() - started) / 1_000_000)
    lifecycle("stop")

# Idle resident: settle 60 seconds, then seven samples ten seconds apart.
lifecycle("start")
pid = resident_pid()
time.sleep(60)
idle_rss = []
for index in range(7):
    idle_rss.append(rss_bytes(pid))
    if index != 6:
        time.sleep(10)

# Acquire, approve, and grant only the exact null-sink fixture used by call measurements.
source = root / "baseline-null-sink-source"
source.mkdir()
for name in ["child.toml", "watch-null-sink.wasm"]:
    (source / name).write_bytes((args.fixture / name).read_bytes())
staged = expect("POST", "/artifacts/stage", {
    "source_root": str(source),
    "manifest_path": "child.toml",
    "component_path": "watch-null-sink.wasm",
    "claimed_child_name": "watch-null-sink",
    "claimed_artifact_version": "0.1.0",
    "expected_digest": None,
    "standing_source_authority_id": None,
    "claimed_publisher": None,
    "require_source_sidecars": False,
    "children_dir": str(children),
    "state_path": str(state),
})
artifact_id = staged["artifact_id"]
expect("POST", "/children/approve", {
    "expected_config_path": str(config),
    "expected_children_dir": str(children),
    "expected_state_path": str(state),
    "expected_artifact_id": artifact_id,
    "child_name": "watch-null-sink",
    "strict_integrity": True,
})
expect("POST", "/watch/supporting-grant", {
    "expected_config_path": str(config),
    "expected_children_dir": str(children),
    "expected_state_path": str(state),
    "child_name": "watch-null-sink",
    "expires_at": "2099-01-01T00:00:00Z",
    "grant": {"kind": "observability", "logging": True, "measure": True},
})

template = call_template()
for index in range(100):
    body = expect("POST", "/calls", unique_call(template, f"warmup-{index}"))
    if body.get("outcome") != "completed":
        raise RuntimeError(f"warmup failed: {body!r}")
latency_us = []
for index in range(1000):
    value = unique_call(template, f"latency-{index}")
    started = time.monotonic_ns()
    body = expect("POST", "/calls", value)
    latency_us.append((time.monotonic_ns() - started) / 1000)
    if body.get("outcome") != "completed":
        raise RuntimeError(f"latency call failed: {body!r}")

# Four concurrent same-UID clients, 500 calls each.
throughput_failures = []
throughput_peak_rss = rss_bytes(pid)
monitor_done = threading.Event()

def monitor() -> None:
    global throughput_peak_rss
    while not monitor_done.wait(0.02):
        try:
            throughput_peak_rss = max(throughput_peak_rss, rss_bytes(pid))
        except (subprocess.CalledProcessError, ValueError):
            return


def client(client_index: int) -> None:
    for call_index in range(500):
        suffix = f"throughput-{client_index}-{call_index}"
        try:
            status, body = request("POST", "/calls", unique_call(template, suffix))
            if status != 200 or body.get("outcome") != "completed":
                throughput_failures.append(f"{suffix}:{status}:{body!r}")
        except Exception as error:  # evidence records every failure rather than hiding a worker
            throughput_failures.append(f"{suffix}:{error}")

cpu_before = cpu_seconds(pid)
throughput_started = time.monotonic()
monitor_thread = threading.Thread(target=monitor)
workers = [threading.Thread(target=client, args=(index,)) for index in range(4)]
monitor_thread.start()
for worker in workers:
    worker.start()
for worker in workers:
    worker.join()
throughput_seconds = time.monotonic() - throughput_started
monitor_done.set()
monitor_thread.join()
cpu_after = cpu_seconds(pid)
if throughput_failures:
    raise RuntimeError(f"throughput failures: {throughput_failures[:5]!r}")

# One 4,097-occurrence fire-late recovery turn: 31 candidates plus one terminal range.
trigger_payload = json.dumps([{
    "watcher": "release-baseline-trigger",
    "stream-name": "patina:watch/events@0.1.0.emit",
    "change-kind": "created",
    "absolute-path": "trigger-baseline.txt",
    "relative-path": "trigger-baseline.txt",
    "size-bytes": 0,
    "modified-unix-ms": 1,
    "sha256": "sha256:" + "b" * 64,
    "detected-at": "2026-07-22T00:00:00Z",
}], separators=(",", ":")).encode()
blob = expect("POST", "/blobs", {
    "digest": digest_bytes(trigger_payload),
    "size_bytes": len(trigger_payload),
    "content_type": "application/json",
    "classification": "trigger-static",
    "bytes_base64": base64.b64encode(trigger_payload).decode(),
}, 201)
ledger_lines_before = len(ledger.read_text().splitlines())
ledger_bytes_before = ledger.stat().st_size
now = datetime.now(timezone.utc)
anchor = now - timedelta(minutes=4096, seconds=1)
iso = lambda value: value.isoformat(timespec="microseconds").replace("+00:00", "Z")
turn_cpu_before = cpu_seconds(pid)
turn_started = time.monotonic_ns()
expect("POST", "/triggers/create", {
    "expected_config_path": str(config),
    "expected_state_path": str(state),
    "scope": {
        "trigger_authority_id": "trigger:release-baseline-turn",
        "target": {"namespace": "patina:watch", "interface_name": "events@0.1.0", "function_name": "emit"},
        "payload_constraint": blob["payload"],
        "trigger_source": {"source_kind": "temporal", "anchor_at": iso(anchor), "interval_ms": 60000},
        "missed_fire_policy": "fire_late_bounded",
        "overlap_policy": "refuse",
        "starts_at": iso(anchor),
        "expires_at": "2099-01-01T00:00:00Z",
    },
})
deadline = time.monotonic() + 20
terminal_count = None
admitted = 0
while time.monotonic() < deadline:
    entries = [json.loads(line) for line in ledger.read_text().splitlines()[ledger_lines_before:]]
    admitted = sum(
        entry["observation"].get("safe_message")
        in {
            "trigger firing constructed durable call",
            "trigger occurrence admitted pending",
            "trigger occurrence terminal before call",
        }
        for entry in entries
    )
    represented = []
    for entry in entries:
        observation = entry["observation"]
        detail = observation.get("detail_ref") or ""
        if observation.get("safe_message") == "trigger occurrence capacity refused" and detail.startswith("call-trigger-occurrence-v1:"):
            record = json.loads(detail.split(":", 1)[1])
            represented.append(record["represented_set"]["count"])
    if admitted == 31 and 4066 in represented:
        terminal_count = 4066
        break
    time.sleep(0.05)
if terminal_count != 4066 or admitted != 31:
    raise RuntimeError(f"trigger turn incomplete: admitted={admitted} terminal={terminal_count}")
turn_ms = (time.monotonic_ns() - turn_started) / 1_000_000
turn_cpu_seconds = cpu_seconds(pid) - turn_cpu_before
status_started = time.monotonic_ns()
status, status_body = request("GET", "/status", {})
status_ms = (time.monotonic_ns() - status_started) / 1_000_000
if status != 200 or status_body.get("health") != "healthy" or status_body.get("readiness") != "ready":
    raise RuntimeError(f"ordinary status unavailable under trigger turn: {status} {status_body!r}")
ledger_growth = ledger.stat().st_size - ledger_bytes_before

lifecycle("stop")
lifecycle("uninstall")

result = {
    "startup_ms": startup_ms,
    "startup_min_ms": min(startup_ms),
    "startup_median_ms": statistics.median(startup_ms),
    "startup_max_ms": max(startup_ms),
    "idle_rss_bytes": idle_rss,
    "idle_rss_min_bytes": min(idle_rss),
    "idle_rss_median_bytes": int(statistics.median(idle_rss)),
    "idle_rss_max_bytes": max(idle_rss),
    "uds_latency_warmups": 100,
    "uds_latency_samples": 1000,
    "uds_latency_p50_us": percentile(latency_us, 0.50),
    "uds_latency_p95_us": percentile(latency_us, 0.95),
    "uds_latency_p99_us": percentile(latency_us, 0.99),
    "uds_latency_max_us": max(latency_us),
    "uds_latency_successes": len(latency_us),
    "throughput_clients": 4,
    "throughput_calls_per_client": 500,
    "throughput_seconds": throughput_seconds,
    "throughput_calls_per_second": 2000 / throughput_seconds,
    "throughput_failures": 0,
    "throughput_cpu_seconds": cpu_after - cpu_before,
    "throughput_peak_rss_bytes": throughput_peak_rss,
    "trigger_turn_ms": turn_ms,
    "trigger_turn_cpu_seconds": turn_cpu_seconds,
    "trigger_turn_admitted": admitted,
    "trigger_turn_terminal_refusals": terminal_count,
    "trigger_turn_status_ms": status_ms,
    "trigger_turn_ledger_bytes_after": ledger_growth,
}
print(json.dumps(result, indent=2, sort_keys=True))
