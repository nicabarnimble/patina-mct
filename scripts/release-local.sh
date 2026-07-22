#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
command_name=${1:-}
shift || true
target=aarch64-apple-darwin
output="$repo_root/dist"
while (($#)); do
  case $1 in
    --target) target=${2:?missing --target value}; shift 2 ;;
    --output) output=${2:?missing --output value}; shift 2 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
done
if [[ $command_name != build ]]; then
  printf 'usage: %s build [--target aarch64-apple-darwin] [--output directory]\n' "$0" >&2
  exit 2
fi
case $output in
  /*) ;;
  *) output="$repo_root/$output" ;;
esac
mkdir -p "$output"
output=$(cd "$output" && pwd -P)

work=$(mktemp -d "${TMPDIR:-/tmp}/mct-release-local.XXXXXX")
source="$work/source"
cleanup() {
  git -C "$repo_root" worktree remove --force "$source" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT
git -C "$repo_root" worktree add --detach "$source" HEAD >/dev/null
cd "$source"
[[ -z $(git status --porcelain --untracked-files=all) ]] || {
  printf 'detached release worktree is not clean\n' >&2
  exit 1
}
scripts/check-release-version.sh
version=$(python3 - <<'PY'
import tomllib
with open('Cargo.toml', 'rb') as handle:
    print(tomllib.load(handle)['workspace']['package']['version'])
PY
)
source_commit=$(git rev-parse HEAD)
source_epoch=$(git show -s --format=%ct HEAD)
rust_version=$(rustc -Vv)
cargo_version=$(cargo -V)
export CARGO_TARGET_DIR="$work/target"

cargo build --release --locked --target "$target" -p mct-daemon \
  --bin mct-daemon --example release-digests
binary="$CARGO_TARGET_DIR/$target/release/mct-daemon"
digest_helper="$CARGO_TARGET_DIR/$target/release/examples/release-digests"
[[ $($binary version) == "mct-daemon $version" ]] || {
  printf 'built daemon version does not match workspace version\n' >&2
  exit 1
}

metadata="$work/metadata"
scripts/generate-release-sbom.sh \
  --target "$target" --output "$metadata" \
  --source-commit "$source_commit" --source-epoch "$source_epoch"
scripts/check-release-notes.sh "$version"
python3 scripts/extract-release-notes.py "$version" "$metadata/RELEASE-NOTES.md"

payload="$work/payload"
mkdir -p "$payload"
scripts/release-target.sh "$target" assemble "$binary" "$payload" "$version"
python3 scripts/package-release.py \
  --source "$source" \
  --payload "$payload" \
  --notes "$metadata/RELEASE-NOTES.md" \
  --sbom "$metadata/SBOM.cdx.json" \
  --provenance "$metadata/FIXTURE-PROVENANCE.json" \
  --output "$output" \
  --target "$target" \
  --version "$version" \
  --source-commit "$source_commit" \
  --source-epoch "$source_epoch" \
  --rust-version "$rust_version" \
  --cargo-version "$cargo_version" \
  --release-mode smoke \
  --signing-mode adhoc \
  --executable-relative-path payload/mct-daemon.app/Contents/MacOS/mct-daemon \
  --digest-helper "$digest_helper"
archive="$output/mct-daemon-v$version-$target.tar.gz"
scripts/verify-release-artifact.sh "$archive"
printf 'release-local: verified %s from detached clean %s\n' "$archive" "$source_commit"
