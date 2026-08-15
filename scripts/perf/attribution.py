#!/usr/bin/env python3
"""Derive MCT call-stage timings and durability accounting from raw ledger frames."""

from __future__ import annotations

import argparse
from collections import defaultdict
from datetime import datetime
import json
import math
from pathlib import Path
import statistics
import sys
from typing import Any

CAVEAT = (
    "Dev-launched, harness-supervised, unsupervised-by-launchd release-Cargo-profile "
    "binary; not a release artifact and not directly comparable to "
    "BASELINES-v0.2.0-aarch64-apple-darwin.md."
)
SCHEMA = "mct-perf-phase-0-attribution/v1"
DURABILITY_CLASSES = ("before_effect", "buffered", "projection_only")
MAX_COMMITTED_JSON_BYTES = 5_000_000


class AttributionError(RuntimeError):
    """Malformed, ambiguous, or inconsistent evidence."""


def percentile(values: list[float], percent: float) -> float:
    if not values:
        raise AttributionError("cannot summarize empty samples")
    ordered = sorted(values)
    return ordered[max(0, min(len(ordered) - 1, math.ceil(percent * len(ordered)) - 1))]


def sample_summary(values: list[float]) -> dict[str, Any]:
    if not values:
        return {"count": 0, "p50_us": None, "p95_us": None, "max_us": None}
    return {
        "count": len(values),
        "p50_us": percentile(values, 0.50),
        "p95_us": percentile(values, 0.95),
        "max_us": max(values),
    }


def integer_summary(values: list[int]) -> dict[str, Any]:
    if not values:
        return {"count": 0, "p50": None, "p95": None, "max": None, "total": 0}
    floats = [float(value) for value in values]
    return {
        "count": len(values),
        "p50": int(percentile(floats, 0.50)),
        "p95": int(percentile(floats, 0.95)),
        "max": max(values),
        "total": sum(values),
    }


def parse_timestamp(value: str) -> datetime:
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise AttributionError(f"invalid observation timestamp {value!r}") from error


def elapsed_us(start: dict[str, Any], end: dict[str, Any]) -> float:
    return (parse_timestamp(end["observed_at"]) - parse_timestamp(start["observed_at"])).total_seconds() * 1_000_000


def load_clients(path: Path) -> list[dict[str, Any]]:
    clients: list[dict[str, Any]] = []
    seen: set[str] = set()
    with path.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            try:
                value = json.loads(line)
            except json.JSONDecodeError as error:
                raise AttributionError(f"{path}:{line_number}: malformed client JSON") from error
            call_id = value.get("call_id")
            if not isinstance(call_id, str) or not call_id:
                raise AttributionError(f"{path}:{line_number}: missing call_id")
            if call_id in seen:
                raise AttributionError(f"duplicate measured call_id {call_id}")
            if not isinstance(value.get("duration_us"), (int, float)):
                raise AttributionError(f"{path}:{line_number}: missing client duration")
            seen.add(call_id)
            clients.append(value)
    if not clients:
        raise AttributionError(f"no measured client calls in {path}")
    return clients


