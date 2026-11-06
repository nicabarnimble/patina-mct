#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
output=${1:-"$PWD/target/first-real-app"}
[[ $output == /* ]] || { echo 'output must be absolute' >&2; exit 2; }
mkdir -p "$output"
offline=${MCT_CHILD_CARGO_OFFLINE:-1}
case "$offline" in
  1) offline_flag=(--offline) ;;
  0) offline_flag=() ;;
  *) echo 'MCT_CHILD_CARGO_OFFLINE must be 0 or 1' >&2; exit 2 ;;
esac
CARGO_TARGET_DIR="$output/build" cargo build -p mct-repo-report --target wasm32-wasip1 --release --locked "${offline_flag[@]}"
wasm-tools component new "$output/build/wasm32-wasip1/release/mct_repo_report.wasm" -o "$output/mct-repo-report.wasm"
wasm-tools validate --features component-model "$output/mct-repo-report.wasm"
wasm-tools component wit "$output/mct-repo-report.wasm" > "$output/child.wit"
actual=$(awk '/^world / { inworld=1; next } inworld && /^}/ { inworld=0 } inworld && /^  import / { print $2 }' "$output/child.wit" | LC_ALL=C sort)
expected=$(printf '%s\n' 'wasi:filesystem/preopens@0.2.3;' 'wasi:filesystem/types@0.2.3;' | LC_ALL=C sort)
if [[ "$actual" != "$expected" ]]; then
  printf 'unexpected component imports:\n%s\n' "$actual" >&2
  exit 1
fi
cp crates/mct-repo-report/child.toml "$output/child.toml"
shasum -a 256 "$output/mct-repo-report.wasm"
