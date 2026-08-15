#!/usr/bin/env python3
"""Capture isolated, dev-launched MCT resident call-path evidence.

This harness never uses launchd or a production service root. Official mode has one
fixed matrix; --smoke exists only for fast, explicitly non-official harness checks.
Ledger attribution is added by the separate Phase 0 attribution task.
"""

from __future__ import annotations

import argparse
import base64
import copy
import ctypes
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import platform
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import traceback
from typing import Any, BinaryIO, TextIO

COVERED_REVISION = "ead8796d5143d0f9da623057dadc5c920c47bf2b"
CAVEAT = (
    "Dev-launched, harness-supervised, unsupervised-by-launchd release-Cargo-profile "
    "binary; not a release artifact and not directly comparable to "
    "BASELINES-v0.2.0-aarch64-apple-darwin.md."
)
SCHEMA = "mct-perf-phase-0-call-path/v1"
RAW_DIGEST_SCHEMA = "mct-perf-phase-0-raw-digests/v1"
MAX_HTTP_BYTES = 8 * 1024 * 1024
PRODUCTION_LABEL = "io.patina.mct.mother"


class HarnessError(RuntimeError):
    """Typed harness refusal or measurement failure."""


@dataclass(frozen=True)
class Matrix:
    startup_samples: int
    idle_settle_seconds: float
    idle_samples: int
    idle_interval_seconds: float
    sequential_warmups: int
    sequential_calls: int
    scaling_window: int
    throughput_clients: int
    throughput_calls_per_client: int


OFFICIAL_MATRIX = Matrix(5, 60.0, 7, 10.0, 100, 10_000, 500, 4, 500)
SMOKE_MATRIX = Matrix(1, 0.0, 1, 0.0, 2, 4, 2, 2, 2)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="microseconds").replace("+00:00", "Z")


def percentile(values: list[float], percent: float) -> float:
    """Nearest-rank percentile: sorted[ceil(p*n)-1]."""
    if not values:
        raise HarnessError("cannot summarize an empty sample set")
    if not 0 < percent <= 1:
        raise HarnessError(f"invalid percentile {percent}")
    ordered = sorted(values)
    return ordered[max(0, min(len(ordered) - 1, math.ceil(percent * len(ordered)) - 1))]


def summary(values: list[float], percentiles: tuple[float, ...] = ()) -> dict[str, Any]:
    result: dict[str, Any] = {
        "samples": values,
        "count": len(values),
        "min": min(values),
        "median": statistics.median(values),
        "max": max(values),
    }
    for value in percentiles:
        result[f"p{round(value * 100)}"] = percentile(values, value)
    return result


def command_text(argv: list[str | Path]) -> str:
    return " ".join(str(value) for value in argv)


class Log:
    def __init__(self, path: Path) -> None:
        self.path = path
        self._handle = path.open("a", encoding="utf-8", buffering=1)
        self._lock = threading.Lock()

    def write(self, message: str) -> None:
        line = f"{utc_now()} {message}"
        with self._lock:
            self._handle.write(line + "\n")
            self._handle.flush()
        print(line, file=sys.stderr, flush=True)

    def close(self) -> None:
        self._handle.close()


class DarwinProcessInfo:
    """Read resident CPU and RSS directly through libproc, without ps sampling."""

    class RusageInfoV2(ctypes.Structure):
        _fields_ = [("ri_uuid", ctypes.c_uint8 * 16)] + [
            (name, ctypes.c_uint64)
            for name in (
                "ri_user_time",
                "ri_system_time",
                "ri_pkg_idle_wkups",
                "ri_interrupt_wkups",
                "ri_pageins",
                "ri_wired_size",
                "ri_resident_size",
                "ri_phys_footprint",
                "ri_proc_start_abstime",
                "ri_proc_exit_abstime",
                "ri_child_user_time",
                "ri_child_system_time",
                "ri_child_pkg_idle_wkups",
                "ri_child_interrupt_wkups",
                "ri_child_pageins",
                "ri_child_elapsed_abstime",
                "ri_diskio_bytesread",
                "ri_diskio_byteswritten",
            )
        ]

    def __init__(self) -> None:
        self._libproc = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self._function = self._libproc.proc_pid_rusage
        self._function.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(self.RusageInfoV2),
        ]
        self._function.restype = ctypes.c_int

    def sample(self, pid: int) -> dict[str, float | int]:
        value = self.RusageInfoV2()
        if self._function(pid, 2, ctypes.byref(value)) != 0:
            error = ctypes.get_errno()
            raise HarnessError(f"proc_pid_rusage({pid}) failed: errno={error}")
        return {
            "cpu_seconds": (value.ri_user_time + value.ri_system_time) / 1_000_000_000,
            "user_cpu_seconds": value.ri_user_time / 1_000_000_000,
            "system_cpu_seconds": value.ri_system_time / 1_000_000_000,
            "resident_bytes": int(value.ri_resident_size),
            "physical_footprint_bytes": int(value.ri_phys_footprint),
        }


@dataclass
class ScenarioPaths:
    name: str
    root: Path
    identity: Path
    config: Path
    children: Path
    state: Path
    ledger: Path
    socket: Path
    source: Path
    stdout: Path
    stderr: Path
    identity_log: Path