def load_ledger(
    path: Path, measured: set[str]
) -> tuple[dict[str, list[dict[str, Any]]], dict[str, dict[str, int]], dict[str, Any]]:
    observations: dict[str, list[dict[str, Any]]] = defaultdict(list)
    accounting: dict[str, dict[str, int]] = {
        call_id: {
            **{f"{kind}_entries": 0 for kind in DURABILITY_CLASSES},
            **{f"{kind}_bytes": 0 for kind in DURABILITY_CLASSES},
            "total_entries": 0,
            "total_bytes": 0,
        }
        for call_id in measured
    }
    total_frames = 0
    total_bytes = 0
    matched_frames = 0
    with path.open("rb") as handle:
        for line_number, raw in enumerate(handle, 1):
            total_frames += 1
            total_bytes += len(raw)
            try:
                entry = json.loads(raw)
            except json.JSONDecodeError as error:
                raise AttributionError(f"{path}:{line_number}: malformed ledger JSON") from error
            observation = entry.get("observation")
            if not isinstance(observation, dict):
                raise AttributionError(f"{path}:{line_number}: missing observation")
            call_id = observation.get("call_id")
            if call_id not in measured:
                continue
            durability = entry.get("durability_class")
            if durability not in DURABILITY_CLASSES:
                raise AttributionError(
                    f"{path}:{line_number}: unknown durability_class {durability!r}"
                )
            observed_at = observation.get("observed_at")
            if not isinstance(observed_at, str):
                raise AttributionError(f"{path}:{line_number}: missing observed_at")
            parse_timestamp(observed_at)
            matched_frames += 1
            observations[call_id].append(
                {
                    "local_sequence": entry.get("local_sequence"),
                    "observed_at": observed_at,
                    "kind": observation.get("kind"),
                    "safe_message": observation.get("safe_message"),
                    "observation_id": observation.get("observation_id"),
                    "durability_class": durability,
                    "frame_bytes": len(raw),
                }
            )
            values = accounting[call_id]
            values[f"{durability}_entries"] += 1
            values[f"{durability}_bytes"] += len(raw)
            values["total_entries"] += 1
            values["total_bytes"] += len(raw)
    for values in observations.values():
        values.sort(key=lambda item: (parse_timestamp(item["observed_at"]), item["local_sequence"] or 0))
    return observations, accounting, {
        "total_ledger_frames": total_frames,
        "total_ledger_bytes": total_bytes,
        "matched_call_frames": matched_frames,
        "measured_calls_with_frames": len(observations),
    }


def first_message(values: list[dict[str, Any]], message: str) -> dict[str, Any] | None:
    return next((value for value in values if value["safe_message"] == message), None)


def last_message(values: list[dict[str, Any]], message: str) -> dict[str, Any] | None:
    return next((value for value in reversed(values) if value["safe_message"] == message), None)


def first_message_after(
    values: list[dict[str, Any]], message: str, boundary: dict[str, Any] | None
) -> dict[str, Any] | None:
    if boundary is None:
        return None
    boundary_time = parse_timestamp(boundary["observed_at"])
    return next(
        (
            value
            for value in values
            if value["safe_message"] == message
            and parse_timestamp(value["observed_at"]) >= boundary_time
        ),
        None,
    )


BOUNDARY_DEFINITIONS = {
    "submission_received": {
        "kind": "call_received",
        "safe_message": "authenticated local call received",
        "validity": "persisted local ingress timestamp",
    },
    "submission_constructed": {
        "kind": "call_constructed",
        "safe_message": "local call accepted for evaluation",
        "validity": "persisted local ingress timestamp",
    },
    "route_selected": {
        "kind": "route_selected",
        "safe_message": "route selected",
        "validity": "initial decision observation",
    },
    "route_revalidated": {
        "kind": "route_revalidated",
        "safe_message": "route revalidated",
        "validity": "distinct execution authority revalidation observation",
    },
    "before_effect_first_toy": {
        "kind": "toy_grant_allowed",
        "safe_message": "toy grant allowed",
        "validity": "first such observation timestamp after route revalidated",
    },
    "wasm_started": {
        "kind": "runtime_execution_started",
        "safe_message": "wasm component execution started",
        "validity": "existing component-start boundary",
    },
    "nominal_wasm_completed": {
        "kind": "runtime_execution_completed",
        "safe_message": "wasm component execution completed",
        "validity": "audited but invalid as completion clock because timestamp is pre-sampled",
    },
    "wasm_completed_proxy": {
        "kind": "runtime_execution_completed",
        "safe_message": "runtime execution observed",
        "validity": "obs-executed-on truthful post-runtime-return proxy",
    },
    "terminal_result": {
        "kind": "result_recorded",
        "safe_message": "local call result recorded",
        "validity": "durable terminal fact before UDS response",
    },
}


