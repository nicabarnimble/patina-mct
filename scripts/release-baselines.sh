#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
artifact=
output=
while (($#)); do
  case $1 in
    --artifact) artifact=${2:?missing --artifact value}; shift 2 ;;
    --output) output=${2:?missing --output value}; shift 2 ;;
    *) printf 'unknown baselines argument: %s\n' "$1" >&2; exit 2 ;;
  esac
done
[[ -n $artifact && -n $output ]] || {
  printf 'usage: %s --artifact archive --output BASELINES.md\n' "$0" >&2
  exit 2
}
case $artifact in /*) ;; *) artifact="$repo_root/$artifact" ;; esac
case $output in /*) ;; *) output="$repo_root/$output" ;; esac
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || {
  printf 'release baselines require aarch64-apple-darwin\n' >&2; exit 1;
}
[[ -f $artifact && -f $artifact.sha256 && -f $artifact.blake3 ]] || {
  printf 'release baselines require archive and both sidecars\n' >&2; exit 1;
}

uid=$(/usr/bin/id -u)
lock=/tmp/mct-release-smoke-$uid.lock
mkdir "$lock" 2>/dev/null || { printf 'release smoke/baseline label lock is held: %s\n' "$lock" >&2; exit 1; }
work=$(mktemp -d /tmp/mct-release-baselines.XXXXXX)
chmod 700 "$work"
source=$work/source
root=$work/service-root
fixture_root=$work/fixture-service-root
snapshot=$work/production-snapshot
mkdir -m 700 "$root" "$fixture_root" "$snapshot"
production_record=$HOME/.mct/supervisor.json
production_plist=$HOME/Library/LaunchAgents/io.patina.mct.mother.plist
harness=
success=false

snapshot_one() {
  local path=$1 name=$2
  if [[ -e $path ]]; then
    [[ -f $path ]] || { printf 'production path is not a regular file: %s\n' "$path" >&2; return 1; }
    printf 'present\n' >"$snapshot/$name.state"
    cp -p "$path" "$snapshot/$name.bytes"
  else
    printf 'absent\n' >"$snapshot/$name.state"
  fi
}
compare_one() {
  local path=$1 name=$2 state
  state=$(<"$snapshot/$name.state")
  if [[ $state == present ]]; then
    [[ -f $path ]] && cmp -s "$snapshot/$name.bytes" "$path"
  else
    [[ ! -e $path ]]
  fi
}
compare_production() {
  compare_one "$production_record" record && compare_one "$production_plist" plist
}
cleanup() {
  local status=$?
  set +e
  for cleanup_root in "$root" "$fixture_root"; do
    if [[ -n ${harness:-} && -x $harness && -e $cleanup_root/supervisor.json ]]; then
      "$harness" release-smoke-internal stop --root "$cleanup_root" >/dev/null 2>&1
      "$harness" release-smoke-internal uninstall --root "$cleanup_root" >/dev/null 2>&1
    fi
  done
  compare_production
  files_safe=$?
  git -C "$repo_root" worktree remove --force "$source" >/dev/null 2>&1
  rmdir "$lock" >/dev/null 2>&1
  if [[ $status -eq 0 && $files_safe -eq 0 && $success == true ]]; then
    rm -rf "$work"
  else
    printf 'release baseline capture failed; preserved evidence at %s\n' "$work" >&2
    status=1
  fi
  trap - EXIT INT TERM
  exit "$status"
}
trap cleanup EXIT INT TERM

snapshot_one "$production_record" record
snapshot_one "$production_plist" plist

git -C "$repo_root" worktree add --detach "$source" HEAD >/dev/null
[[ -z $(git -C "$source" status --porcelain --untracked-files=all) ]] || {
  printf 'detached baseline worktree is not clean\n' >&2; exit 1;
}
export CARGO_TARGET_DIR=$work/target
cargo build --manifest-path "$source/Cargo.toml" --release --locked \
  -p mct-daemon --bin mct-daemon --example release-digests --example verify-release \
  --features release-smoke-internal
harness=$CARGO_TARGET_DIR/release/mct-daemon
digest_helper=$CARGO_TARGET_DIR/release/examples/release-digests
verifier=$CARGO_TARGET_DIR/release/examples/verify-release
"$verifier" "$artifact" "$work/extracted" aarch64-apple-darwin
release_root=$(find "$work/extracted" -mindepth 1 -maxdepth 1 -type d -print)
[[ -n $release_root && $release_root != *$'\n'* ]] || { printf 'baseline extraction root mismatch\n' >&2; exit 1; }
binary=$release_root/payload/mct-daemon.app/Contents/MacOS/mct-daemon
"$source/scripts/release-target.sh" aarch64-apple-darwin verify "$release_root/payload"
[[ $(jq -er .source_commit "$release_root/RELEASE-MANIFEST.json") == $(git -C "$source" rev-parse HEAD) ]] || {
  printf 'baseline artifact is not from current detached HEAD\n' >&2; exit 1;
}
"$harness" release-smoke-internal preflight --root "$root"
"$harness" release-smoke-internal install --root "$root" --executable "$binary" >/dev/null
python3 "$source/scripts/release-baselines.py" \
  --binary "$binary" --harness "$harness" --digest-helper "$digest_helper" \
  --root "$root" --fixture "$source/crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0" \
  >"$work/baselines.json"

# Measure the complete three-fixture correctness segment separately from the null-sink samples.
"$harness" release-smoke-internal preflight --root "$fixture_root"
"$harness" release-smoke-internal install --root "$fixture_root" --executable "$binary" >/dev/null
"$harness" release-smoke-internal start --root "$fixture_root" >/dev/null
catalog_before=$(du -sk "$fixture_root/children" | awk '{print $1 * 1024}')
state_before=$(stat -f %z "$fixture_root/state.sqlite")
ledger_before=$(stat -f %z "$fixture_root/observations.jsonl")
/usr/bin/time -l python3 "$source/scripts/release-smoke-proof.py" \
  --binary "$binary" --harness "$harness" --digest-helper "$digest_helper" \
  --root "$fixture_root" --fixtures "$source/crates/mct-daemon/tests/fixtures" \
  --primary-archive "$artifact" >"$work/fixture-proof.raw" 2>"$work/fixture-time.txt"
tail -1 "$work/fixture-proof.raw" >"$work/fixture-proof.json"
catalog_after=$(du -sk "$fixture_root/children" | awk '{print $1 * 1024}')
state_after=$(stat -f %z "$fixture_root/state.sqlite")
ledger_after=$(stat -f %z "$fixture_root/observations.jsonl")
"$harness" release-smoke-internal stop --root "$fixture_root" >/dev/null
"$harness" release-smoke-internal uninstall --root "$fixture_root" >/dev/null
"$harness" release-smoke-internal postflight --root "$fixture_root" >/dev/null
compare_production || { printf 'production supervisor files changed during baselines\n' >&2; exit 1; }

archive_sha256=$(cut -d' ' -f1 "$artifact.sha256")
archive_blake3=$(cut -d' ' -f1 "$artifact.blake3")
source_revision=$(jq -er .source_commit "$release_root/RELEASE-MANIFEST.json")
executable_blake3=$(jq -er .executable_blake3 "$release_root/RELEASE-MANIFEST.json")
rust_version=$(jq -er .rust_version "$release_root/RELEASE-MANIFEST.json" | tr '\n' '; ' | sed 's/;$//')
cargo_version=$(jq -er .cargo_version "$release_root/RELEASE-MANIFEST.json")
os_version=$(sw_vers -productVersion)
hardware_model=$(sysctl -n hw.model)
logical_cpus=$(sysctl -n hw.logicalcpu)
memory_bytes=$(sysctl -n hw.memsize)
power_mode=$(pmset -g custom | tr '\n' ' ' | tr -s ' ' | sed 's/^ //; s/ $//')
fixture_wall=$(awk '$2 == "real" {print $1; exit}' "$work/fixture-time.txt")
fixture_user=$(awk '$2 == "user" {print $1; exit}' "$work/fixture-time.txt")
fixture_sys=$(awk '$2 == "sys" {print $1; exit}' "$work/fixture-time.txt")
fixture_peak_rss=$(awk '/maximum resident set size/ {print $1; exit}' "$work/fixture-time.txt")
jq -n \
  --arg source_revision "$source_revision" \
  --arg archive_sha256 "$archive_sha256" \
  --arg archive_blake3 "$archive_blake3" \
  --arg executable_blake3 "$executable_blake3" \
  --arg rust_version "$rust_version" \
  --arg cargo_version "$cargo_version" \
  --arg os_version "$os_version" \
  --arg hardware_model "$hardware_model" \
  --arg logical_cpus "$logical_cpus" \
  --arg memory_bytes "$memory_bytes" \
  --arg power_mode "$power_mode" \
  --arg fixture_wall "$fixture_wall" \
  --arg fixture_user "$fixture_user" \
  --arg fixture_sys "$fixture_sys" \
  --arg fixture_peak_rss "$fixture_peak_rss" \
  --argjson catalog_delta "$((catalog_after - catalog_before))" \
  --argjson state_delta "$((state_after - state_before))" \
  --argjson ledger_delta "$((ledger_after - ledger_before))" \
  --arg artifact "$artifact" \
  --arg output "$output" \
  '$ARGS.named' >"$work/context.json"
python3 "$source/scripts/render-release-baselines.py" \
  --baseline-json "$work/baselines.json" \
  --fixture-json "$work/fixture-proof.json" \
  --context-json "$work/context.json" \
  --output "$output"

success=true
printf 'release-baselines: wrote %s\n' "$output"
