#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

target=""
output=""
source_commit=$(git rev-parse HEAD)
source_epoch=$(git show -s --format=%ct HEAD)
while (($#)); do
  case $1 in
    --target)
      target=${2:?missing --target value}
      shift 2
      ;;
    --output)
      output=${2:?missing --output value}
      shift 2
      ;;
    --source-commit)
      source_commit=${2:?missing --source-commit value}
      shift 2
      ;;
    --source-epoch)
      source_epoch=${2:?missing --source-epoch value}
      shift 2
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done
if [[ -z $target || -z $output ]]; then
  printf 'usage: %s --target <triple> --output <directory> [--source-commit sha] [--source-epoch epoch]\n' "$0" >&2
  exit 2
fi

if [[ $(cargo-sbom --version) != "cargo-sbom 0.10.0" ]]; then
  printf 'cargo-sbom 0.10.0 is required\n' >&2
  exit 1
fi
version=$(python3 - <<'PY'
import tomllib
with open('Cargo.toml', 'rb') as handle:
    print(tomllib.load(handle)['workspace']['package']['version'])
PY
)

temp=$(mktemp -d "${TMPDIR:-/tmp}/mct-release-sbom.XXXXXX")
trap 'rm -rf "$temp"' EXIT
cargo sbom --output-format cyclone_dx_json_1_6 > "$temp/raw.json"
fixture_paths=(
  crates/mct-daemon/tests/fixtures/slate-manager-0.2.0/slate-manager.toml
  crates/mct-daemon/tests/fixtures/slate-manager-0.2.0/slate-manager.wasm
  crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/child.toml
  crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/folder-watch-actor.wasm
  crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0/MCT-REBUILD.patch
  crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0/child.toml
  crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0/watch-null-sink.wasm
)
cargo run --quiet -p mct-daemon --example release-digests -- "${fixture_paths[@]}" \
  > "$temp/digests.json"
mkdir -p "$output"
python3 scripts/normalize-release-sbom.py \
  --raw "$temp/raw.json" \
  --digests "$temp/digests.json" \
  --sbom-output "$output/SBOM.cdx.json" \
  --provenance-output "$output/FIXTURE-PROVENANCE.json" \
  --source-commit "$source_commit" \
  --source-epoch "$source_epoch" \
  --target "$target" \
  --version "$version"
printf 'release-sbom: wrote %s and %s\n' \
  "$output/SBOM.cdx.json" "$output/FIXTURE-PROVENANCE.json"
