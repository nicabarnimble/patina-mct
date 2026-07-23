#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
bash scripts/check-release-version.sh
bash scripts/check-release-tooling.sh
cargo audit
cargo clippy --workspace --all-targets -- -D warnings
# Resident integration tests own real UDS and Iroh endpoint lifecycles. Serialize the
# Tier-0 harness so constrained CI runners do not make those independent lifecycles
# contend with one another; product-level concurrency remains covered inside tests.
cargo test --workspace -- --test-threads=1
bash scripts/check-comparative-vocabulary.sh
allium check layer/allium
