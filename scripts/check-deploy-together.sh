#!/usr/bin/env bash
set -uo pipefail

harness_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
source_root=$(cd "${MCT_SOURCE_ROOT:-$harness_root}" && pwd -P)
export TMPDIR=$(cd "${MCT_DEPLOY_TMPDIR:-/tmp}" && pwd -P)
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=${MCT_DEPLOY_TARGET_DIR:-$source_root/target/deploy-together}
umask 077

status=0
run_check() {
  local name=$1
  shift
  printf '=== deploy-together: %s ===\n' "$name"
  if ! "$@"; then
    printf 'deploy-together: %s failed\n' "$name" >&2
    status=1
  fi
}

if [[ $(id -u) -eq 0 ]]; then
  printf 'deploy-together: run as an unprivileged user\n' >&2
  exit 1
fi
for tool in cargo node python3 openssl; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'deploy-together: missing required tool: %s\n' "$tool" >&2
    exit 1
  }
done

run_check release-version bash "$source_root/scripts/check-release-version.sh"
run_check container-package bash "$source_root/scripts/check-container-package.sh" "$source_root"
run_check release-build cargo build \
  --manifest-path "$source_root/Cargo.toml" \
  --release --locked --offline -p mct-daemon --bin mct-daemon

binary="$CARGO_TARGET_DIR/release/mct-daemon"
if [[ -x $binary ]]; then
  product_version=$(python3 - "$source_root/Cargo.toml" <<'PY'
import sys
import tomllib
with open(sys.argv[1], 'rb') as handle:
    print(tomllib.load(handle)['workspace']['package']['version'])
PY
)
  run_check release-identity test "$($binary version)" = "mct-daemon $product_version"
else
  printf 'deploy-together: release binary was not built: %s\n' "$binary" >&2
  status=1
fi

run_check hosted-approval env \
  MCT_SOURCE_ROOT="$source_root" \
  MCT_BINARY="$binary" \
  node --test "$harness_root/scripts/pocketid-deployment.e2e.test.mjs"

if [[ $status -eq 0 ]]; then
  printf 'deploy-together: release Mother, hosted approval gate, OIDC, and headless CLI passed together\n'
fi
exit "$status"