class Resident:
    def __init__(
        self,
        binary: Path,
        paths: ScenarioPaths,
        log: Log,
        process_info: DarwinProcessInfo,
    ) -> None:
        self.binary = binary
        self.paths = paths
        self.log = log
        self.process_info = process_info
        self.process: subprocess.Popen[bytes] | None = None
        self._stdout: BinaryIO | None = None
        self._stderr: BinaryIO | None = None

    def initialize_identity(self) -> None:
        if os.path.lexists(self.paths.socket):
            raise HarnessError(f"refusing existing socket before identity setup: {self.paths.socket}")
        argv = [
            self.binary,
            "iroh",
            "identity",
            self.paths.identity,
            "--config",
            self.paths.config,
            "--ledger",
            self.paths.ledger,
            "--uds",
            self.paths.socket,
        ]
        self.log.write(f"identity {self.paths.name}: {command_text(argv)}")
        with self.paths.identity_log.open("wb") as output:
            subprocess.run(argv, check=True, stdout=output, stderr=subprocess.STDOUT)
        if os.path.lexists(self.paths.socket):
            raise HarnessError(f"offline identity unexpectedly created socket: {self.paths.socket}")

    def start(self) -> int:
        if self.process is not None:
            raise HarnessError(f"resident already started: {self.paths.name}")
        if os.path.lexists(self.paths.socket):
            raise HarnessError(f"refusing existing resident socket: {self.paths.socket}")
        argv = [
            self.binary,
            "serve",
            "--identity",
            self.paths.identity,
            "--config",
            self.paths.config,
            "--children-dir",
            self.paths.children,
            "--state",
            self.paths.state,
            "--ledger",
            self.paths.ledger,
            "--max-connections",
            "64",
            "--uds",
            self.paths.socket,
        ]
        self.log.write(f"launch direct resident {self.paths.name}: {command_text(argv)}")
        self._stdout = self.paths.stdout.open("wb")
        self._stderr = self.paths.stderr.open("wb")
        self.process = subprocess.Popen(argv, stdout=self._stdout, stderr=self._stderr)
        return self.process.pid

    def await_ready(self, timeout: float = 30.0) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        last_error = "socket not bound"
        while time.monotonic() < deadline:
            if self.process is None:
                raise HarnessError("resident was not started")
            status = self.process.poll()
            if status is not None:
                raise HarnessError(
                    f"resident {self.paths.name} exited before readiness with status {status}"
                )
            if os.path.lexists(self.paths.socket):
                try:
                    code, body = uds_request(self.paths.socket, "GET", "/status", {})
                    if (
                        code == 200
                        and isinstance(body, dict)
                        and body.get("health") == "healthy"
                        and body.get("readiness") == "ready"
                        and body.get("safe_message") == "ready"
                    ):
                        return body
                    last_error = f"HTTP {code}: {body!r}"
                except (OSError, HarnessError) as error:
                    last_error = str(error)
            time.sleep(0.01)
        raise HarnessError(f"resident {self.paths.name} readiness timed out: {last_error}")

    def stop(self) -> None:
        process = self.process
        if process is None:
            return
        clean = False
        try:
            if process.poll() is None:
                self.log.write(f"SIGTERM direct resident {self.paths.name} pid={process.pid}")
                process.send_signal(signal.SIGTERM)
            try:
                status = process.wait(timeout=30)
            except subprocess.TimeoutExpired as error:
                process.kill()
                process.wait(timeout=10)
                raise HarnessError(f"resident {self.paths.name} did not stop cleanly") from error
            if status != 0:
                raise HarnessError(f"resident {self.paths.name} exited with status {status}")
            clean = True
        finally:
            self.process = None
            if self._stdout is not None:
                self._stdout.close()
                self._stdout = None
            if self._stderr is not None:
                self._stderr.close()
                self._stderr = None
        if clean:
            self.log.write(f"clean direct resident shutdown {self.paths.name}")

    def force_stop_after_failure(self) -> None:
        process = self.process
        if process is None:
            return
        try:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        finally:
            self.process = None
            if self._stdout is not None:
                self._stdout.close()
                self._stdout = None
            if self._stderr is not None:
                self._stderr.close()
                self._stderr = None


def uds_request(socket_path: Path, method: str, path: str, value: object) -> tuple[int, Any]:
    body = json.dumps(value, separators=(",", ":")).encode()
    wire = (
        f"{method} {path} HTTP/1.1\r\nHost: local\r\n"
        f"Content-Type: application/json\r\nContent-Length: {len(body)}\r\n\r\n"
    ).encode() + body
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(60)
        client.connect(str(socket_path))
        client.sendall(wire)
        client.shutdown(socket.SHUT_WR)
        chunks: list[bytes] = []
        total = 0
        while True:
            chunk = client.recv(65536)
            if not chunk:
                break
            total += len(chunk)
            if total > MAX_HTTP_BYTES:
                raise HarnessError(f"UDS response exceeded {MAX_HTTP_BYTES} bytes for {path}")
            chunks.append(chunk)
    response = b"".join(chunks)
    if b"\r\n\r\n" not in response:
        raise HarnessError(f"malformed UDS HTTP response for {path}")
    headers, response_body = response.split(b"\r\n\r\n", 1)
    fields = headers.split()
    if len(fields) < 2:
        raise HarnessError(f"malformed UDS HTTP status for {path}")
    try:
        status = int(fields[1])
        decoded = json.loads(response_body)
    except (ValueError, json.JSONDecodeError) as error:
        raise HarnessError(f"invalid UDS response for {path}: {error}") from error
    return status, decoded


def expect(socket_path: Path, method: str, path: str, value: object, wanted: int = 200) -> Any:
    status, body = uds_request(socket_path, method, path, value)
    if status != wanted:
        raise HarnessError(f"{method} {path}: HTTP {status}: {body!r}")
    return body


def resolved_nonexistent(path: Path) -> Path:
    return path.parent.resolve(strict=True) / path.name


def production_roots() -> list[Path]:
    home = Path.home()
    return [
        (home / ".mct").resolve(strict=False),
        (home / "Library" / "LaunchAgents").resolve(strict=False),
        Path("/Library/LaunchAgents"),
        Path("/Library/LaunchDaemons"),
    ]


def assert_safe_path(path: Path, purpose: str) -> None:
    resolved = path.resolve(strict=False)
    text = str(resolved)
    if PRODUCTION_LABEL in text:
        raise HarnessError(f"refusing production service label in {purpose} path: {resolved}")
    for forbidden in production_roots():
        if resolved == forbidden or forbidden in resolved.parents:
            raise HarnessError(f"refusing production {purpose} path: {resolved}")


def run_capture(argv: list[str | Path], log_path: Path, cwd: Path) -> None:
    with log_path.open("wb") as output:
        subprocess.run([str(value) for value in argv], cwd=cwd, check=True, stdout=output, stderr=subprocess.STDOUT)


