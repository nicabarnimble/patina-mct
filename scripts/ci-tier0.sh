#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/check-comparative-vocabulary.sh
allium check layer/allium
