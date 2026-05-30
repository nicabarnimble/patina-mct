#!/usr/bin/env bash
set -euo pipefail

git config core.hooksPath .githooks
echo "Installed git hooks path: .githooks"