STAGE_DEFINITIONS = [
    {
        "id": "submission_observations",
        "start": "submission_received",
        "end": "submission_constructed",
        "interpretation": "CallReceived to CallConstructed in local_submission_observations",
        "code_anchor": "crates/mct-daemon/src/daemon/resident/local_ingress.rs",
    },
    {
        "id": "combined_pre_route",
        "start": "submission_constructed",
        "end": "route_selected",
        "interpretation": "receiver echo, deadline, payload, idempotency, decision snapshot, candidates, and initial route; successful deadline/idempotency has no dedicated timestamp",
        "code_anchor": "crates/mct-daemon/src/daemon/resident/pipeline.rs; crates/mct-daemon/src/daemon/resident/decision.rs",
    },
    {
        "id": "effect_authority_revalidation",
        "start": "route_selected",
        "end": "route_revalidated",
        "interpretation": "durable initial route through distinct effect-boundary authority revalidation",
        "code_anchor": "crates/mct-daemon/src/daemon/resident/execution.rs",
    },
    {
        "id": "before_effect_facts",
        "start": "route_revalidated",
        "end": "before_effect_first_toy",
        "interpretation": "effect revalidation through first required effect-time Toy authority fact",
        "code_anchor": "crates/mct-daemon/src/daemon/resident/execution.rs",
    },
    {
        "id": "runtime_preparation",
        "start": "before_effect_first_toy",
        "end": "wasm_started",
        "interpretation": "required Toy facts, append acknowledgement, runtime construction/import discovery, and component-start boundary",
        "code_anchor": "crates/mct-daemon/src/daemon/resident/execution.rs; crates/mct-daemon/src/wasm.rs",
    },
    {
        "id": "wasm_execution_proxy",
        "start": "wasm_started",
        "end": "wasm_completed_proxy",
        "interpretation": "WASM started observation to truthful post-invocation obs-executed-on proxy",
        "code_anchor": "crates/mct-daemon/src/wasm.rs; crates/mct-daemon/src/daemon/resident/execution.rs",
    },
    {
        "id": "terminal_persistence",
        "start": "wasm_completed_proxy",
        "end": "terminal_result",
        "interpretation": "post-invocation proxy through payload facts and durable local ResultRecorded before UDS reply",
        "code_anchor": "crates/mct-daemon/src/daemon/resident/execution.rs; crates/mct-daemon/src/daemon/resident/local_ingress.rs",
    },
]


def call_boundaries(values: list[dict[str, Any]]) -> dict[str, dict[str, Any] | None]:
    route_selected = first_message(values, "route selected")
    route_revalidated = last_message(values, "route revalidated")
    return {
        "submission_received": first_message(values, "authenticated local call received"),
        "submission_constructed": first_message(values, "local call accepted for evaluation"),
        "route_selected": route_selected,
        "route_revalidated": route_revalidated,
        "before_effect_first_toy": first_message_after(
            values, "toy grant allowed", route_revalidated
        ),
        "wasm_started": first_message(values, "wasm component execution started"),
        "nominal_wasm_completed": first_message(values, "wasm component execution completed"),
        "wasm_completed_proxy": first_message(values, "runtime execution observed"),
        "terminal_result": last_message(values, "local call result recorded"),
    }


def boundary_projection(value: dict[str, Any] | None) -> dict[str, Any] | None:
    if value is None:
        return None
    return {
        "observed_at": value["observed_at"],
        "observation_id": value["observation_id"],
        "safe_message": value["safe_message"],
    }


