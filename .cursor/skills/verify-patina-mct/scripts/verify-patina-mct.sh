#!/usr/bin/env bash
set -euo pipefail

skill_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
repo_root=$(cd "$skill_dir/../../.." && pwd)

usage() {
  printf 'usage: %s <launch|doctor|drive-admin|gates|release-artifact|nix-smoke|cleanup>\n' "$0" >&2
  printf 'Set VERIFY_PATINA_MCT_ROOT to an absolute directory outside the home directory.\n' >&2
}

run_root() {
  node "$skill_dir/scripts/run-directory.mjs" path
}

ensure_layout() {
  umask 077
  node "$skill_dir/scripts/run-directory.mjs" prepare >/dev/null
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
  printf '%s\t%s\t%s\t%s\n' "$@" >>"$file"
}

launch() {
  local root bin
  root=$(run_root)
  ensure_layout
  cd "$repo_root"
  cargo build -p mct-daemon --bin mct-daemon
  bin=$(binary_path)
  [[ -x $bin ]] || return 1
  printf '%s\n' "$bin" >"$root/proof/binary-path.txt"
  printf 'launch: built %s\n' "$bin"
}

doctor() {
  local root bin version_line
  root=$(run_root)
  ensure_layout
  bin=$(binary_path)
  [[ -x $bin ]] || return 1
  version_line=$("$bin" version)
  printf '%s\n' "$version_line" >"$root/proof/version.txt"
  [[ $version_line == "mct-daemon 0.2.0" ]] || return 1
  printf 'doctor: %s at %s\n' "$version_line" "$bin"
}

assert_json_shape() {
  node - "$1" "$2" <<'JS'
const fs = require('node:fs');
const assert = require('node:assert/strict');
const [file, kind] = process.argv.slice(2);
const value = JSON.parse(fs.readFileSync(file, 'utf8'));
if (kind === 'day-key') {
  assert.deepEqual(Object.keys(value).sort(), ['public_key', 'scheme']);
  assert.equal(value.scheme, 'ed25519');
  assert.match(value.public_key, /^[0-9a-f]{64}$/);
} else if (kind === 'genesis') {
  assert.equal(value.intent.mother_node_id, 'mother-verify');
  assert.equal(value.intent.ledger_id, 'ledger-verify');
  assert.equal(value.intent.action.action, 'bootstrap');
  assert.ok(value.challenge);
} else {
  throw new Error('unknown shape');
}
JS
}

