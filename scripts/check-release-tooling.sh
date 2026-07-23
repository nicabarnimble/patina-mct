#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
python3 - <<'PY'
import ast
from pathlib import Path
for name in (
    "scripts/extract-release-notes.py",
    "scripts/normalize-release-sbom.py",
    "scripts/package-release.py",
):
    ast.parse(Path(name).read_text(encoding="utf-8"), filename=name)
PY
bash -n \
  scripts/check-release-notes.sh \
  scripts/generate-release-sbom.sh \
  scripts/release-local.sh \
  scripts/release-target.sh \
  scripts/release/targets/aarch64-apple-darwin.sh \
  scripts/release/targets/linux-unsupported.sh \
  scripts/test-release-platform.sh \
  scripts/test-release-sbom.sh \
  scripts/verify-release-artifact.sh
scripts/check-release-notes.sh 0.2.0
printf 'release-tooling: portable syntax and notes gates passed\n'