def command_output(argv: list[str | Path], cwd: Path | None = None) -> str:
    return subprocess.check_output([str(value) for value in argv], cwd=cwd, text=True).strip()


def check_host(official: bool, load_note: str) -> dict[str, Any]:
    if sys.version_info < (3, 11):
        raise HarnessError(f"Python 3.11+ required, found {platform.python_version()}")
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise HarnessError(
            f"refusing non-aarch64-apple-darwin host: {platform.system()} {platform.machine()}"
        )
    rustc_verbose = command_output(["rustc", "-vV"])
    if "host: aarch64-apple-darwin" not in rustc_verbose.splitlines():
        raise HarnessError("rustc host is not aarch64-apple-darwin")
    battery = command_output(["/usr/bin/pmset", "-g", "batt"])
    if official and "Now drawing from 'AC Power'" not in battery:
        raise HarnessError(f"official measurements require AC power: {battery}")
    if official and not load_note.strip():
        raise HarnessError("official measurements require nonblank --load-note")
    return {"rustc_verbose": rustc_verbose, "power_battery": battery}


def git_preflight(repo: Path, official: bool) -> dict[str, Any]:
    head = command_output(["git", "rev-parse", "HEAD"], repo)
    branch = command_output(["git", "branch", "--show-current"], repo)
    status = command_output(["git", "status", "--porcelain", "--untracked-files=all"], repo)
    if official and status:
        raise HarnessError(f"official measurement requires a clean tree:\n{status}")
    production_diff = command_output(
        [
            "git",
            "diff",
            "--name-only",
            f"{COVERED_REVISION}..HEAD",
            "--",
            "crates/mct-daemon/src",
            "crates/mct-kernel/src",
            "crates/mct-observation/src",
            "crates/mct-iroh/src",
        ],
        repo,
    )
    if production_diff:
        raise HarnessError(f"production source differs from covered revision:\n{production_diff}")
    crate_diff = command_output(
        ["git", "diff", "--name-only", f"{COVERED_REVISION}..HEAD", "--", "crates"], repo
    ).splitlines()
    allowed = {
        "crates/mct-daemon/Cargo.toml",
        "crates/mct-daemon/benches/perf_phase_0.rs",
    }
    unexpected = sorted(set(filter(None, crate_diff)) - allowed)
    if unexpected:
        raise HarnessError(f"unexpected crates diff after covered revision: {unexpected}")
    return {
        "covered_revision": COVERED_REVISION,
        "harness_revision": head,
        "branch": branch,
        "working_tree_porcelain": status,
        "production_source_diff": [],
        "ratified_crate_diff": crate_diff,
    }


def create_paths(run_root: Path, name: str, raw_root: Path) -> ScenarioPaths:
    root = run_root / name
    root.mkdir(mode=0o700)
    children = root / "children"
    source = root / "fixture-source"
    children.mkdir(mode=0o700)
    source.mkdir(mode=0o700)
    raw = raw_root / name
    raw.mkdir(parents=True, mode=0o700)
    paths = ScenarioPaths(
        name=name,
        root=root,
        identity=root / "identity.hex",
        config=root / "config.json",
        children=children,
        state=root / "state.sqlite",
        ledger=root / "observations.jsonl",
        socket=root / "c.sock",
        source=source,
        stdout=raw / "resident.stdout.log",
        stderr=raw / "resident.stderr.log",
        identity_log=raw / "identity.log",
    )
    for path in vars(paths).values():
        if isinstance(path, Path):
            assert_safe_path(path, name)
    if len(os.fsencode(paths.socket)) >= 100:
        raise HarnessError(f"generated UDS path is not safely below Darwin limit: {paths.socket}")
    return paths


def digest_files(helper: Path, paths: list[Path]) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for path in paths:
        output = command_output([helper, path])
        values = json.loads(output)
        if len(values) != 1:
            raise HarnessError(f"digest helper returned unexpected output for {path}")
        records.append(values[0])
    return records


def digest_bytes(helper: Path, root: Path, value: bytes) -> str:
    handle = tempfile.NamedTemporaryFile(dir=root, delete=False)
    path = Path(handle.name)
    try:
        with handle:
            handle.write(value)
        record = digest_files(helper, [path])[0]
        return str(record["blake3"])
    finally:
        path.unlink(missing_ok=True)


def stage_fixture(paths: ScenarioPaths, fixture: Path, mutation_suffix: str) -> dict[str, Any]:
    for name in ("child.toml", "watch-null-sink.wasm"):
        shutil.copyfile(fixture / name, paths.source / name)
    staged = expect(
        paths.socket,
        "POST",
        "/artifacts/stage",
        {
            "source_root": str(paths.source),
            "manifest_path": "child.toml",
            "component_path": "watch-null-sink.wasm",
            "claimed_child_name": "watch-null-sink",
            "claimed_artifact_version": "0.1.0",
            "expected_digest": None,
            "standing_source_authority_id": None,
            "claimed_publisher": None,
            "require_source_sidecars": False,
            "children_dir": str(paths.children),
            "state_path": str(paths.state),
        },
    )
    artifact_id = staged.get("artifact_id")
    if not isinstance(artifact_id, str) or not artifact_id.startswith("sha256:"):
        raise HarnessError(f"stage omitted exact artifact identity: {staged!r}")
    approved = expect(
        paths.socket,
        "POST",
        "/children/approve",
        {
            "expected_config_path": str(paths.config),
            "expected_children_dir": str(paths.children),
            "expected_state_path": str(paths.state),
            "expected_artifact_id": artifact_id,
            "child_name": "watch-null-sink",
            "strict_integrity": True,
        },
    )
    mutation_id = f"perf-phase-0-observability-{mutation_suffix}"
    granted = expect(
        paths.socket,
        "POST",
        "/watch/supporting-grant",
        {
            "mutation_id": mutation_id,
            "expected_config_path": str(paths.config),
            "expected_children_dir": str(paths.children),
            "expected_state_path": str(paths.state),
            "child_name": "watch-null-sink",
            "expires_at": "2099-01-01T00:00:00Z",
            "grant": {"kind": "observability", "logging": True, "measure": True},
        },
    )
    authority = granted.get("mutation_result", {}).get("grants_authority")
    required = {
        "mother_node_id",
        "authority_epoch",
        "generation",
        "source_authority_observation_id",
    }
    if not isinstance(authority, dict) or set(authority) != required:
        raise HarnessError(f"grant response omitted complete receiver authority: {granted!r}")
    status = expect(paths.socket, "GET", "/status", {})
    resident = status.get("resident", {})
    if (
        status.get("health") != "healthy"
        or status.get("readiness") != "ready"
        or resident.get("node_id") != authority["mother_node_id"]
        or resident.get("loaded_child_count", 0) < 1
        or resident.get("approved_child_count", 0) < 1
    ):
        raise HarnessError(f"post-grant owner status is not ready/consistent: {status!r}")
    return {
        "artifact_id": artifact_id,
        "observed_digest": staged.get("observed_digest"),
        "observed_size_bytes": staged.get("observed_size_bytes"),
        "approval": approved,
        "receiver_authority": authority,
        "owner_status": status,
    }


