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
if [[ $archive_name =~ ^mct-daemon-v[0-9]+\.[0-9]+\.[0-9]+-aarch64-apple-darwin\.tar\.gz$ ]]; then
  target=aarch64-apple-darwin
else
  printf 'release artifact has unsupported R3 target basename: %s\n' "$archive_name" >&2
  exit 1
fi

temp=$(mktemp -d "${TMPDIR:-/tmp}/mct-release-verify.XXXXXX")
trap 'rm -rf "$temp"' EXIT
cargo run --quiet -p mct-daemon --example verify-release -- \
  "$archive" "$temp/extracted" "$target"
release_root=$(find "$temp/extracted" -mindepth 1 -maxdepth 1 -type d -print)
if [[ -z $release_root || $release_root == *$'\n'* ]]; then
  printf 'release verification did not extract exactly one package root\n' >&2
  exit 1
fi
scripts/release-target.sh "$target" verify "$release_root/payload"
printf 'release-signature: target adapter verified %s\n' "$target"
