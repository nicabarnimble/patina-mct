#!/usr/bin/env bash
set -euo pipefail
umask 077

if (($# != 4)); then
  echo 'usage: build.sh expected-builder-host source-revision absolute-target-dir absolute-artifacts-dir' >&2
  exit 2
fi
expected_host=$1
revision=$2
export CARGO_TARGET_DIR=$3
artifacts=$4
snapshot_a=${MCT_REPO_REPORT_A_LOCK:-}
snapshot_b=${MCT_REPO_REPORT_B_LOCK:-}
[[ $(hostname) == "$expected_host" ]] || { echo 'not the configured builder' >&2; exit 2; }
[[ $revision =~ ^[a-f0-9]{40}$ ]] || { echo 'full source revision required' >&2; exit 2; }
[[ $CARGO_TARGET_DIR == /* && $artifacts == /* && $artifacts != / ]] || exit 2
[[ ! -e $artifacts ]] || { echo 'artifacts directory must be new' >&2; exit 2; }
[[ -f source.sha256 ]] || { echo 'source.sha256 from the committed source is required' >&2; exit 2; }
for snapshot in "$snapshot_a" "$snapshot_b"; do
  [[ $snapshot == /* && -f $snapshot && ! -L $snapshot ]] || {
    echo 'MCT_REPO_REPORT_A_LOCK and MCT_REPO_REPORT_B_LOCK must name absolute regular files' >&2
    exit 2
  }
done
sha256sum --check source.sha256
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
nix develop .#mother --command cargo build --locked --offline -p mct-daemon --bin mct-daemon --example federation_host
nix develop .#mother --command cargo test --locked --offline -p mct-daemon --example federation_host
child_output="$CARGO_TARGET_DIR/nic105-repo-report"
[[ ! -e $child_output ]] || { echo 'Child output directory must be new' >&2; exit 2; }
nix develop .#mother --command bash scripts/first-real-app/build-child.sh "$child_output"
mkdir -m700 "$artifacts"
install -m555 "$CARGO_TARGET_DIR/debug/mct-daemon" "$artifacts/mct-daemon"
install -m555 "$CARGO_TARGET_DIR/debug/examples/federation_host" "$artifacts/federation_host"
install -m444 "$child_output/mct-repo-report.wasm" "$artifacts/mct-repo-report.wasm"
install -m444 "$child_output/child.toml" "$artifacts/child.toml"
mkdir -m700 "$artifacts/snapshots"
for label in a b; do mkdir -m700 "$artifacts/snapshots/$label"; done
install -m444 "$snapshot_a" "$artifacts/snapshots/a/Cargo.lock"
install -m444 "$snapshot_b" "$artifacts/snapshots/b/Cargo.lock"
component_sha256=$(sha256sum "$artifacts/mct-repo-report.wasm" | cut -d' ' -f1)
snapshot_a_sha256=$(sha256sum "$artifacts/snapshots/a/Cargo.lock" | cut -d' ' -f1)
snapshot_b_sha256=$(sha256sum "$artifacts/snapshots/b/Cargo.lock" | cut -d' ' -f1)
printf '{"source_revision":"%s","profile":"dev-no-debuginfo","purpose":"nic105-dependency-drift-acceptance","repo_report_component_sha256":"%s","snapshot_a_sha256":"%s","snapshot_b_sha256":"%s"}\n' \
  "$revision" "$component_sha256" "$snapshot_a_sha256" "$snapshot_b_sha256" > "$artifacts/provenance.json"
sha256sum \
  "$artifacts/mct-daemon" \
  "$artifacts/federation_host" \
  "$artifacts/mct-repo-report.wasm" \
  "$artifacts/child.toml" \
  "$artifacts/snapshots/a/Cargo.lock" \
  "$artifacts/snapshots/b/Cargo.lock"
nix-build nix/federation-bundle.nix --arg artifacts "$artifacts" --no-out-link
