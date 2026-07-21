#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UPSTREAM="${1:-$(dirname "$ROOT")/patina-child-watcher-system}"
COMMIT="526dbf123b040198cb4395c1a63cf498a28ff915"
FOLDER_TAG="folder-watch-actor-v0.1.0"
SINK_TAG="watch-null-sink-v0.1.0"
FOLDER_FIXTURE="$ROOT/crates/mct-daemon/tests/fixtures/folder-watch-actor-0.1.0"
SINK_FIXTURE="$ROOT/crates/mct-daemon/tests/fixtures/watch-null-sink-0.1.0"
PATCH="$FOLDER_FIXTURE/MCT-REBUILD.patch"

for command in git cargo rustc wasm-tools shasum cmp; do
  command -v "$command" >/dev/null || {
    echo "missing required command: $command" >&2
    exit 1
  }
done

test -d "$UPSTREAM/.git" || {
  echo "upstream repository not found: $UPSTREAM" >&2
  exit 1
}

resolved="$(git -C "$UPSTREAM" rev-parse "$COMMIT^{commit}")"
test "$resolved" = "$COMMIT" || {
  echo "upstream commit mismatch: $resolved" >&2
  exit 1
}
for tag in "$FOLDER_TAG" "$SINK_TAG"; do
  tagged="$(git -C "$UPSTREAM" rev-parse "refs/tags/$tag^{commit}")"
  test "$tagged" = "$COMMIT" || {
    echo "$tag resolves to $tagged, expected $COMMIT" >&2
    exit 1
  }
done

work="$(mktemp -d "${TMPDIR:-/tmp}/mct-watch-fixtures.XXXXXX")"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/source"
git -C "$UPSTREAM" archive "$COMMIT" | tar -x -C "$work/source"

cd "$work/source"
git apply --check "$PATCH"
git apply "$PATCH"

test "$(grep -c 'absolute_path: relative_path.clone()' children/folder-watch-actor/src/lib.rs)" -eq 2
test -z "$(grep -E 'wasi:http/outgoing-handler|wasi:sql/readwrite|patina:connect/connect|patina:git/git' children/folder-watch-actor/wit/world.wit || true)"

cargo component build --release -p patina-ai-child-folder-watch-actor
cargo component build --release -p patina-ai-child-watch-null-sink

folder_output="$work/source/target/wasm32-wasip1/release/patina_ai_child_folder_watch_actor.wasm"
sink_output="$work/source/target/wasm32-wasip1/release/patina_ai_child_watch_null_sink.wasm"

cmp "$folder_output" "$FOLDER_FIXTURE/folder-watch-actor.wasm"
cmp "$sink_output" "$SINK_FIXTURE/watch-null-sink.wasm"
cmp "$work/source/children/folder-watch-actor/child.toml" "$FOLDER_FIXTURE/child.toml"
cmp "$work/source/children/watch-null-sink/child.toml" "$SINK_FIXTURE/child.toml"

for sidecar in \
  "$FOLDER_FIXTURE/child.toml.sha256" \
  "$FOLDER_FIXTURE/folder-watch-actor.wasm.sha256" \
  "$SINK_FIXTURE/child.toml.sha256" \
  "$SINK_FIXTURE/watch-null-sink.wasm.sha256"
do
  test ! -e "$sidecar" || {
    echo "raw fixture sidecar is forbidden: $sidecar" >&2
    exit 1
  }
done

mkdir -p "$work/blake3/src"
cat >"$work/blake3/Cargo.toml" <<'EOF'
[package]
name = "mct-fixture-blake3"
version = "0.0.0"
edition = "2024"

[dependencies]
blake3 = "1"
EOF
cat >"$work/blake3/src/main.rs" <<'EOF'
fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read receipt input");
        println!("{}  {}", blake3::hash(&bytes).to_hex(), path);
    }
}
EOF

printf '%s\n' 'toolchain:'
rustc --version
cargo --version
cargo component --version
wasm-tools --version
printf '%s\n' 'SHA-256 receipts:'
shasum -a 256 \
  "$PATCH" \
  "$FOLDER_FIXTURE/child.toml" \
  "$FOLDER_FIXTURE/folder-watch-actor.wasm" \
  "$SINK_FIXTURE/child.toml" \
  "$SINK_FIXTURE/watch-null-sink.wasm"
printf '%s\n' 'BLAKE3 receipts:'
cargo run --quiet --manifest-path "$work/blake3/Cargo.toml" -- \
  "$PATCH" \
  "$FOLDER_FIXTURE/child.toml" \
  "$FOLDER_FIXTURE/folder-watch-actor.wasm" \
  "$SINK_FIXTURE/child.toml" \
  "$SINK_FIXTURE/watch-null-sink.wasm"
printf '%s\n' 'byte sizes:'
wc -c \
  "$PATCH" \
  "$FOLDER_FIXTURE/child.toml" \
  "$FOLDER_FIXTURE/folder-watch-actor.wasm" \
  "$SINK_FIXTURE/child.toml" \
  "$SINK_FIXTURE/watch-null-sink.wasm"
printf '%s\n' 'folder component WIT:'
wasm-tools component wit "$folder_output"
printf '%s\n' 'sink component WIT:'
wasm-tools component wit "$sink_output"
