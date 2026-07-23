#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
target=${1:?usage: release-target.sh <target> <adapter-command> [args...]}
shift
case $target in
  aarch64-apple-darwin)
    exec "$repo_root/scripts/release/targets/aarch64-apple-darwin.sh" "$@"
    ;;
  *-unknown-linux-*|*-linux-*)
    exec "$repo_root/scripts/release/targets/linux-unsupported.sh" "$@"
    ;;
  *)
    printf 'unsupported release target adapter: %s\n' "$target" >&2
    exit 1
    ;;
esac