drive_admin() {
  local root bin secret pin proof status mode system
  root=$(node "$skill_dir/scripts/run-directory.mjs" reset)
  system=$(uname -s)
  [[ -n $system ]] || return 1
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
  record "$proof/drive-results.tsv" help auto 0 'operator help lists passkey-admin'

  "$bin" passkey-admin day-keygen --secret-output "$secret" >"$proof/day-keygen.stdout"
  mode=$(node -e 'console.log((require("node:fs").statSync(process.argv[1]).mode & 0o777).toString(8))' "$secret")
  [[ $mode == 600 ]] || return 1
  assert_json_shape "$proof/day-keygen.stdout" day-key
  record "$proof/drive-results.tsv" day-keygen auto 0 'writes a mode 0600 secret and prints only the public half'

  set +e
  "$bin" passkey-admin day-keygen --secret-output "$secret" >"$proof/day-keygen-replace.stdout" 2>"$proof/day-keygen-replace.stderr"
  status=$?
  set -e
  [[ $status -ne 0 ]] || return 1
  grep -q 'refusing to replace an existing day-key secret' "$proof/day-keygen-replace.stderr"
  record "$proof/drive-results.tsv" day-keygen-replace auto "$status" 'refuses to replace an existing secret'

  node - "$repo_root/scripts/fixtures/passkey-admin-challenge.json" "$pin" <<'JS'
const fs = require('node:fs');
const [source, destination] = process.argv.slice(2);
const { root } = JSON.parse(fs.readFileSync(source, 'utf8'));
fs.writeFileSync(destination, JSON.stringify(root), { flag: 'wx', mode: 0o600 });
JS
  chmod 600 "$pin"
  "$bin" passkey-admin prepare-genesis \
    --root "$root/scratch/mother" \
    --mother mother-verify \
    --ledger-id ledger-verify \
    --registered-root "$pin" >"$proof/prepare-genesis.json"
  assert_json_shape "$proof/prepare-genesis.json" genesis
  record "$proof/drive-results.tsv" prepare-genesis auto 0 'emits a bootstrap challenge and does not write a ledger'

  if [[ -e $root/scratch/mother/observations.jsonl || -e $root/scratch/mother/ledger.jsonl ]]; then
    printf 'prepare-genesis wrote ledger state\n' >&2
    exit 1
  fi

  : >"$proof/install.stdout"
  : >"$proof/install.stderr"
  if [[ $system == Darwin ]]; then
    record "$proof/drive-results.tsv" install manual not-run 'macOS launchd lifecycle requires a separate operator-controlled smoke'
  else
    set +e
    "$bin" install >"$proof/install.stdout" 2>"$proof/install.stderr"
    status=$?
    set -e
    [[ $status -ne 0 ]] || return 1
    grep -q 'supervisor lifecycle is supported only on macOS' "$proof/install.stderr"
    record "$proof/drive-results.tsv" install auto "$status" 'macOS launchd lifecycle refuses on this host'
  fi

  set +e
  "$bin" status --uds "$root/scratch/absent.sock" --json >"$proof/status.stdout" 2>"$proof/status.stderr"
  status=$?
  set -e
  [[ $status -ne 0 ]] || return 1
  grep -q 'resident not running at verified control socket' "$proof/status.stderr"
  record "$proof/drive-results.tsv" status-absent auto "$status" 'missing socket fails closed'

  set +e
  "$bin" not-a-command >"$proof/unknown.stdout" 2>"$proof/unknown.stderr"
  status=$?
  set -e
  [[ $status -ne 0 ]] || return 1
  grep -q "unknown command 'not-a-command'" "$proof/unknown.stderr"
  record "$proof/drive-results.tsv" unknown-command auto "$status" 'unknown command fails closed'

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
    run_one test "$log_dir/test.log" bash scripts/ci-tier0.sh test
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
  [[ $status -eq 2 ]] || return 1
  grep -q 'usage:' "$proof/release-usage.stderr"

  archive=$root/scratch/not-a-release.tar.gz
  printf 'not an archive\n' >"$archive"
  set +e
  bash scripts/verify-release-artifact.sh "$archive" >"$proof/release-basename.stdout" 2>"$proof/release-basename.stderr"
  status=$?
  set -e
  printf 'release-basename\tauto\t%s\n' "$status" >>"$proof/release-results.tsv"
  [[ $status -eq 1 ]] || return 1
  grep -q 'unsupported R3 target basename' "$proof/release-basename.stderr"

  archive=$root/scratch/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz
  printf 'not an archive\n' >"$archive"
  set +e
  bash scripts/verify-release-artifact.sh "$archive" >"$proof/release-bogus.stdout" 2>"$proof/release-bogus.stderr"
  status=$?
  set -e
  printf 'release-bogus-archive\tauto\t%s\n' "$status" >>"$proof/release-results.tsv"
  [[ $status -ne 0 ]] || return 1
  printf 'release-artifact: results at %s\n' "$proof/release-results.tsv"
}

nix_smoke() {
  local root proof status classification system architecture
  root=$(run_root)
  ensure_layout
  proof=$root/proof
  cd "$repo_root"
  system=$(uname -s)
  architecture=$(uname -m)
  {
    printf 'uname -s: %s\n' "$system"
    printf 'uname -m: %s\n' "$architecture"
    if command -v nix >/dev/null 2>&1; then
      printf 'nix: %s\n' "$(command -v nix)"
    else
      printf 'nix: absent\n'
    fi
    printf 'flake system: aarch64-linux\n'
  } >"$proof/nix-host.txt"
  : >"$proof/nix-smoke.stdout"
  : >"$proof/nix-smoke.stderr"
  classification=out-of-vm
  if [[ $system != Linux || $architecture != aarch64 ]]; then
    printf 'not run: requires aarch64-linux\n' >"$proof/nix-smoke.stderr"
    status=2
  elif ! command -v nix >/dev/null 2>&1; then
    printf 'not run: nix is absent\n' >"$proof/nix-smoke.stderr"
    status=127
  else
    classification=auto
    set +e
    nix run .#mother-install-smoke >"$proof/nix-smoke.stdout" 2>"$proof/nix-smoke.stderr"
    status=$?
    set -e
  fi
  printf 'nix-smoke\t%s\t%s\n' "$classification" "$status" >"$proof/nix-results.tsv"
  printf 'nix-smoke: exit %s\n' "$status"
  return "$status"
}

cleanup() {
  local root
  root=$(node "$skill_dir/scripts/run-directory.mjs" cleanup)
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
