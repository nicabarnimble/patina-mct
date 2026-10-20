#!/usr/bin/env python3
"""Run bounded MCT completion proofs without confusing progress with readiness."""

import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "docs/finish-checks.json"
REQUIRED = {
    "independent-policy", "fleet-baseline", "keyed-retry-fence", "administration",
    "canonical-authority", "final-admission", "key-recovery", "fleet-operations",
    "distributed-failures", "useful-workflow", "release-evidence",
}
STATUSES = {"open", "decision_required", "implemented"}
PROOF_PACKAGES = {"mct-daemon", "mct-kernel", "mct-observation"}
NAME = re.compile(r"[A-Za-z0-9_:-]+\Z")
RESULT = re.compile(
    r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;"
)


def validate(data, root=ROOT):
    if not isinstance(data, dict) or data.get("schema") != "mct-finish-check/v1":
        raise ValueError("unknown completion inventory schema")
    requirements, proofs = data.get("requirements"), data.get("proofs")
    if not isinstance(requirements, list) or not isinstance(proofs, dict) or not proofs:
        raise ValueError("requirements and executable proofs are required")
    seen = set()
    referenced = set()
    for row in requirements:
        if not isinstance(row, dict):
            raise ValueError("requirement must be an object")
        identifier = row.get("id")
        if not isinstance(identifier, str) or identifier not in REQUIRED or identifier in seen:
            raise ValueError(f"unknown or duplicate requirement: {identifier!r}")
        seen.add(identifier)
        if row.get("status") not in STATUSES or not isinstance(row.get("title"), str) or not row["title"].strip():
            raise ValueError(f"invalid status/title: {identifier}")
        references, evidence = row.get("proofs"), row.get("evidence")
        if not isinstance(references, list) or not isinstance(evidence, list):
            raise ValueError(f"proof/evidence lists required: {identifier}")
        if any(not isinstance(proof, str) or proof not in proofs for proof in references):
            raise ValueError(f"unknown proof: {identifier}")
        referenced.update(references)
        if row["status"] == "implemented" and (not references or not evidence):
            raise ValueError(f"implemented requirement lacks proof/evidence: {identifier}")
        for relative in evidence:
            if not isinstance(relative, str):
                raise ValueError(f"invalid evidence path: {identifier}")
            path = (root / relative).resolve()
            if not path.is_relative_to(root.resolve()) or not path.is_file():
                raise ValueError(f"missing or external evidence: {relative}")
    if seen != REQUIRED:
        raise ValueError(f"missing completion requirements: {sorted(REQUIRED - seen)}")
    if referenced != set(proofs):
        raise ValueError("every executable proof must belong to a requirement")
    for identifier, proof in proofs.items():
        if not isinstance(proof, dict):
            raise ValueError(f"invalid proof: {identifier}")
        package = proof.get("package", "mct-daemon")
        if not isinstance(package, str) or package not in PROOF_PACKAGES:
            raise ValueError(f"invalid cargo proof package: {identifier}")
        target = proof.get("target")
        valid_target = target == ["--lib"] or (
            isinstance(target, list) and len(target) == 2
            and target[0] in {"--bin", "--test"}
            and isinstance(target[1], str) and NAME.fullmatch(target[1])
        )
        if not valid_target or not isinstance(proof.get("filter"), str) or not NAME.fullmatch(proof["filter"]):
            raise ValueError(f"invalid cargo test target/filter: {identifier}")
        minimum = proof.get("minimum_tests")
        if type(minimum) is not int or minimum < 1:
            raise ValueError(f"positive minimum test count required: {identifier}")
    return data


def inventory(data):
    incomplete = []
    for row in data["requirements"]:
        print(f"{row['status']:17} {row['id']}: {row['title']}", flush=True)
        if row["status"] != "implemented":
            incomplete.append(row["id"])
    print(f"\n{len(incomplete)} completion obligations remain. Status is not a test receipt.", flush=True)
    return not incomplete


def command(args, timeout=900):
    print("\n+ " + " ".join(args), flush=True)
    # All proof subprocesses (including their resident fixtures) belong to a
    # private process group. Timeout/interrupt must not leave test Mothers alive.
    with subprocess.Popen(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, start_new_session=True) as process:
        try:
            output, _ = process.communicate(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            output, _ = process.communicate()
            print(output, end="", flush=True)
            raise
        print(output, end="", flush=True)
        if process.returncode:
            raise RuntimeError(f"command failed ({process.returncode}): {' '.join(args)}")
        return output


def registered_count(output, minimum):
    count = sum(line.endswith(": test") for line in output.splitlines())
    if count < minimum:
        raise RuntimeError(f"test filter selected {count}, expected at least {minimum}")
    return count


def verify_result(output, selected):
    results = RESULT.findall(output)
    if len(results) != 1:
        raise RuntimeError("expected exactly one successful test-binary summary")
    passed, failed, ignored, measured = map(int, results[0])
    if passed != selected or failed or ignored or measured:
        raise RuntimeError(f"selected tests did not all run and pass: {results[0]}")


def run_proofs(data):
    failures = []
    for identifier, proof in data["proofs"].items():
        print(f"\n=== completion proof: {identifier} ===", flush=True)
        cargo = ["cargo", "test", "--locked", "-p", proof.get("package", "mct-daemon"),
                 *proof["target"], proof["filter"]]
        try:
            selected = registered_count(command(cargo + ["--", "--list"]), proof["minimum_tests"])
            verify_result(command(cargo + ["--", "--nocapture", "--test-threads=1"]), selected)
        except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
            print(f"FAIL {identifier}: {error}", file=sys.stderr, flush=True)
            failures.append(identifier)
    print(f"\nImplemented proof groups: {len(data['proofs']) - len(failures)} passed; {len(failures)} failed.", flush=True)
    return not failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", nargs="?", default="check",
                        choices=["validate", "status", "proofs", "check", "release"])
    args = parser.parse_args()
    try:
        data = validate(json.loads(MANIFEST.read_text()))
        if args.mode == "validate":
            print("Completion inventory is structurally valid; no readiness claim.")
            return 0
        if args.mode == "status":
            return 0 if inventory(data) else 1
        passed = run_proofs(data)
        if args.mode == "release":
            for script in ["scripts/ci-tier0.sh", "scripts/build-docs.sh"]:
                try:
                    command(["bash", script], timeout=3600)
                except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
                    print(f"FAIL {script}: {error}", file=sys.stderr)
                    passed = False
        if args.mode == "proofs":
            print("\nImplemented proofs only; this is NOT release readiness.")
            return 0 if passed else 1
        complete = inventory(data)
        print("\nCompletion check passed; operator release review still required."
              if passed and complete else "\nNOT FINISHED: failed proofs or open completion obligations.")
        return 0 if passed and complete else 1
    except (OSError, ValueError, TypeError) as error:
        print(f"Invalid completion inventory: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
