#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || {
  printf 'release-platform-test: target-specific test requires macOS arm64\n' >&2
  exit 1
}
temp=$(mktemp -d "${TMPDIR:-/tmp}/mct-release-platform-test.XXXXXX")
trap 'rm -rf "$temp"' EXIT
cargo build --quiet -p mct-daemon --bin mct-daemon
scripts/release-target.sh aarch64-apple-darwin assemble \
  target/debug/mct-daemon "$temp/payload" 0.2.0
scripts/release-target.sh aarch64-apple-darwin verify "$temp/payload"
actual=$(find "$temp/payload/mct-daemon.app/Contents/_CodeSignature" \
  -mindepth 1 -maxdepth 1 -type f -print | sed "s|.*/||")
[[ $actual == CodeResources ]]
printf forbidden > "$temp/payload/mct-daemon.app/Contents/_CodeSignature/CodeDirectory"
if scripts/release-target.sh aarch64-apple-darwin verify "$temp/payload" >/dev/null 2>&1; then
  printf 'platform verifier admitted an extra signature member\n' >&2
  exit 1
fi
if MCT_RELEASE_SIGNING_MODE=notarized \
  scripts/release-target.sh aarch64-apple-darwin assemble \
    target/debug/mct-daemon "$temp/notarized" 0.2.0 >/dev/null 2>&1; then
  printf 'notarization slot opened without credentials\n' >&2
  exit 1
fi
if MCT_RELEASE_SIGNING_MODE=notarized \
  MCT_APPLE_SIGNING_IDENTITY=test MCT_NOTARYTOOL_KEYCHAIN_PROFILE=test \
  scripts/release-target.sh aarch64-apple-darwin assemble \
    target/debug/mct-daemon "$temp/notarized" 0.2.0 >/dev/null 2>&1; then
  printf 'notarization execution unexpectedly opened in R3\n' >&2
  exit 1
fi
plan=$(scripts/release-target.sh aarch64-apple-darwin notarization-plan \
  'Developer ID Application: SLOT' 'MCT-NOTARY-SLOT')
for required in '--options runtime' 'notarytool submit' 'stapler staple' 'stapler validate'; do
  grep -F -- "$required" <<<"$plan" >/dev/null
 done
if scripts/release-target.sh x86_64-unknown-linux-gnu assemble ignored ignored 0.2.0 \
  >/dev/null 2>&1; then
  printf 'Linux signing slot unexpectedly emitted a release\n' >&2
  exit 1
fi
printf 'release-platform-test: signed bundle exact; future slots closed\n'
