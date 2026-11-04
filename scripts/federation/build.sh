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
[[ $(hostname) == "$expected_host" ]] || { echo 'not the configured builder' >&2; exit 2; }
[[ $revision =~ ^[a-f0-9]{40}$ ]] || { echo 'full source revision required' >&2; exit 2; }
[[ $CARGO_TARGET_DIR == /* && $artifacts == /* && $artifacts != / ]] || exit 2
[[ ! -e $artifacts ]] || { echo 'artifacts directory must be new' >&2; exit 2; }
[[ -f source.sha256 ]] || { echo 'source.sha256 from the committed source is required' >&2; exit 2; }
sha256sum --check source.sha256
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
nix develop .#mother --command cargo build --locked --offline -p mct-daemon --bin mct-daemon --example federation_host
nix develop .#mother --command cargo test --locked --offline -p mct-daemon --example federation_host
mkdir -m700 "$artifacts"
install -m555 "$CARGO_TARGET_DIR/debug/mct-daemon" "$artifacts/mct-daemon"
install -m555 "$CARGO_TARGET_DIR/debug/examples/federation_host" "$artifacts/federation_host"
printf '{"source_revision":"%s","profile":"dev-no-debuginfo","purpose":"test-only"}\n' "$revision" > "$artifacts/provenance.json"
sha256sum "$artifacts/mct-daemon" "$artifacts/federation_host"
nix-build nix/federation-bundle.nix --arg artifacts "$artifacts" --no-out-link
