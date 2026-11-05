#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
output="$PWD/target/first-real-app"
mkdir -p "$output"
CARGO_TARGET_DIR="$output/build" cargo build -p mct-repo-report --target wasm32-wasip1 --release --locked --offline
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
