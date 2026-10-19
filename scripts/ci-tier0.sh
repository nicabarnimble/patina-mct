#!/usr/bin/env bash
# Tier 0 checks. One subcommand is one CI job. `all` runs every check and
# reports each result; a failure does not skip the remaining checks.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  printf 'usage: %s [fmt|clippy|test|specs|audit|all]\n' "$0" >&2
}

run_fmt() {
  cargo fmt --check
}

run_clippy() {
  cargo clippy --workspace --all-targets -- -D warnings
}

run_test() {
  # Resident integration tests own real UDS and Iroh endpoint lifecycles. Serialize the
  # Tier-0 harness so constrained CI runners do not make those independent lifecycles
  # contend with one another; product-level concurrency remains covered inside tests.
  cargo test --workspace -- --test-threads=1
}

run_audit() {
  cargo audit
}

run_release() {
  bash scripts/check-release-version.sh
  bash scripts/check-release-tooling.sh
}

run_vocabulary() {
  bash scripts/check-comparative-vocabulary.sh
}

run_specs() {
  local status=0
  run_release || status=1
  run_vocabulary || status=1
  return "$status"
}

run_named() {
  case "$1" in
    fmt) run_fmt ;;
    clippy) run_clippy ;;
    test) run_test ;;
    specs) run_specs ;;
    audit) run_audit ;;
    *)
      usage
      return 2
      ;;
  esac
}

if (($# > 1)); then
  usage
  exit 2
fi

cmd="${1:-all}"
if [[ "$cmd" != all ]]; then
  run_named "$cmd"
  exit 0
fi

status=0
for name in fmt specs clippy test audit; do
  printf '=== ci-tier0: %s ===\n' "$name"
  if ! run_named "$name"; then
    printf 'ci-tier0: %s failed\n' "$name" >&2
    status=1
  fi
done
exit "$status"
