#!/usr/bin/env python3
"""Require a reason and a revisit date or condition on every audit.toml ignore.

`--overdue` fails when a `revisit-by` date is before today (UTC). An entry
that only has `revisit-when` is not date-overdue.
"""

from __future__ import annotations

import argparse
import re
import sys
from datetime import date, datetime, timezone
from pathlib import Path

ADVISORY_RE = re.compile(r'"((?:RUSTSEC|CVE)-[^"]+)"')
KEY_RE = re.compile(r"^(reason|revisit-by|revisit-when):\s*(.*?)\s*$")
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def parse_ignores(text: str) -> list[dict[str, str]]:
    lines = text.splitlines()
    start = None
    for index, line in enumerate(lines):
        if re.match(r"^\s*ignore\s*=\s*\[", line):
            start = index + 1
            break
    if start is None:
        raise ValueError("audit config has no advisories.ignore array")

    entries: list[dict[str, str]] = []
    pending: dict[str, str] = {}
    for line in lines[start:]:
        stripped = line.strip()
        if stripped == "]":
            break
        if not stripped:
            pending = {}
            continue
        if stripped.startswith("#"):
            match = KEY_RE.match(stripped[1:].strip())
            if match:
                pending[match.group(1)] = match.group(2)
            continue
        advisory = ADVISORY_RE.search(stripped)
        if advisory is None:
            continue
        entry = dict(pending)
        entry["id"] = advisory.group(1)
        entries.append(entry)
        pending = {}
    return entries


def validate(entries: list[dict[str, str]], *, overdue: bool, today: date) -> list[str]:
    errors: list[str] = []
    seen: set[str] = set()
    for entry in entries:
        advisory_id = entry["id"]
        if advisory_id in seen:
            errors.append(f"{advisory_id}: repeated ignore")
        seen.add(advisory_id)
        reason = entry.get("reason", "").strip()
        revisit_by = entry.get("revisit-by", "").strip()
        revisit_when = entry.get("revisit-when", "").strip()
        if not reason:
            errors.append(f"{advisory_id}: missing reason")
        if not revisit_by and not revisit_when:
            errors.append(f"{advisory_id}: missing revisit-by date or revisit-when condition")
        if revisit_by and not DATE_RE.fullmatch(revisit_by):
            errors.append(f"{advisory_id}: revisit-by must be YYYY-MM-DD")
            continue
        if overdue and revisit_by:
            try:
                due = date.fromisoformat(revisit_by)
            except ValueError:
                errors.append(f"{advisory_id}: revisit-by is not a calendar date")
                continue
            if due < today:
                errors.append(
                    f"{advisory_id}: revisit-by {revisit_by} is overdue "
                    f"(reason: {reason or 'unspecified'})"
                )
    return errors


def self_test() -> None:
    today = date(2026, 10, 14)
    valid = """
[advisories]
ignore = [
    # reason: transitive and unused at runtime
    # revisit-by: 2027-04-14
    "RUSTSEC-2024-0001",
    # reason: waiting on an upstream refresh
    # revisit-when: the next Iroh refresh
    "RUSTSEC-2024-0002",
]
"""
    missing = """
[advisories]
ignore = [
    "RUSTSEC-2024-0003",
]
"""
    overdue = """
[advisories]
ignore = [
    # reason: still transitive
    # revisit-by: 2026-01-01
    "RUSTSEC-2024-0004",
]
"""
    cases = (
        (valid, False, True),
        (valid, True, True),
        (missing, False, False),
        (overdue, False, True),
        (overdue, True, False),
    )
    for text, check_overdue, expect_ok in cases:
        errors = validate(parse_ignores(text), overdue=check_overdue, today=today)
        if expect_ok != (not errors):
            raise SystemExit(f"self-test failed: {errors!r} for overdue={check_overdue}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--file", type=Path, default=Path(".cargo/audit.toml"))
    parser.add_argument("--overdue", action="store_true")
    parser.add_argument("--today", help="UTC date YYYY-MM-DD; tests only")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("audit ignore self-test passed")
        return 0
    today = date.fromisoformat(args.today) if args.today else datetime.now(timezone.utc).date()
    entries = parse_ignores(args.file.read_text(encoding="utf-8"))
    errors = validate(entries, overdue=args.overdue, today=today)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    kind = "overdue check" if args.overdue else "metadata check"
    print(f"audit ignore {kind} passed ({len(entries)} ignores, today {today.isoformat()})")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as exc:
        print(exc, file=sys.stderr)
        raise SystemExit(1) from exc
