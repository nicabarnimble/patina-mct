#!/usr/bin/env bash
# Run cargo audit under .cargo/audit.toml.
#
#   scripts/ci-audit.sh strict
#   scripts/ci-audit.sh changed --event pull_request <base> <head>
#   scripts/ci-audit.sh changed --event push <base> <head>
#   scripts/ci-audit.sh --self-test
#
# `changed` fails when cargo audit fails and the diff touches Cargo.lock or a
# Cargo.toml. A diff that does not touch those files may warn and exit 0.
# Any other audit failure, including a missing base commit, fails the job.
# `strict` always returns cargo audit's status. A missing cargo-audit binary
# still fails.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  printf 'usage: %s strict | changed --event pull_request|push <base> <head> | --self-test\n' "$0" >&2
}

# Match manifest paths without a pipe. `grep -q` in a pipeline under pipefail
# exits 141 when it finds an early match, which used to look like "no Cargo
# files changed" and turn every audit failure into a warning.
list_has_cargo_manifest() {
  local line
  while IFS= read -r line || [[ -n "$line" ]]; do
    case "$line" in
      Cargo.toml | Cargo.lock | */Cargo.toml | */Cargo.lock) return 0 ;;
    esac
  done <<< "$1"
  return 1
}

deps_changed() {
  local event="$1" base="$2" head="$3" names
  if [[ -z "$base" || "$base" =~ ^0+$ ]]; then
    printf 'audit gate: base commit is missing; treating dependency files as changed\n' >&2
    return 0
  fi
  if [[ "$event" == pull_request ]]; then
    names=$(git diff --name-only "${base}...${head}") || {
      printf 'audit gate: git diff failed; treating dependency files as changed\n' >&2
      return 0
    }
  else
    names=$(git diff --name-only "$base" "$head") || {
      printf 'audit gate: git diff failed; treating dependency files as changed\n' >&2
      return 0
    }
  fi
  if list_has_cargo_manifest "$names"; then
    return 0
  fi
  return 1
}

annotate_warning() {
  local message="$1"
  printf '%s\n' "$message"
  if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
    printf '::warning title=dependency advisories::%s\n' "$message"
  fi
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    {
      printf '### dependency advisories\n\n'
      printf '%s\n\n' "$message"
      printf '```\n'
      cat
      printf '```\n'
    } >>"$GITHUB_STEP_SUMMARY"
  fi
}

run_audit() {
  local log status
  log=$(mktemp)
  set +e
  cargo audit >"$log" 2>&1
  status=$?
  set -e
  cat "$log"
  printf '%s\n' "$status" >"${log}.status"
  AUDIT_LOG=$log
}

