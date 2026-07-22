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
rust_version=$(jq -er .rust_version "$release_root/RELEASE-MANIFEST.json")
cargo_version=$(jq -er .cargo_version "$release_root/RELEASE-MANIFEST.json")
os_version=$(sw_vers -productVersion)
hardware_model=$(sysctl -n hw.model)
logical_cpus=$(sysctl -n hw.logicalcpu)
memory_bytes=$(sysctl -n hw.memsize)
power_mode=$(pmset -g custom | tr '\n' ' ' | tr -s ' ' | sed 's/^ //; s/ $//')
fixture_wall=$(awk '/ real / {print $1; exit}' "$work/fixture-time.txt")
fixture_user=$(awk '/ user / {print $1; exit}' "$work/fixture-time.txt")
fixture_sys=$(awk '/ sys / {print $1; exit}' "$work/fixture-time.txt")
fixture_peak_rss=$(awk '/maximum resident set size/ {print $1; exit}' "$work/fixture-time.txt")
mkdir -p "$(dirname "$output")"
python3 - "$work/baselines.json" "$work/fixture-proof.json" "$output" <<PY
import json, sys
baseline = json.load(open(sys.argv[1]))
fixture = json.load(open(sys.argv[2]))
out = sys.argv[3]
def values(items): return ", ".join(f"{value:.3f}" if isinstance(value, float) else str(value) for value in items)
text = f'''# MCT 0.2.0 performance baselines — aarch64-apple-darwin

These values are release evidence, not SLOs or admission thresholds.

## Artifact and host

- Source revision: `$source_revision`
- Archive SHA-256: `sha256:$archive_sha256`
- Archive BLAKE3: `blake3:$archive_blake3`
- Executable BLAKE3: `$executable_blake3`
- Rust: `$rust_version`
- Cargo: `$cargo_version`
- macOS: `$os_version`; architecture: `arm64`
- Hardware model: `$hardware_model`; logical CPUs: `$logical_cpus`; memory bytes: `$memory_bytes`
- Power configuration: `$power_mode`

## Startup

Method: five real fixed-label launchd `start` requests through the internal D1.18 plist seam, each awaiting owner-authenticated readiness, followed by clean `stop`.

- Samples (ms): {values(baseline['startup_ms'])}
- Min/median/max (ms): {baseline['startup_min_ms']:.3f} / {baseline['startup_median_ms']:.3f} / {baseline['startup_max_ms']:.3f}

## Idle RSS

Method: 60 seconds ready and idle, then seven RSS samples ten seconds apart from the launchd-supervised PID.

- Samples (bytes): {values(baseline['idle_rss_bytes'])}
- Min/median/max (bytes): {baseline['idle_rss_min_bytes']} / {baseline['idle_rss_median_bytes']} / {baseline['idle_rss_max_bytes']}

## Owner-authenticated UDS calls

Payload: exact approved `watch-null-sink@0.1.0` `patina:watch/events@0.1.0.emit` call with public inline legacy file-change data.

- Sequential: {baseline['uds_latency_warmups']} warmups; {baseline['uds_latency_samples']} measured; {baseline['uds_latency_successes']} successes
- p50/p95/p99/max (µs): {baseline['uds_latency_p50_us']:.3f} / {baseline['uds_latency_p95_us']:.3f} / {baseline['uds_latency_p99_us']:.3f} / {baseline['uds_latency_max_us']:.3f}
- Throughput: {baseline['throughput_clients']} clients × {baseline['throughput_calls_per_client']} calls in {baseline['throughput_seconds']:.3f}s = {baseline['throughput_calls_per_second']:.3f} calls/s; failures={baseline['throughput_failures']}
- Throughput resident CPU seconds: {baseline['throughput_cpu_seconds']:.3f}; peak RSS bytes: {baseline['throughput_peak_rss_bytes']}

## Trigger-turn load

Method: one production scheduler recovery range of 4,097 occurrences under `fire_late_bounded`, yielding 31 admitted candidates plus one terminal record representing every excess refusal.

- Turn wall/CPU: {baseline['trigger_turn_ms']:.3f} ms / {baseline['trigger_turn_cpu_seconds']:.3f} s
- Admitted candidates: {baseline['trigger_turn_admitted']}; terminally represented refusals: {baseline['trigger_turn_terminal_refusals']}
- Concurrent ordinary owner-authenticated status latency: {baseline['trigger_turn_status_ms']:.3f} ms
- Ledger bytes after turn: {baseline['trigger_turn_ledger_bytes_after']}

## Complete three-fixture resources

Method: `/usr/bin/time -l python3 scripts/release-smoke-proof.py ...` over copied Slate, folder-watch actor, and null-sink fixtures, including exact approval/grants, Watch call-out, temporal trigger, revocation, and clean restart.

- Wall/user/system seconds: `$fixture_wall` / `$fixture_user` / `$fixture_sys`
- Peak RSS bytes: `$fixture_peak_rss`
- Catalog bytes delta: {int('$catalog_after') - int('$catalog_before')}
- State bytes delta: {int('$state_after') - int('$state_before')}
- Ledger bytes delta: {int('$ledger_after') - int('$ledger_before')}
- Terminal outcomes: fixture acquisitions={fixture['fixture_acquisitions']}; Watch deliveries={fixture['watch_event_deliveries']}; revocation survived restart={str(fixture['revocation_survived_restart']).lower()}

## Reproduction

```text
scripts/release-local.sh baselines --artifact $artifact --output $output
```

The harness refuses an occupied fixed MCT launchd label, uses no network acquisition, snapshots and post-compares production supervisor files byte-for-byte, and exposes no alternate plist or label selector in the distributed CLI.
'''
open(out, 'w').write(text)
PY

success=true
printf 'release-baselines: wrote %s\n' "$output"
