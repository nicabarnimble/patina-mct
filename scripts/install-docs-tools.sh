#!/usr/bin/env bash
set -euo pipefail

MDBOOK_VERSION="0.5.4"
LINKCHECK_VERSION="0.13.0"

cargo install --locked --version "$MDBOOK_VERSION" mdbook
cargo install --locked --version "$LINKCHECK_VERSION" mdbook-linkcheck2
