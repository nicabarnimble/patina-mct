#!/usr/bin/env bash
set -euo pipefail

skill_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bash -n "$skill_dir/scripts/verify-patina-mct.sh"
cd "$skill_dir/../../.."
node --test .cursor/skills/verify-patina-mct/scripts/check-skill.test.mjs