def call_template(helper: Path, root: Path, receiver_authority: dict[str, Any]) -> dict[str, Any]:
    payload = [
        {
            "watcher": "release-baseline",
            "stream-name": "patina:watch/events@0.1.0.emit",
            "change-kind": "created",
            "absolute-path": "baseline.txt",
            "relative-path": "baseline.txt",
            "size-bytes": 0,
            "modified-unix-ms": 1,
            "sha256": "sha256:" + "a" * 64,
            "detected-at": "2026-07-22T00:00:00Z",
        }
    ]
    payload_bytes = json.dumps(payload, separators=(",", ":")).encode()
    return {
        "protocol_request_id": "proto-perf-template",
        "call_id": "call-perf-template",
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
            "expected_receiver_grants_authority": receiver_authority,
            "vision_policy_revision": 1,
        },
        "deadline": "2099-01-01T00:00:00Z",
        "trace_context": {"trace_id": "trace-perf-template", "span_id": "span-perf-template"},
        "payload": {
            "payload_kind": "inline_payload",
            "inline_payload_ref": "payload-perf-template",
            "content_type": "application/json",
            "size_bytes": len(payload_bytes),
            "blake3_digest_hex": digest_bytes(helper, root, payload_bytes),
        },
        "inline_payload_base64": base64.b64encode(payload_bytes).decode(),
        "idempotency_key": "idempotency-perf-template",
    }


def unique_call(template: dict[str, Any], suffix: str) -> dict[str, Any]:
    value = copy.deepcopy(template)
    value["protocol_request_id"] = f"proto-perf-{suffix}"
    value["call_id"] = f"call-perf-{suffix}"
    value["trace_context"] = {"trace_id": f"trace-perf-{suffix}", "span_id": f"span-{suffix}"}
    value["payload"]["inline_payload_ref"] = f"payload-perf-{suffix}"
    value["idempotency_key"] = f"idempotency-perf-{suffix}"
    return value


def measured_call(
    socket_path: Path,
    template: dict[str, Any],
    suffix: str,
    scenario: str,
    ordinal: int,
    client_index: int | None,
    require_completed: bool = True,
) -> tuple[float, dict[str, Any], dict[str, Any]]:
    value = unique_call(template, suffix)
    started_at = utc_now()
    started = time.monotonic_ns()
    status, body = uds_request(socket_path, "POST", "/calls", value)
    completed = time.monotonic_ns()
    duration_us = (completed - started) / 1_000
    row = {
        "schema": "mct-perf-phase-0-client-call/v1",
        "scenario": scenario,
        "ordinal": ordinal,
        "client_index": client_index,
        "protocol_request_id": value["protocol_request_id"],
        "call_id": value["call_id"],
        "started_at": started_at,
        "monotonic_started_ns": started,
        "monotonic_completed_ns": completed,
        "duration_us": duration_us,
        "http_status": status,
        "outcome": body.get("outcome") if isinstance(body, dict) else None,
        "protocol_reason": body.get("protocol_reason") if isinstance(body, dict) else None,
        "safe_message": body.get("safe_message") if isinstance(body, dict) else None,
    }
    completed_ok = status == 200 and isinstance(body, dict) and body.get("outcome") == "completed"
    if require_completed and not completed_ok:
        raise HarnessError(f"measured call failed: {row!r}; body={body!r}")
    return duration_us, row, body


def write_json_line(handle: TextIO, value: dict[str, Any], lock: threading.Lock) -> None:
    line = json.dumps(value, sort_keys=True, separators=(",", ":"))
    with lock:
        handle.write(line + "\n")
        handle.flush()


def incremental_ledger_count(path: Path, prior_size: int, prior_count: int) -> tuple[int, int]:
    size = path.stat().st_size
    if size < prior_size:
        raise HarnessError(f"ledger shrank during measurement: {path}")
    with path.open("rb") as handle:
        handle.seek(prior_size)
        count = prior_count
        while chunk := handle.read(1024 * 1024):
            count += chunk.count(b"\n")
    return size, count


def copy_scenario_evidence(paths: ScenarioPaths, raw_root: Path, combined: BinaryIO | None) -> None:
    target = raw_root / paths.name / "observations.jsonl"
    shutil.copyfile(paths.ledger, target)
    if combined is not None:
        with target.open("rb") as source:
            shutil.copyfileobj(source, combined)
        combined.flush()


def process_snapshot() -> str:
    result = command_output(
        ["/bin/ps", "-Ao", "pid,ppid,%cpu,%mem,rss,etime,command", "-r"]
    ).splitlines()
    return "\n".join(result[:51])


