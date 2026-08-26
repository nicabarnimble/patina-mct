#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in mdbook mdbook-linkcheck2; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing documentation tool: $tool" >&2
    echo "run ./scripts/install-docs-tools.sh" >&2
    exit 1
  fi
done

rm -rf target/mdbook target/rustdoc target/site/docs target/site/api

mdbook build docs

RUSTDOCFLAGS="${RUSTDOCFLAGS:+$RUSTDOCFLAGS }-D warnings" \
  cargo doc --workspace --no-deps --target-dir target/rustdoc

mkdir -p target/site
cp -R target/mdbook/html target/site/docs
cp -R target/rustdoc/doc target/site/api

printf 'Product documentation: %s\n' "$repo_root/target/site/docs/index.html"
printf 'Rust API documentation: %s\n' "$repo_root/target/site/api/index.html"
