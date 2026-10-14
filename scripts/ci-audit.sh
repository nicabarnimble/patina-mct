#!/usr/bin/env bash
# Run cargo audit under .cargo/audit.toml.
#
#   scripts/ci-audit.sh strict
#   scripts/ci-audit.sh changed --event pull_request <base> <head>
#   scripts/ci-audit.sh changed --event push <base> <head>
#
# `changed` fails only when the diff touches Cargo.lock or a Cargo.toml.
# Otherwise it prints the findings, emits a GitHub warning annotation, and
# exits 0. A missing cargo-audit binary still fails.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  printf 'usage: %s strict | changed --event pull_request|push <base> <head>\n' "$0" >&2
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
  if printf '%s\n' "$names" | grep -Eq '(^|/)Cargo\.(toml|lock)$'; then
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
