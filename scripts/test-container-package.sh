#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
checker="$repo_root/scripts/check-container-package.sh"

bash "$checker" "$repo_root"

work=$(mktemp -d "${TMPDIR:-/tmp}/mct-container-package.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/scripts"
cp "$repo_root/Dockerfile" "$repo_root/.dockerignore" "$work/"
cp "$repo_root/scripts/container-entrypoint.sh" "$repo_root/scripts/container-smoke.sh" "$work/scripts/"

expect_refused() {
  local name=$1 pattern=$2 replacement=$3
  cp "$repo_root/Dockerfile" "$work/Dockerfile"
  cp "$repo_root/scripts/container-smoke.sh" "$work/scripts/container-smoke.sh"
  PATTERN=$pattern REPLACEMENT=$replacement perl -0pi -e \
    's/\Q$ENV{PATTERN}\E/$ENV{REPLACEMENT}/g' "$work/${4:-Dockerfile}"
  if bash "$checker" "$work" >"$work/$name.out" 2>&1; then
    printf 'container-package-test: mutation survived: %s\n' "$name" >&2
    exit 1
  fi
}

expect_refused unpinned-builder \
  'rust:1.96.0-bookworm@sha256:' 'rust:1.96.0-bookworm#sha256:'
expect_refused root-runtime 'USER 10001:10001' 'USER 0:0'
expect_refused online-compile ' --offline --target' ' --target'
expect_refused hardcoded-product-version '${PRODUCT_VERSION}' '0.2.0'
expect_refused host-network '--network none' '--network host' scripts/container-smoke.sh

printf 'container-package-test: 5/5 unsafe mutations refused\n'