self_test() {
  local names line head parent output status cargo_commit
  names="Cargo.toml"
  for line in $(seq 1 4000); do
    names+=$'\n'"src/file${line}.rs"
  done
  if ! list_has_cargo_manifest "$names"; then
    printf 'self-test: missed Cargo.toml at the head of a long diff\n' >&2
    exit 1
  fi
  names=$'src/lib.rs\ncrates/mct-daemon/src/wasm.rs'
  if list_has_cargo_manifest "$names"; then
    printf 'self-test: plain sources were treated as dependency files\n' >&2
    exit 1
  fi
  names=$'fuzz/Cargo.lock\ncrates/mct-daemon/Cargo.toml\nCargo.toml.bak\nnot-Cargo.toml'
  if ! list_has_cargo_manifest "$names"; then
    printf 'self-test: missed a nested Cargo.lock or Cargo.toml\n' >&2
    exit 1
  fi

  head=$(git rev-parse HEAD)
  if deps_changed push "$head" "$head"; then
    printf 'self-test: an empty diff was treated as a dependency change\n' >&2
    exit 1
  fi
  cargo_commit=$(git log -1 --format=%H -- Cargo.toml)
  parent=$(git rev-parse "${cargo_commit}^")
  if ! deps_changed pull_request "$parent" "$cargo_commit"; then
    printf 'self-test: a Cargo.toml commit was not detected\n' >&2
    exit 1
  fi
  if ! deps_changed push "$parent" "$cargo_commit"; then
    printf 'self-test: a Cargo.toml push range was not detected\n' >&2
    exit 1
  fi

  local stubdir
  stubdir=$(mktemp -d)
  cat >"${stubdir}/cargo" <<'EOF'
#!/bin/bash
if [[ "$1" == audit && "${2:-}" == --version ]]; then
  printf 'cargo-audit 0.22.2\n'
  exit 0
fi
if [[ "$1" == audit ]]; then
  printf 'error: stub audit failed\n'
  exit "${AUDIT_STUB_STATUS:-1}"
fi
printf 'unexpected cargo %s\n' "$*" >&2
exit 9
EOF
  chmod +x "${stubdir}/cargo"

  set +e
  output=$(PATH="${stubdir}:${PATH}" AUDIT_STUB_STATUS=1 bash "$0" changed --event push "$head" "$head" 2>&1)
  status=$?
  set -e
  if [[ "$status" -ne 0 || "$output" != *"does not modify Cargo.lock"* ]]; then
    printf 'self-test: unchanged diff should warn and exit 0 (status %s)\n%s\n' "$status" "$output" >&2
    exit 1
  fi

  set +e
  output=$(PATH="${stubdir}:${PATH}" AUDIT_STUB_STATUS=7 bash "$0" changed --event pull_request "$parent" "$cargo_commit" 2>&1)
  status=$?
  set -e
  if [[ "$status" -ne 7 || "$output" == *"is not failing"* || "$output" != *"touches Cargo.lock"* ]]; then
    printf 'self-test: dependency diff must keep audit status 7 (status %s)\n%s\n' "$status" "$output" >&2
    exit 1
  fi

  set +e
  output=$(PATH="${stubdir}:${PATH}" AUDIT_STUB_STATUS=1 bash "$0" changed --event push 0000000000000000000000000000000000000000 "$head" 2>&1)
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf 'self-test: a missing base commit must fail closed\n%s\n' "$output" >&2
    exit 1
  fi

  set +e
  PATH="${stubdir}:${PATH}" AUDIT_STUB_STATUS=4 bash "$0" strict >/dev/null
  status=$?
  set -e
  if [[ "$status" -ne 4 ]]; then
    printf 'self-test: strict mode returned %s, expected 4\n' "$status" >&2
    exit 1
  fi

  set +e
  PATH="${stubdir}:${PATH}" AUDIT_STUB_STATUS=0 bash "$0" strict >/dev/null
  status=$?
  set -e
  if [[ "$status" -ne 0 ]]; then
    printf 'self-test: a clean strict audit returned %s\n' "$status" >&2
    exit 1
  fi

  rm -rf "$stubdir"
  printf 'ci-audit self-test passed\n'
}

if [[ "${1:-}" == --self-test ]]; then
  self_test
  exit 0
fi

if (($# < 1)); then
  usage
  exit 2
fi

mode=$1
shift

if ! cargo audit --version >/dev/null 2>&1; then
  printf 'cargo audit is not installed\n' >&2
  exit 1
fi

case "$mode" in
  strict)
    if (($# != 0)); then
      usage
      exit 2
    fi
    run_audit
    status=$(cat "${AUDIT_LOG}.status")
    rm -f "$AUDIT_LOG" "${AUDIT_LOG}.status"
    exit "$status"
    ;;
  changed)
    event=""
    if [[ "${1:-}" == --event ]]; then
      event="${2:-}"
      shift 2
    fi
    if [[ "$event" != pull_request && "$event" != push ]]; then
      usage
      exit 2
    fi
    if (($# != 2)); then
      usage
      exit 2
    fi
    base=$1
    head=$2
    run_audit
    status=$(cat "${AUDIT_LOG}.status")
    if [[ "$status" == 0 ]]; then
      rm -f "$AUDIT_LOG" "${AUDIT_LOG}.status"
      exit 0
    fi
    if deps_changed "$event" "$base" "$head"; then
      printf 'cargo audit failed and this change touches Cargo.lock or a Cargo.toml\n' >&2
      rm -f "$AUDIT_LOG" "${AUDIT_LOG}.status"
      exit "$status"
    fi
    message="cargo audit reported findings, but this change does not modify Cargo.lock or a Cargo.toml. The audit job is not failing. The nightly audit on main keeps the R2 policy in .cargo/audit.toml."
    annotate_warning "$message" <"$AUDIT_LOG"
    rm -f "$AUDIT_LOG" "${AUDIT_LOG}.status"
    exit 0
    ;;
  *)
    usage
    exit 2
    ;;
esac