def host_record(
    repo: Path,
    git: dict[str, Any],
    initial: dict[str, Any],
    official: bool,
    load_note: str,
    raw_digests: list[dict[str, Any]],
    success: bool,
    failure: dict[str, Any] | None,
) -> dict[str, Any]:
    profiler = json.loads(command_output(["/usr/sbin/system_profiler", "SPHardwareDataType", "-json"]))
    return {
        "schema": "mct-perf-phase-0-host/v1",
        "caveat": CAVEAT,
        "official": official,
        "success": success,
        "captured_at": utc_now(),
        "source": git,
        "python_version": platform.python_version(),
        "cargo_version": command_output(["cargo", "--version"]),
        "rustc_version": command_output(["rustc", "--version"]),
        "rustc_verbose": initial["rustc_verbose"],
        "hardware": {
            "model": command_output(["/usr/sbin/sysctl", "-n", "hw.model"]),
            "chip_or_cpu": command_output(["/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string"]),
            "logical_cpus": int(command_output(["/usr/sbin/sysctl", "-n", "hw.logicalcpu"])),
            "memory_bytes": int(command_output(["/usr/sbin/sysctl", "-n", "hw.memsize"])),
            "system_profiler": profiler,
        },
        "os": {
            "sw_vers": command_output(["/usr/bin/sw_vers"]),
            "uname": command_output(["/usr/bin/uname", "-a"]),
            "platform": platform.platform(),
            "machine": platform.machine(),
        },
        "power": {
            "battery": initial["power_battery"],
            "custom": command_output(["/usr/bin/pmset", "-g", "custom"]),
        },
        "load": {
            "operator_note": load_note,
            "load_averages": list(os.getloadavg()),
            "process_snapshot": process_snapshot(),
        },
        "raw_evidence": {
            "schema": RAW_DIGEST_SCHEMA,
            "digest_algorithm": "blake3",
            "files": raw_digests,
        },
        "failure": failure,
        "repository": str(repo),
    }


def render_markdown(result: dict[str, Any], host: dict[str, Any]) -> str:
    lines = [
        "# MCT Performance Phase 0 — call path",
        "",
        f"> **Non-comparability notice:** {CAVEAT}",
        "",
        f"- Covered revision: `{result['covered_revision']}`",
        f"- Harness revision: `{result['harness_revision']}`",
        f"- Official matrix: `{str(result['official']).lower()}`",
        f"- Percentiles: `{result['percentile_method']}`",
        f"- Host: `{host['hardware']['model']}` / `{host['hardware']['chip_or_cpu']}`",
        f"- Power: `{host['power']['battery'].splitlines()[0]}`",
        f"- Load note: {host['load']['operator_note']}",
        "",
        "## Startup to ready",
        "",
        "| Samples | Min ms | Median ms | Max ms |",
        "|---:|---:|---:|---:|",
    ]
    startup = result["startup"]
    lines.append(
        f"| {startup['count']} | {startup['min_ms']:.3f} | {startup['median_ms']:.3f} | {startup['max_ms']:.3f} |"
    )
    lines.extend(["", "## Idle RSS after settle", "", "| Samples | Min bytes | Median bytes | Max bytes |", "|---:|---:|---:|---:|"])
    idle = result["idle_rss"]
    lines.append(
        f"| {idle['count']} | {idle['min_bytes']} | {idle['median_bytes']} | {idle['max_bytes']} |"
    )
    lines.extend(["", "## Sequential headline (calls 1–1000)", "", "| Calls | p50 µs | p95 µs | p99 µs | Max µs |", "|---:|---:|---:|---:|---:|"])
    headline = result["sequential"]["headline_calls_1_to_1000"]
    lines.append(
        f"| {headline['count']} | {headline['p50_us']:.3f} | {headline['p95_us']:.3f} | {headline['p99_us']:.3f} | {headline['max_us']:.3f} |"
    )
    lines.extend(["", "## Scaling windows", "", "| Calls | p50 µs | p95 µs | Ledger entries | Ledger bytes |", "|---|---:|---:|---:|---:|"])
    for window in result["sequential"]["scaling_windows"]:
        lines.append(
            f"| {window['start_call']}–{window['end_call']} | {window['p50_us']:.3f} | {window['p95_us']:.3f} | {window['ledger_entries']} | {window['ledger_bytes']} |"
        )
    lines.extend(["", "## Concurrent throughput", "", "| Clients × calls | Elapsed s | Calls/s | CPU s | Peak RSS bytes | Failures |", "|---|---:|---:|---:|---:|---:|"])
    throughput = result["throughput"]
    lines.append(
        f"| {throughput['clients']} × {throughput['calls_per_client']} | {throughput['elapsed_seconds']:.6f} | {throughput['calls_per_second']:.3f} | {throughput['cpu_seconds']:.6f} | {throughput['peak_rss_bytes']} | {len(throughput['failures'])} |"
    )
    lines.extend(
        [
            "",
            "## Attribution status",
            "",
            "Ledger-derived stage attribution is intentionally pending the separate Phase 0 attribution task.",
            "",
            "## Raw evidence",
            "",
            f"Raw observations, client intervals, and logs remain outside git. `host.json` records {len(host['raw_evidence']['files'])} raw-file byte sizes and BLAKE3 digests.",
            "",
        ]
    )
    return "\n".join(lines)