def derive(run_path: Path, ledger_path: Path, clients_path: Path) -> dict[str, Any]:
    run = json.loads(run_path.read_text(encoding="utf-8"))
    clients = load_clients(clients_path)
    call_ids = [value["call_id"] for value in clients]
    observations, accounting, ledger_meta = load_ledger(ledger_path, set(call_ids))
    stage_arrays: dict[str, list[float | None]] = {
        definition["id"]: [] for definition in STAGE_DEFINITIONS
    }
    remainder_raw: list[float | None] = []
    remainder_clamped: list[float | None] = []
    call_index: list[dict[str, Any]] = []
    anomalies: list[dict[str, Any]] = []
    nominal_timestamp_records: list[dict[str, Any]] = []

    for client in clients:
        call_id = client["call_id"]
        values = observations.get(call_id, [])
        boundaries = call_boundaries(values)
        attributed = 0.0
        all_stages_present = True
        call_stage_values: dict[str, float | None] = {}
        for definition in STAGE_DEFINITIONS:
            start = boundaries[definition["start"]]
            end = boundaries[definition["end"]]
            duration: float | None = None
            if start is not None and end is not None:
                duration = elapsed_us(start, end)
                if duration < 0:
                    anomalies.append(
                        {
                            "call_id": call_id,
                            "kind": "negative_stage_duration",
                            "stage": definition["id"],
                            "duration_us": duration,
                            "start": boundary_projection(start),
                            "end": boundary_projection(end),
                        }
                    )
                    duration = None
            if duration is None:
                all_stages_present = False
            else:
                attributed += duration
            stage_arrays[definition["id"]].append(duration)
            call_stage_values[definition["id"]] = duration

        client_total = float(client["duration_us"])
        raw_remainder = client_total - attributed if all_stages_present else None
        clamped = max(raw_remainder, 0.0) if raw_remainder is not None else None
        if raw_remainder is not None and raw_remainder < 0:
            anomalies.append(
                {
                    "call_id": call_id,
                    "kind": "negative_unattributed_remainder",
                    "raw_remainder_us": raw_remainder,
                    "client_total_us": client_total,
                    "attributed_us": attributed,
                }
            )
        remainder_raw.append(raw_remainder)
        remainder_clamped.append(clamped)

        nominal = boundaries["nominal_wasm_completed"]
        started = boundaries["wasm_started"]
        proxy = boundaries["wasm_completed_proxy"]
        if nominal is not None:
            nominal_timestamp_records.append(
                {
                    "call_id": call_id,
                    "nominal_completed_observed_at": nominal["observed_at"],
                    "wasm_started_observed_at": started["observed_at"] if started else None,
                    "post_invocation_proxy_observed_at": proxy["observed_at"] if proxy else None,
                    "nominal_minus_started_us": elapsed_us(started, nominal) if started else None,
                    "proxy_minus_nominal_us": elapsed_us(nominal, proxy) if proxy else None,
                }
            )
        call_index.append(
            {
                "call_id": call_id,
                "scenario": client.get("scenario"),
                "client_index": client.get("client_index"),
                "ordinal": client.get("ordinal"),
                "http_status": client.get("http_status"),
                "outcome": client.get("outcome"),
                "client_total_us": client_total,
                "has_ledger_frames": bool(values),
                "complete_stage_chain": all_stages_present,
                "boundaries": {
                    name: boundary_projection(value) for name, value in boundaries.items()
                },
            }
        )

    stages: dict[str, Any] = {}
    for definition in STAGE_DEFINITIONS:
        samples = stage_arrays[definition["id"]]
        eligible = [value for value in samples if value is not None]
        stages[definition["id"]] = {
            **definition,
            **sample_summary(eligible),
            "eligible_calls": len(eligible),
            "excluded_calls": len(samples) - len(eligible),
            "durations_us": samples,
        }

    durability: dict[str, Any] = {
        "call_order": call_ids,
        "semantics": "Each matched persisted JSONL frame contributes one entry and its exact source-line byte length including newline. Actor-message batch count is not inferred.",
        "classes": {},
    }
    for durability_class in DURABILITY_CLASSES:
        counts = [accounting[call_id][f"{durability_class}_entries"] for call_id in call_ids]
        byte_values = [accounting[call_id][f"{durability_class}_bytes"] for call_id in call_ids]
        durability["classes"][durability_class] = {
            "entries_per_call": counts,
            "bytes_per_call": byte_values,
            "entry_summary": integer_summary(counts),
            "byte_summary": integer_summary(byte_values),
        }
    total_counts = [accounting[call_id]["total_entries"] for call_id in call_ids]
    total_byte_values = [accounting[call_id]["total_bytes"] for call_id in call_ids]
    durability["total"] = {
        "entries_per_call": total_counts,
        "bytes_per_call": total_byte_values,
        "entry_summary": integer_summary(total_counts),
        "byte_summary": integer_summary(total_byte_values),
    }

    raw_values = [value for value in remainder_raw if value is not None]
    clamped_values = [value for value in remainder_clamped if value is not None]
    outcome_counts: dict[str, int] = defaultdict(int)
    for client in clients:
        outcome_counts[str(client.get("outcome"))] += 1
    result = {
        "schema": SCHEMA,
        "caveat": CAVEAT,
        "covered_revision": run.get("covered_revision"),
        "harness_revision": run.get("harness_revision"),
        "source": {
            "run": str(run_path),
            "ledger": str(ledger_path),
            "clients": str(clients_path),
            "ledger_frame_semantics": "binary readline; exact len(raw_frame) includes newline",
        },
        "percentile_method": "nearest_rank_sorted_ceil_p_times_n_minus_1",
        "measured_calls": len(clients),
        "call_order": call_ids,
        "outcomes": dict(sorted(outcome_counts.items())),
        "client_total": sample_summary([float(value["duration_us"]) for value in clients]),
        "ledger": ledger_meta,
        "boundary_mapping": BOUNDARY_DEFINITIONS,
        "stage_mapping": STAGE_DEFINITIONS,
        "stages": stages,
        "unattributed_remainder": {
            "definition": "max(client_total_us - sum(nonoverlapping_complete_stage_chain_us), 0); raw negative retained as anomaly",
            "eligible_calls": len(clamped_values),
            "excluded_calls": len(clients) - len(clamped_values),
            "raw_us": remainder_raw,
            "clamped_us": remainder_clamped,
            "raw_summary": sample_summary(raw_values),
            "clamped_summary": sample_summary(clamped_values),
        },
        "durability_classes": durability,
        "call_index": call_index,
        "clock_anomalies": anomalies,
        "nominal_wasm_completed_timestamp_audit": {
            "used_as_completion_boundary": False,
            "reason": "execution.rs creates both nominal invocation timestamps before runtime invocation; obs-executed-on is the truthful post-return proxy",
            "records": nominal_timestamp_records,
        },
        "attribution_gaps": [
            {
                "id": "successful_deadline_idempotency_boundary_unavailable",
                "detail": "Successful deadline admission and fresh idempotency reservation emit no dedicated persisted observation; combined_pre_route cannot be split honestly.",
            },
            {
                "id": "nominal_wasm_completed_timestamp_presampled",
                "detail": "The nominal wasm-completed observed_at is pre-sampled with the start ID before invoke and is not a completion clock; runtime execution observed (obs-executed-on) is used as post-invocation proxy.",
            },
            {
                "id": "pre_wasm_internal_split_unavailable",
                "detail": "Existing observations cannot separately time snapshot replay, Child reload/hash, SQLite open, Engine construction, import-discovery compilation, adapter construction, and invoke-path compilation.",
            },
            {
                "id": "failed_calls_have_partial_chains",
                "detail": "Non-completed concurrent calls remain in durability accounting and call_index but are excluded stage-by-stage where required boundaries do not exist.",
            },
        ],
    }
    return result


