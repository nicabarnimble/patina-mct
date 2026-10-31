#!/usr/bin/env python3
"""Run pinned OSV; classify one exact-version false positive, not an ID-wide ignore.

GHSA-qwgh-2vcv-g2f7 concerns an inline cursor introduced in block-buffer 0.11.
0.10.4 stores pos separately and has no ReadBuffer. See the upstream 0.11
changelog and crates/mct-kernel/tests/block_buffer_advisory.rs. OSV currently
uses an overbroad introduced=0 range. Never exempt 0.11/0.12.0 or another ID.
Revisit by 2027-01-29 or when Wasmtime drops this older transitive dependency.
"""
import datetime
import json
from pathlib import Path
import subprocess
import sys

ADVISORY = "GHSA-qwgh-2vcv-g2f7"
REVIEW_BY = datetime.date(2027, 1, 29)
ROOT = Path(__file__).resolve().parent.parent
LOCKS = {str(ROOT / "Cargo.lock"), str(ROOT / "fuzz/Cargo.lock")}


def only_known_false_positive(report, today):
    if today >= REVIEW_BY:
        return False
    try:
        sources = report["results"]
        if not isinstance(sources, list) or not sources:
            return False
        if {s["source"]["path"] for s in sources} != LOCKS or len(sources) != 2:
            return False
        for source in sources:
            if source["source"]["type"] != "lockfile":
                return False
            packages = source["packages"]
            if not isinstance(packages, list) or not packages:
                return False
            for package in packages:
                identity = package["package"]
                if (identity["ecosystem"], identity["name"], identity["version"]) != (
                        "crates.io", "block-buffer", "0.10.4"):
                    return False
                vulns = package["vulnerabilities"]
                groups = package["groups"]
                if not vulns or not groups:
                    return False
                if any(v["id"] != ADVISORY for v in vulns):
                    return False
                if any(g["ids"] != [ADVISORY] or g["aliases"] != [ADVISORY]
                       for g in groups):
                    return False
        return True
    except (KeyError, TypeError):
        return False


def scan_status(returncode, output, today):
    if returncode != 1:
        return 0 if returncode == 0 else 1
    try:
        return 0 if only_known_false_positive(json.loads(output), today) else 1
    except (ValueError, TypeError):
        return 1


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: scan-osv.py /path/to/osv-scanner")
    result = subprocess.run([
        sys.argv[1], "scan", "source", "--format", "json", "--config", "osv-scanner.toml",
        "-L", "Cargo.lock", "-L", "fuzz/Cargo.lock",
    ], cwd=ROOT, capture_output=True, text=True)
    # Preserve the complete scanner report, including the classified finding.
    print(result.stdout)
    print(result.stderr, file=sys.stderr)
    status = scan_status(result.returncode, result.stdout, datetime.date.today())
    if result.returncode == 1 and status == 0:
        print(f"NOT AFFECTED: {ADVISORY} in block-buffer 0.10.4 only: separate cursor, "
              f"no ReadBuffer; review expires {REVIEW_BY}. All other findings still fail.")
    raise SystemExit(status)


if __name__ == "__main__":
    main()
