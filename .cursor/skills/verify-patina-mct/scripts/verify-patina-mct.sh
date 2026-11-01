#!/usr/bin/env bash
set -euo pipefail

skill_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
repo_root=$(cd "$skill_dir/../../.." && pwd)

usage() {
  printf 'usage: %s <launch|doctor|drive-admin|gates|release-artifact|nix-smoke|cleanup>\n' "$0" >&2
  printf 'Set VERIFY_PATINA_MCT_ROOT to an absolute directory outside the home directory.\n' >&2
}

refuse_home() {
  local path=$1
  local home=${HOME%/}
  case "$path" in
    "$home"|"$home"/*)
      printf 'refusing a run directory inside the home directory: %s\n' "$path" >&2
      exit 2
      ;;
  esac
}

run_root() {
  if [[ -z ${VERIFY_PATINA_MCT_ROOT:-} ]]; then
    printf 'VERIFY_PATINA_MCT_ROOT is unset\n' >&2
    exit 2
  fi
  case "$VERIFY_PATINA_MCT_ROOT" in
    /*) ;;
    *)
      printf 'VERIFY_PATINA_MCT_ROOT must be absolute: %s\n' "$VERIFY_PATINA_MCT_ROOT" >&2
      exit 2
      ;;
  esac
  refuse_home "$VERIFY_PATINA_MCT_ROOT"
  printf '%s\n' "$VERIFY_PATINA_MCT_ROOT"
}

ensure_layout() {
  local root
  root=$(run_root)
  umask 077
  mkdir -p "$root/scratch/mother" "$root/proof"
  chmod 700 "$root" "$root/scratch" "$root/scratch/mother" "$root/proof"
}

binary_path() {
  if [[ -n ${VERIFY_PATINA_MCT_BIN:-} ]]; then
    printf '%s\n' "$VERIFY_PATINA_MCT_BIN"
    return
  fi
  printf '%s\n' "$repo_root/target/debug/mct-daemon"
}

record() {
  local file=$1
  shift
  printf '%s\n' "$*" >>"$file"
}

launch() {
  local root bin
  root=$(run_root)
  ensure_layout
  cd "$repo_root"
  cargo build -p mct-daemon --bin mct-daemon
  bin=$(binary_path)
  [[ -x $bin ]]
  printf '%s\n' "$bin" >"$root/proof/binary-path.txt"
  printf 'launch: built %s\n' "$bin"
}

doctor() {
  local root bin version_line
  root=$(run_root)
  ensure_layout
  bin=$(binary_path)
  [[ -x $bin ]]
  refuse_home "$bin"
  version_line=$("$bin" version)
  printf '%s\n' "$version_line" >"$root/proof/version.txt"
  [[ $version_line == "mct-daemon 0.2.0" ]]
  printf 'doctor: %s at %s\n' "$version_line" "$bin"
}

assert_json_shape() {
  python3 - "$1" "$2" <<'PY'
import json, sys
path, kind = sys.argv[1], sys.argv[2]
value = json.load(open(path))
if kind == "day-key":
    if set(value) != {"scheme", "public_key"}:
        raise SystemExit("day-key public half has unexpected keys")
    if value["scheme"] != "ed25519" or len(value["public_key"]) != 64:
        raise SystemExit("day-key public half is not a 64-hex ed25519 key")
    raise SystemExit(0)
if kind == "genesis":
    action = value["intent"]["action"]
    if value["intent"]["mother_node_id"] != "mother-verify":
        raise SystemExit("prepare-genesis mother id mismatch")
    if value["intent"]["ledger_id"] != "ledger-verify":
        raise SystemExit("prepare-genesis ledger id mismatch")
    if action.get("action") != "bootstrap" or "challenge" not in value:
        raise SystemExit("prepare-genesis did not return a bootstrap challenge")
    raise SystemExit(0)
raise SystemExit("unknown shape")
PY
}

drive_admin() {
  local root bin secret pin proof status
  root=$(run_root)
  rm -rf "$root/scratch"
  ensure_layout
  launch
  doctor
  bin=$(binary_path)
  proof=$root/proof
  secret=$root/scratch/day-key.json
  pin=$root/scratch/pin.json
  : >"$proof/drive-results.tsv"

  "$bin" help >"$proof/help.txt"
  grep -q 'passkey-admin' "$proof/help.txt"
  record "$proof/drive-results.tsv" $'help\tauto\t0\toperator help lists passkey-admin'

  "$bin" passkey-admin day-keygen --secret-output "$secret" >"$proof/day-keygen.stdout"
  mode=$(stat -c '%a' "$secret")
  [[ $mode == 600 ]]
  assert_json_shape "$proof/day-keygen.stdout" day-key
  record "$proof/drive-results.tsv" $'day-keygen\tauto\t0\twrites a mode 0600 secret and prints only the public half'

  set +e
  "$bin" passkey-admin day-keygen --secret-output "$secret" >"$proof/day-keygen-replace.stdout" 2>"$proof/day-keygen-replace.stderr"
  status=$?
  set -e
  [[ $status -ne 0 ]]
  grep -q 'refusing to replace an existing day-key secret' "$proof/day-keygen-replace.stderr"
  record "$proof/drive-results.tsv" "day-keygen-replace	auto	$status	refuses to replace an existing secret"

  python3 - "$repo_root/scripts/fixtures/passkey-admin-challenge.json" "$pin" <<'PY'
import json, sys
src, dest = sys.argv[1], sys.argv[2]
root = json.load(open(src))["root"]
with open(dest, "w") as handle:
    json.dump(root, handle)
PY
  chmod 600 "$pin"
  "$bin" passkey-admin prepare-genesis \
    --root "$root/scratch/mother" \
    --mother mother-verify \
    --ledger-id ledger-verify \
    --registered-root "$pin" >"$proof/prepare-genesis.json"
  assert_json_shape "$proof/prepare-genesis.json" genesis
  record "$proof/drive-results.tsv" $'prepare-genesis\tauto\t0\temits a bootstrap challenge and does not write a ledger'

  if [[ -e $root/scratch/mother/observations.jsonl || -e $root/scratch/mother/ledger.jsonl ]]; then
    printf 'prepare-genesis wrote ledger state\n' >&2
    exit 1
  fi

  set +e
  "$bin" install >"$proof/install.stdout" 2>"$proof/install.stderr"
  status=$?
  set -e
  [[ $status -ne 0 ]]
  grep -q 'supervisor lifecycle is supported only on macOS' "$proof/install.stderr"
  record "$proof/drive-results.tsv" "install	auto	$status	macOS launchd lifecycle refuses on this host"

  set +e
  "$bin" status --uds "$root/scratch/absent.sock" --json >"$proof/status.stdout" 2>"$proof/status.stderr"
  status=$?
  set -e
  [[ $status -ne 0 ]]
  grep -q 'resident not running at verified control socket' "$proof/status.stderr"
  record "$proof/drive-results.tsv" "status-absent	auto	$status	missing socket fails closed"

  set +e
  "$bin" not-a-command >"$proof/unknown.stdout" 2>"$proof/unknown.stderr"
  status=$?
  set -e
  [[ $status -ne 0 ]]
  grep -q "unknown command 'not-a-command'" "$proof/unknown.stderr"
  record "$proof/drive-results.tsv" "unknown-command	auto	$status	unknown command fails closed"

  rm -f "$secret" "$pin"
  printf 'drive-admin: proof at %s\n' "$proof"
}

run_one() {
  local name=$1
  local log=$2
  shift 2
  local status
  set +e
  "$@" >"$log" 2>&1
  status=$?
  set -e
  printf '%s\t%s\n' "$name" "$status"
  return 0
}

gates() {
  local root proof log_dir failed name status
  root=$(run_root)
  ensure_layout
  proof=$root/proof
  log_dir=$proof/gates
  mkdir -p "$log_dir"
  : >"$proof/gates-results.tsv"
  failed=0
  cd "$repo_root"

  while IFS=$'\t' read -r name status; do
    printf '%s\tauto\t%s\n' "$name" "$status" >>"$proof/gates-results.tsv"
    printf 'gates: %s exit %s\n' "$name" "$status"
    if [[ $status -ne 0 ]]; then
      failed=1
    fi
  done < <(
    run_one fmt "$log_dir/fmt.log" cargo fmt --all --check
    run_one clippy "$log_dir/clippy.log" cargo clippy -- -D warnings
    run_one test "$log_dir/test.log" cargo test -- --test-threads=1
    run_one ci-tier0-specs "$log_dir/ci-tier0-specs.log" bash scripts/ci-tier0.sh specs
    run_one git-diff-check "$log_dir/git-diff-check.log" git diff --check
  )
  printf 'gates: logs at %s\n' "$log_dir"
  return "$failed"
}

release_artifact() {
  local root proof archive status
  root=$(run_root)
  ensure_layout
  proof=$root/proof
  cd "$repo_root"
  set +e
  bash scripts/verify-release-artifact.sh >"$proof/release-usage.stdout" 2>"$proof/release-usage.stderr"
  status=$?
  set -e
  printf 'release-usage\tauto\t%s\n' "$status" >"$proof/release-results.tsv"
  [[ $status -eq 2 ]]
  grep -q 'usage:' "$proof/release-usage.stderr"

  archive=$root/scratch/not-a-release.tar.gz
  printf 'not an archive\n' >"$archive"
  set +e
  bash scripts/verify-release-artifact.sh "$archive" >"$proof/release-basename.stdout" 2>"$proof/release-basename.stderr"
  status=$?
  set -e
  printf 'release-basename\tauto\t%s\n' "$status" >>"$proof/release-results.tsv"
  [[ $status -eq 1 ]]
  grep -q 'unsupported R3 target basename' "$proof/release-basename.stderr"

  archive=$root/scratch/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz
  printf 'not an archive\n' >"$archive"
  set +e
  bash scripts/verify-release-artifact.sh "$archive" >"$proof/release-bogus.stdout" 2>"$proof/release-bogus.stderr"
  status=$?
  set -e
  printf 'release-bogus-archive\tauto\t%s\n' "$status" >>"$proof/release-results.tsv"
  [[ $status -ne 0 ]]
  printf 'release-artifact: results at %s\n' "$proof/release-results.tsv"
}

nix_smoke() {
  local root proof status
  root=$(run_root)
  ensure_layout
  proof=$root/proof
  cd "$repo_root"
  {
    printf 'uname -s: %s\n' "$(uname -s)"
    printf 'uname -m: %s\n' "$(uname -m)"
    if command -v nix >/dev/null 2>&1; then
      printf 'nix: %s\n' "$(command -v nix)"
    else
      printf 'nix: absent\n'
    fi
    printf 'flake system: aarch64-linux\n'
  } >"$proof/nix-host.txt"
  set +e
  nix run .#mother-install-smoke >"$proof/nix-smoke.stdout" 2>"$proof/nix-smoke.stderr"
  status=$?
  set -e
  printf 'nix-smoke\tout-of-vm\t%s\n' "$status" >"$proof/nix-results.tsv"
  printf 'nix-smoke: exit %s\n' "$status"
  return "$status"
}

cleanup() {
  local root
  root=$(run_root)
  refuse_home "$root"
  if [[ ! -d $root/proof ]]; then
    printf 'cleanup refuses to run without proof at %s\n' "$root/proof" >&2
    exit 1
  fi
  rm -rf "$root/scratch"
  if [[ ! -d $root/proof ]]; then
    printf 'cleanup removed proof at %s\n' "$root/proof" >&2
    exit 1
  fi
  printf 'cleanup: scratch removed, proof remains at %s\n' "$root/proof"
}

cmd=${1:-}
case "$cmd" in
  launch) launch ;;
  doctor) doctor ;;
  drive-admin) drive_admin ;;
  gates) gates ;;
  release-artifact) release_artifact ;;
  nix-smoke) nix_smoke ;;
  cleanup) cleanup ;;
  *)
    usage
    exit 2
    ;;
esac
