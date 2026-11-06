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
repo_report_component=${MCT_REPO_REPORT_COMPONENT:-}
readonly repo_report_sha256=447fe08ba0a105e65ceb6795162015944b67d7c0b639df010efa4989e49e0e23
[[ $(hostname) == "$expected_host" ]] || { echo 'not the configured builder' >&2; exit 2; }
[[ $revision =~ ^[a-f0-9]{40}$ ]] || { echo 'full source revision required' >&2; exit 2; }
[[ $CARGO_TARGET_DIR == /* && $artifacts == /* && $artifacts != / ]] || exit 2
[[ ! -e $artifacts ]] || { echo 'artifacts directory must be new' >&2; exit 2; }
[[ -f source.sha256 ]] || { echo 'source.sha256 from the committed source is required' >&2; exit 2; }
[[ $repo_report_component == /* && -f $repo_report_component && ! -L $repo_report_component ]] || { echo 'MCT_REPO_REPORT_COMPONENT must name the exact regular PR #107 component artifact' >&2; exit 2; }
[[ $(sha256sum "$repo_report_component" | cut -d' ' -f1) == "$repo_report_sha256" ]] || { echo 'mct-repo-report component digest mismatch' >&2; exit 2; }
sha256sum --check source.sha256
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
nix develop .#mother --command cargo build --locked --offline -p mct-daemon --bin mct-daemon --example federation_host
nix develop .#mother --command cargo test --locked --offline -p mct-daemon --example federation_host
mkdir -m700 "$artifacts"
install -m555 "$CARGO_TARGET_DIR/debug/mct-daemon" "$artifacts/mct-daemon"
install -m555 "$CARGO_TARGET_DIR/debug/examples/federation_host" "$artifacts/federation_host"
install -m444 "$repo_report_component" "$artifacts/mct-repo-report.wasm"
install -m444 crates/mct-repo-report/child.toml "$artifacts/child.toml"
install -m444 Cargo.lock "$artifacts/Cargo.lock"
printf '{"source_revision":"%s","profile":"dev-no-debuginfo","purpose":"test-only","fixtures":["fleet-arithmetic","mct-repo-report-directory-read"],"repo_report_component_sha256":"%s"}\n' "$revision" "$repo_report_sha256" > "$artifacts/provenance.json"
sha256sum "$artifacts/mct-daemon" "$artifacts/federation_host" "$artifacts/mct-repo-report.wasm" "$artifacts/child.toml" "$artifacts/Cargo.lock"
nix-build nix/federation-bundle.nix --arg artifacts "$artifacts" --no-out-link
