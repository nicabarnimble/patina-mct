#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
version=${1:?usage: check-release-notes.sh <version>}
temp=$(mktemp "${TMPDIR:-/tmp}/mct-release-notes.XXXXXX")
trap 'rm -f "$temp"' EXIT
python3 scripts/extract-release-notes.py "$version" "$temp"
python3 - "$version" "$temp" <<'PY'
import re
import sys
from pathlib import Path
version, path = sys.argv[1], Path(sys.argv[2])
notes = path.read_text(encoding="utf-8")
assert notes.startswith(f"# MCT {version}\n\n")
sections = re.findall(r"^### (Added|Changed|Fixed|Security)$", notes, re.MULTILINE)
assert sections == ["Added", "Changed", "Fixed", "Security"], sections
bullets = [line for line in notes.splitlines() if line.startswith("- ")]
assert bullets, "release notes contain no entries"
pattern = re.compile(r"\(\[#\d+\]\(https://github\.com/nicabarnimble/patina-mct/pull/\d+\)\)\.$")
for bullet in bullets:
    assert pattern.search(bullet), f"release note has no terminal PR link: {bullet}"
assert "session-" not in notes.lower()
print(f"release-notes: {version} has categorized PR-linked notes")
PY