class Harness:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.repo = Path(__file__).resolve().parents[2]
        self.output = resolved_nonexistent(args.output)
        self.run_root: Path | None = None
        self.raw_root: Path | None = None
        self.log: Log | None = None
        self.process_info: DarwinProcessInfo | None = None
        self.active: Resident | None = None
        self.failures: list[dict[str, Any]] = []
        self.result: dict[str, Any] = {}
        self.client_handle: TextIO | None = None
        self.combined_observations: BinaryIO | None = None
        self.client_lock = threading.Lock()

    def run_command(self) -> None:
        assert_safe_path(self.output, "output")
        if self.output.exists() or os.path.lexists(self.output):
            raise HarnessError(f"refusing to overwrite output path: {self.output}")
        if self.repo == self.output or self.repo in self.output.parents:
            raise HarnessError("output must be outside the repository so raw evidence cannot enter git")
        initial = check_host(self.args.official, self.args.load_note)
        self.process_info = DarwinProcessInfo()
        git = git_preflight(self.repo, self.args.official)
        self.output.mkdir(mode=0o700)
        self.raw_root = self.output / "raw"
        self.raw_root.mkdir(mode=0o700)
        self.log = Log(self.output / "harness.log")
        self.log.write(f"start mode={'official' if self.args.official else 'smoke'} output={self.output}")
        self.run_root = Path(tempfile.mkdtemp(prefix="mctp0."))
        self.run_root.chmod(0o700)
        assert_safe_path(self.run_root, "temporary run root")
        self.log.write(f"fresh temporary run root={self.run_root}")
        binary = self.repo / "target" / "release" / "mct-daemon"
        helper = self.repo / "target" / "release" / "examples" / "release-digests"
        fixture = self.repo / "crates" / "mct-daemon" / "tests" / "fixtures" / "watch-null-sink-0.1.0"
        for path in (binary, helper, fixture):
            assert_safe_path(path, "input")
        matrix = OFFICIAL_MATRIX if self.args.official else SMOKE_MATRIX
        failure: dict[str, Any] | None = None
        success = False
        try:
            self.prepare_tools(binary, helper)
            self.client_handle = (self.output / "client-calls.jsonl").open(
                "w", encoding="utf-8", buffering=1
            )
            self.combined_observations = (self.output / "observations.jsonl").open("wb")
            startup = self.measure_startup(binary, fixture, matrix)
            idle = self.measure_idle(binary, fixture, matrix)
            sequential = self.measure_sequential(binary, helper, fixture, matrix)
            throughput = self.measure_throughput(binary, helper, fixture, matrix)
            self.client_handle.close()
            self.client_handle = None
            self.combined_observations.close()
            self.combined_observations = None
            self.result = {
                "schema": SCHEMA,
                "caveat": CAVEAT,
                "official": self.args.official,
                "success": True,
                "covered_revision": COVERED_REVISION,
                "harness_revision": git["harness_revision"],
                "captured_at": utc_now(),
                "percentile_method": "nearest_rank_sorted_ceil_p_times_n_minus_1",
                "units": {"latency": "microseconds", "rss": "bytes", "cpu": "seconds"},
                "matrix": vars(matrix),
                "startup": startup,
                "idle_rss": idle,
                "sequential": sequential,
                "throughput": throughput,
                "failures": self.failures,
                "attribution": {"status": "pending_separate_task"},
            }
            (self.output / "call-path.json").write_text(
                json.dumps(self.result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
            )
            success = True
        except BaseException as error:
            if self.active is not None:
                self.active.force_stop_after_failure()
                self.active = None
            failure = {
                "schema": "mct-perf-phase-0-failure/v1",
                "failed_at": utc_now(),
                "error_type": type(error).__name__,
                "error": str(error),
                "traceback": traceback.format_exc(),
                "temporary_run_root": str(self.run_root),
            }
            (self.output / "failure.json").write_text(
                json.dumps(failure, indent=2, sort_keys=True) + "\n", encoding="utf-8"
            )
            self.log.write(f"failure: {type(error).__name__}: {error}")
        finally:
            if self.client_handle is not None:
                self.client_handle.close()
                self.client_handle = None
            if self.combined_observations is not None:
                self.combined_observations.close()
                self.combined_observations = None
            self.log.write(f"finish success={success}")
            raw_paths = sorted(
                path
                for path in self.output.rglob("*")
                if path.is_file()
                and (
                    path.name.endswith(".log")
                    or path.name in {"observations.jsonl", "client-calls.jsonl", "failure.json"}
                )
            )
            raw_records: list[dict[str, Any]] = []
            if helper.exists():
                for record in digest_files(helper, raw_paths):
                    absolute = Path(record.pop("path"))
                    record["relative_path"] = str(absolute.relative_to(self.output))
                    record["size_bytes"] = record.pop("size")
                    raw_records.append(record)
            host = host_record(
                self.repo,
                git,
                initial,
                self.args.official,
                self.args.load_note,
                raw_records,
                success,
                failure,
            )
            (self.output / "host.json").write_text(
                json.dumps(host, indent=2, sort_keys=True) + "\n", encoding="utf-8"
            )
            if success:
                (self.output / "call-path.md").write_text(
                    render_markdown(self.result, host), encoding="utf-8"
                )
                (self.output / "SUCCESS").write_text(utc_now() + "\n", encoding="utf-8")
                if self.run_root is not None:
                    shutil.rmtree(self.run_root)
                    self.run_root = None
            else:
                self.log.write(f"failed evidence preserved at {self.output}")
            self.log.close()
        if not success:
            raise HarnessError(f"measurement failed; inspect {self.output / 'failure.json'}")

    def prepare_tools(self, binary: Path, helper: Path) -> None:
        assert self.raw_root is not None and self.log is not None
        if not self.args.skip_fixture_rebuild:
            argv: list[str | Path] = [self.repo / "scripts" / "build-watch-fixtures.sh"]
            if self.args.watch_upstream is not None:
                argv.append(self.args.watch_upstream)
            self.log.write(f"verify fixtures: {command_text(argv)}")
            run_capture(argv, self.raw_root / "fixture-verification.log", self.repo)
        else:
            self.log.write("non-official smoke: fixture rebuild skipped")
        if not self.args.skip_build:
            argv = [
                "cargo",
                "build",
                "--release",
                "--locked",
                "-p",
                "mct-daemon",
                "--bin",
                "mct-daemon",
                "--example",
                "release-digests",
            ]
            self.log.write(f"build release profile: {command_text(argv)}")
            run_capture(argv, self.raw_root / "cargo-build.log", self.repo)
        else:
            self.log.write("non-official smoke: release build skipped")
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise HarnessError(f"release resident missing or not executable: {binary}")
        if not helper.is_file() or not os.access(helper, os.X_OK):
            raise HarnessError(f"release digest helper missing or not executable: {helper}")

    def new_resident(self, binary: Path, name: str) -> Resident:
        assert self.run_root is not None and self.raw_root is not None and self.log is not None
        assert self.process_info is not None
        paths = create_paths(self.run_root, name, self.raw_root)
        resident = Resident(binary, paths, self.log, self.process_info)
        resident.initialize_identity()
        return resident

    def setup_resident(self, resident: Resident, fixture: Path, suffix: str) -> dict[str, Any]:
        setup = stage_fixture(resident.paths, fixture, suffix)
        assert self.log is not None
        self.log.write(
            f"ready fixture {resident.paths.name}: artifact={setup['artifact_id']} "
            f"authority={setup['receiver_authority']}"
        )
        return setup

    def measure_startup(self, binary: Path, fixture: Path, matrix: Matrix) -> dict[str, Any]:
        assert self.raw_root is not None
        values: list[float] = []
        statuses: list[dict[str, Any]] = []
        for index in range(matrix.startup_samples):
            resident = self.new_resident(binary, f"startup-{index}")
            self.active = resident
            started = time.monotonic_ns()
            resident.start()
            status = resident.await_ready()
            values.append((time.monotonic_ns() - started) / 1_000_000)
            statuses.append(status)
            self.setup_resident(resident, fixture, f"startup-{index}")
            resident.stop()
            copy_scenario_evidence(resident.paths, self.raw_root, None)
            self.active = None
        values_summary = summary(values)
        return {
            "samples_ms": values,
            "count": len(values),
            "min_ms": values_summary["min"],
            "median_ms": values_summary["median"],
            "max_ms": values_summary["max"],
            "ready_statuses": statuses,
        }

    def measure_idle(self, binary: Path, fixture: Path, matrix: Matrix) -> dict[str, Any]:
        assert self.raw_root is not None
        resident = self.new_resident(binary, "idle")
        self.active = resident
        pid = resident.start()
        resident.await_ready()
        self.setup_resident(resident, fixture, "idle")
        time.sleep(matrix.idle_settle_seconds)
        samples: list[int] = []
        footprints: list[int] = []
        for index in range(matrix.idle_samples):
            value = self.process_info.sample(pid)
            samples.append(int(value["resident_bytes"]))
            footprints.append(int(value["physical_footprint_bytes"]))
            if index + 1 != matrix.idle_samples:
                time.sleep(matrix.idle_interval_seconds)
        resident.stop()
        copy_scenario_evidence(resident.paths, self.raw_root, None)
        self.active = None
        values_summary = summary([float(value) for value in samples])
        return {
            "settle_seconds": matrix.idle_settle_seconds,
            "sample_interval_seconds": matrix.idle_interval_seconds,
            "samples_bytes": samples,
            "physical_footprint_samples_bytes": footprints,
            "count": len(samples),
            "min_bytes": int(values_summary["min"]),
            "median_bytes": int(values_summary["median"]),
            "max_bytes": int(values_summary["max"]),
        }

    def measure_sequential(
        self, binary: Path, helper: Path, fixture: Path, matrix: Matrix
    ) -> dict[str, Any]:
        assert self.raw_root is not None and self.client_handle is not None
        assert self.combined_observations is not None
        resident = self.new_resident(binary, "sequential")
        self.active = resident
        resident.start()
        resident.await_ready()
        setup = self.setup_resident(resident, fixture, "sequential")
        template = call_template(helper, resident.paths.root, setup["receiver_authority"])
        for index in range(matrix.sequential_warmups):
            suffix = f"sequential-warmup-{index}"
            _, _, _ = measured_call(
                resident.paths.socket, template, suffix, "warmup", index + 1, None
            )
        ledger_size, ledger_count = incremental_ledger_count(resident.paths.ledger, 0, 0)
        latencies: list[float] = []
        windows: list[dict[str, Any]] = []
        window_values: list[float] = []
        for index in range(matrix.sequential_calls):
            suffix = f"sequential-{index + 1}"
            latency, row, _ = measured_call(
                resident.paths.socket, template, suffix, "sequential", index + 1, None
            )
            write_json_line(self.client_handle, row, self.client_lock)
            latencies.append(latency)
            window_values.append(latency)
            if (index + 1) % matrix.scaling_window == 0:
                ledger_size, ledger_count = incremental_ledger_count(
                    resident.paths.ledger, ledger_size, ledger_count
                )
                start_call = index + 2 - matrix.scaling_window
                windows.append(
                    {
                        "start_call": start_call,
                        "end_call": index + 1,
                        "sample_count": len(window_values),
                        "p50_us": percentile(window_values, 0.50),
                        "p95_us": percentile(window_values, 0.95),
                        "ledger_entries": ledger_count,
                        "ledger_bytes": ledger_size,
                    }
                )
                window_values = []
        if window_values:
            raise HarnessError("sequential matrix did not end on a complete scaling window")
        resident.stop()
        copy_scenario_evidence(resident.paths, self.raw_root, self.combined_observations)
        self.active = None
        headline_values = latencies[: min(1000, len(latencies))]
        return {
            "warmups_excluded": matrix.sequential_warmups,
            "measured_calls": len(latencies),
            "latency_us": latencies,
            "headline_calls_1_to_1000": {
                "count": len(headline_values),
                "p50_us": percentile(headline_values, 0.50),
                "p95_us": percentile(headline_values, 0.95),
                "p99_us": percentile(headline_values, 0.99),
                "max_us": max(headline_values),
            },
            "scaling_window_calls": matrix.scaling_window,
            "scaling_windows": windows,
            "fixture": {
                "artifact_id": setup["artifact_id"],
                "observed_digest": setup["observed_digest"],
                "observed_size_bytes": setup["observed_size_bytes"],
            },
        }

    def measure_throughput(
        self, binary: Path, helper: Path, fixture: Path, matrix: Matrix
    ) -> dict[str, Any]:
        assert self.raw_root is not None and self.client_handle is not None
        assert self.combined_observations is not None
        resident = self.new_resident(binary, "throughput")
        self.active = resident
        pid = resident.start()
        resident.await_ready()
        setup = self.setup_resident(resident, fixture, "throughput")
        template = call_template(helper, resident.paths.root, setup["receiver_authority"])
        # Clients, RSS monitor, and the coordinating main thread start together.
        barrier = threading.Barrier(matrix.throughput_clients + 2)
        monitor_stop = threading.Event()
        failure_lock = threading.Lock()
        failures: list[dict[str, Any]] = []
        client_counts = [0 for _ in range(matrix.throughput_clients)]
        client_attempt_counts = [0 for _ in range(matrix.throughput_clients)]
        peak = self.process_info.sample(pid)
        peak_rss = int(peak["resident_bytes"])
        monitor_times: list[int] = []

        def monitor() -> None:
            nonlocal peak_rss
            barrier.wait()
            next_sample = time.monotonic()
            while not monitor_stop.is_set():
                sampled_at = time.monotonic_ns()
                try:
                    value = self.process_info.sample(pid)
                    peak_rss = max(peak_rss, int(value["resident_bytes"]))
                    monitor_times.append(sampled_at)
                except BaseException as error:
                    with failure_lock:
                        failures.append(
                            {"worker": "rss-monitor", "error": f"{type(error).__name__}: {error}"}
                        )
                    return
                next_sample += 0.010
                monitor_stop.wait(max(0.0, next_sample - time.monotonic()))

        def client(client_index: int) -> None:
            barrier.wait()
            for call_index in range(matrix.throughput_calls_per_client):
                ordinal = call_index + 1
                suffix = f"throughput-{client_index}-{ordinal}"
                try:
                    _, row, body = measured_call(
                        resident.paths.socket,
                        template,
                        suffix,
                        "throughput",
                        ordinal,
                        client_index,
                        require_completed=False,
                    )
                    write_json_line(self.client_handle, row, self.client_lock)
                    client_attempt_counts[client_index] += 1
                    if row["http_status"] == 200 and row["outcome"] == "completed":
                        client_counts[client_index] += 1
                    else:
                        with failure_lock:
                            failures.append(
                                {
                                    "worker": f"client-{client_index}",
                                    "ordinal": ordinal,
                                    "call_id": row["call_id"],
                                    "http_status": row["http_status"],
                                    "outcome": row["outcome"],
                                    "protocol_reason": row["protocol_reason"],
                                    "safe_message": row["safe_message"],
                                    "body": body,
                                }
                            )
                except BaseException as error:
                    client_attempt_counts[client_index] += 1
                    with failure_lock:
                        failures.append(
                            {
                                "worker": f"client-{client_index}",
                                "ordinal": ordinal,
                                "call_id": f"call-perf-{suffix}",
                                "error": f"{type(error).__name__}: {error}",
                            }
                        )

        monitor_thread = threading.Thread(target=monitor, name="perf-rss-monitor")
        workers = [
            threading.Thread(target=client, args=(index,), name=f"perf-client-{index}")
            for index in range(matrix.throughput_clients)
        ]
        cpu_before = self.process_info.sample(pid)
        monitor_thread.start()
        for worker in workers:
            worker.start()
        started = time.monotonic_ns()
        barrier.wait()
        for worker in workers:
            worker.join()
        completed = time.monotonic_ns()
        monitor_stop.set()
        monitor_thread.join()
        cpu_after = self.process_info.sample(pid)
        elapsed = (completed - started) / 1_000_000_000
        resident.stop()
        copy_scenario_evidence(resident.paths, self.raw_root, self.combined_observations)
        self.active = None
        self.failures.extend(failures)
        expected = matrix.throughput_clients * matrix.throughput_calls_per_client
        attempts = sum(client_attempt_counts)
        successes = sum(client_counts)
        if attempts != expected:
            raise HarnessError(
                f"throughput attempt accounting incomplete: attempts={attempts}/{expected}"
            )
        actual_intervals_ms = [
            (right - left) / 1_000_000 for left, right in zip(monitor_times, monitor_times[1:])
        ]
        return {
            "clients": matrix.throughput_clients,
            "calls_per_client": matrix.throughput_calls_per_client,
            "client_attempt_counts": client_attempt_counts,
            "client_success_counts": client_counts,
            "total_calls": attempts,
            "completed_calls": successes,
            "elapsed_seconds": elapsed,
            "calls_per_second": attempts / elapsed,
            "completed_calls_per_second": successes / elapsed,
            "cpu_seconds": float(cpu_after["cpu_seconds"]) - float(cpu_before["cpu_seconds"]),
            "user_cpu_seconds": float(cpu_after["user_cpu_seconds"])
            - float(cpu_before["user_cpu_seconds"]),
            "system_cpu_seconds": float(cpu_after["system_cpu_seconds"])
            - float(cpu_before["system_cpu_seconds"]),
            "peak_rss_bytes": peak_rss,
            "rss_monitor_target_interval_ms": 10,
            "rss_monitor_samples": len(monitor_times),
            "rss_monitor_observed_intervals_ms": actual_intervals_ms,
            "failures": failures,
        }


def self_test() -> None:
    assert percentile([3, 1, 2, 4], 0.50) == 2
    assert percentile([3, 1, 2, 4], 0.95) == 4
    assert summary([1.0, 2.0, 3.0])["median"] == 2.0
    for root in production_roots():
        try:
            assert_safe_path(root / "perf", "self-test")
        except HarnessError:
            pass
        else:
            raise AssertionError(f"production path was not refused: {root}")
    template = {"call_id": "old", "payload": {"inline_payload_ref": "old"}, "trace_context": {}, "protocol_request_id": "old", "idempotency_key": "old"}
    changed = unique_call(template, "x")
    assert changed["call_id"] == "call-perf-x"
    assert template["call_id"] == "old"
    print("perf harness self-test: ok")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--official", action="store_true")
    parser.add_argument("--load-note", default="")
    parser.add_argument("--watch-upstream", type=Path)
    parser.add_argument("--smoke", action="store_true", help="small non-official harness check")
    parser.add_argument("--skip-build", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--skip-fixture-rebuild", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return args
    if args.output is None:
        parser.error("--output is required")
    if args.official == args.smoke:
        parser.error("choose exactly one of --official or --smoke")
    if args.official and (args.skip_build or args.skip_fixture_rebuild):
        parser.error("official mode cannot skip build or fixture verification")
    return args


def main() -> int:
    args = parse_args()
    if args.self_test:
        self_test()
        return 0
    try:
        Harness(args).run_command()
        return 0
    except (HarnessError, OSError, subprocess.CalledProcessError) as error:
        print(f"perf harness: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
