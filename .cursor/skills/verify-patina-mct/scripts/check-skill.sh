#!/usr/bin/env bash
set -euo pipefail

skill_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$skill_dir"

python3 - <<'PY'
import pathlib, re, sys
root = pathlib.Path(".").resolve()
skill = (root / "SKILL.md").read_text()
if not skill.startswith("---\n"):
    sys.exit("SKILL.md is missing frontmatter")
front = skill.split("---", 2)[1]
if not re.search(r"^name:\s*verify-patina-mct\s*$", front, re.M):
    sys.exit("frontmatter name must be verify-patina-mct")
if not re.search(r"^description:\s*\S", front, re.M):
    sys.exit("frontmatter description is empty")
required = ["## Launch", "## Doctor", "## Drive", "## Evidence", "## Cleanup", "## Helpers"]
for heading in required:
    if heading not in skill:
        sys.exit(f"missing {heading}")
browsers = re.compile(r"\b(chrome|chromium|firefox|safari|webkit|edge|brave)\b", re.I)
forbidden_paths = ("~/.mct", "$HOME/.mct", ".mct-pr", ".mct-passkey", ".mct-approval")
texts = [skill]
feature_dir = root / "features"
readme = (feature_dir / "README.md").read_text()
texts.append(readme)
links = re.findall(r"\]\(\./([^)]+)\)", readme)
if not links:
    sys.exit("feature index has no feature links")
seen = []
for name in links:
    path = feature_dir / name
    if not path.is_file():
        sys.exit(f"missing feature file {name}")
    text = path.read_text()
    texts.append(text)
    seen.append(name)
    headings = re.findall(r"^## .+$", text, re.M)
    expect = [
        "## Sub-features",
        "## How to get to it (user POV)",
        "## Driving it with verify-patina-mct",
        "## Gotchas",
    ]
    if headings != expect:
        sys.exit(f"{name} headings are {headings}")
extra = sorted(p.name for p in feature_dir.glob("*.md") if p.name != "README.md" and p.name not in seen)
if extra:
    sys.exit(f"feature files missing from the index: {extra}")
blob = "\n".join(texts)
if "\u2014" in blob or "\u2013" in blob:
    sys.exit("skill text contains a dash character that is not a hyphen")
match = browsers.search(blob)
if match:
    sys.exit(f"skill text names a browser: {match.group(0)}")
for token in forbidden_paths:
    if token in blob:
        sys.exit(f"skill text references a home run directory token: {token}")
helper = root / "scripts" / "verify-patina-mct.sh"
if not helper.is_file():
    sys.exit("helper script is missing")
print(f"check-skill: {len(seen)} feature files")
PY
