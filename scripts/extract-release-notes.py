#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from pathlib import Path

parser = argparse.ArgumentParser(description="Extract one committed MCT release-notes section")
parser.add_argument("version")
parser.add_argument("output", type=Path)
parser.add_argument("--changelog", type=Path, default=Path("CHANGELOG.md"))
args = parser.parse_args()
text = args.changelog.read_text(encoding="utf-8")
pattern = re.compile(
    rf"^## \[{re.escape(args.version)}\](?: - \d{{4}}-\d{{2}}-\d{{2}})?\n(?P<body>.*?)(?=^## \[|^\[Unreleased\]:|\Z)",
    re.MULTILINE | re.DOTALL,
)
match = pattern.search(text)
if not match:
    raise SystemExit(f"CHANGELOG.md has no exact [{args.version}] release section")
notes = f"# MCT {args.version}\n\n" + match.group("body").strip() + "\n"
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(notes, encoding="utf-8", newline="\n")
