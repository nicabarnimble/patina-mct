#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
git config --local core.hooksPath .githooks
[[ $(git config --local --get core.hooksPath) == .githooks ]]
printf 'git-hooks: core.hooksPath=.githooks\n'