def render_markdown(value: dict[str, Any]) -> str:
    lines = [
        "# MCT Performance Phase 0 — ledger-derived attribution",
        "",
        f"> **Non-comparability notice:** {CAVEAT}",
        "",
        f"- Covered revision: `{value['covered_revision']}`",
        f"- Measured calls: {value['measured_calls']}",
        f"- Outcomes: `{json.dumps(value['outcomes'], sort_keys=True)}`",
        f"- Client total p50/p95: {value['client_total']['p50_us']:.3f} / {value['client_total']['p95_us']:.3f} µs",
        f"- Percentiles: `{value['percentile_method']}`",
        "",
        "## Stage timing",
        "",
        "| Stage | Eligible | Excluded | p50 µs | p95 µs | Max µs |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for stage_id, stage in value["stages"].items():
        p50 = "—" if stage["p50_us"] is None else f"{stage['p50_us']:.3f}"
        p95 = "—" if stage["p95_us"] is None else f"{stage['p95_us']:.3f}"
        maximum = "—" if stage["max_us"] is None else f"{stage['max_us']:.3f}"
        lines.append(
            f"| `{stage_id}` | {stage['eligible_calls']} | {stage['excluded_calls']} | {p50} | {p95} | {maximum} |"
        )
    remainder = value["unattributed_remainder"]
    lines.extend(
        [
            "",
            "## Unattributed remainder",
            "",
            "| Eligible | Excluded | p50 µs | p95 µs | Negative raw anomalies |",
            "|---:|---:|---:|---:|---:|",
            f"| {remainder['eligible_calls']} | {remainder['excluded_calls']} | {remainder['clamped_summary']['p50_us'] or 0:.3f} | {remainder['clamped_summary']['p95_us'] or 0:.3f} | {sum(1 for item in value['clock_anomalies'] if item['kind'] == 'negative_unattributed_remainder')} |",
            "",
            "## Durability-class accounting",
            "",
            "Exact bytes include each JSONL frame's trailing newline. Counts are persisted entries, not inferred actor-message batches.",
            "",
            "| DurabilityClass | Total entries | Entries/call p50/p95/max | Total bytes | Bytes/call p50/p95/max |",
            "|---|---:|---:|---:|---:|",
        ]
    )
    for name, item in value["durability_classes"]["classes"].items():
        entry = item["entry_summary"]
        byte = item["byte_summary"]
        lines.append(
            f"| `{name}` | {entry['total']} | {entry['p50']} / {entry['p95']} / {entry['max']} | {byte['total']} | {byte['p50']} / {byte['p95']} / {byte['max']} |"
        )
    lines.extend(["", "## Boundary mapping", ""])
    for boundary_id, boundary in value["boundary_mapping"].items():
        lines.append(
            f"- **`{boundary_id}`** — kind `{boundary['kind']}`, safe message `{boundary['safe_message']}`; {boundary['validity']}."
        )
    lines.extend(["", "## Stage-to-observation mapping", ""])
    for stage in value["stage_mapping"]:
        lines.append(
            f"- **`{stage['id']}`** — `{stage['start']}` → `{stage['end']}`; {stage['interpretation']}. Code: `{stage['code_anchor']}`."
        )
    lines.extend(["", "## Attribution gaps", ""])
    for gap in value["attribution_gaps"]:
        lines.append(f"- **`{gap['id']}`** — {gap['detail']}")
    lines.append("")
    return "\n".join(lines)


def write_outputs(value: dict[str, Any], json_path: Path, markdown_path: Path) -> None:
    encoded = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    if len(encoded) > MAX_COMMITTED_JSON_BYTES:
        raise AttributionError(
            f"attribution JSON would be {len(encoded)} bytes, above D-P0.10 limit {MAX_COMMITTED_JSON_BYTES}"
        )
    json_path.write_bytes(encoded)
    markdown_path.write_text(render_markdown(value), encoding="utf-8")


def self_test() -> None:
    assert percentile([4, 1, 3, 2], 0.50) == 2
    assert percentile([4, 1, 3, 2], 0.95) == 4
    start = {"observed_at": "2026-01-01T00:00:00.000000Z"}
    end = {"observed_at": "2026-01-01T00:00:00.001000Z"}
    assert elapsed_us(start, end) == 1000
    print("perf attribution self-test: ok")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", type=Path)
    parser.add_argument("--ledger", type=Path)
    parser.add_argument("--clients", type=Path)
    parser.add_argument("--json", dest="json_path", type=Path)
    parser.add_argument("--markdown", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if not args.self_test:
        missing = [name for name in ("run", "ledger", "clients", "json_path", "markdown") if getattr(args, name) is None]
        if missing:
            parser.error("required arguments missing: " + ", ".join(missing))
    return args


def main() -> int:
    args = parse_args()
    try:
        if args.self_test:
            self_test()
        else:
            value = derive(args.run, args.ledger, args.clients)
            write_outputs(value, args.json_path, args.markdown)
        return 0
    except (AttributionError, OSError, json.JSONDecodeError) as error:
        print(f"perf attribution: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
