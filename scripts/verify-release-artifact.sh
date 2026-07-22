#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if (($# != 1)); then
  printf 'usage: %s <mct-daemon-release.tar.gz>\n' "$0" >&2
  exit 2
fi
archive=$1
archive_name=$(basename "$archive")
if [[ $archive_name =~ ^mct-daemon-v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?-(.+)\.tar\.gz$ ]]; then
  target=${BASH_REMATCH[2]}
else
  printf 'release artifact has invalid target-triple basename: %s\n' "$archive_name" >&2
  exit 1
fi

temp=$(mktemp -d "${TMPDIR:-/tmp}/mct-release-verify.XXXXXX")
trap 'rm -rf "$temp"' EXIT
cargo run --quiet -p mct-daemon --example verify-release -- \
  "$archive" "$temp/extracted" "$target"
