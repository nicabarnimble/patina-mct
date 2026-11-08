#!/usr/bin/env bash
set -euo pipefail

root=${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
fail() { printf 'container-package: %s\n' "$*" >&2; exit 1; }
need() { [[ -f "$root/$1" ]] || fail "missing $1"; }
contains() { grep -Fq -- "$2" "$root/$1" || fail "$1 must contain: $2"; }

need Dockerfile
need .dockerignore
need scripts/container-entrypoint.sh
need scripts/container-smoke.sh

contains Dockerfile 'ARG RUST_IMAGE=rust:1.96.0-bookworm@sha256:'
contains Dockerfile 'ARG RUNTIME_IMAGE=debian:bookworm-slim@sha256:'
contains Dockerfile "FROM --platform=linux/amd64 \${RUST_IMAGE} AS build"
contains Dockerfile "FROM --platform=linux/amd64 \${RUNTIME_IMAGE} AS mother"
contains Dockerfile 'cargo build --release --locked --offline --target x86_64-unknown-linux-gnu'
contains Dockerfile 'USER 10001:10001'
contains Dockerfile 'HEALTHCHECK'
contains Dockerfile 'mct-daemon", "status", "--uds", "/var/lib/mct/control.sock", "--json"'
contains Dockerfile 'VOLUME ["/var/lib/mct"]'
contains Dockerfile 'EXPOSE 11204/udp'
contains Dockerfile 'ENTRYPOINT ["/usr/local/bin/mct-container-entrypoint"]'

contains scripts/container-entrypoint.sh 'stat -c'
contains scripts/container-entrypoint.sh '10001:10001 700'
contains scripts/container-entrypoint.sh 'exec /usr/local/bin/mct-daemon serve'

contains scripts/container-smoke.sh '--network none'
contains scripts/container-smoke.sh '--read-only'
contains scripts/container-smoke.sh '--cap-drop ALL'
contains scripts/container-smoke.sh 'no-new-privileges'
contains scripts/container-smoke.sh '--pids-limit'
contains scripts/container-smoke.sh '--memory'
contains scripts/container-smoke.sh 'docker volume create'
contains Dockerfile 'nic131_repair_governed_peers_exchange_receiver_issued_proof_and_call'

if grep -Eq '(^|[[:space:]])(--privileged|--network[=[:space:]]+host|/var/run/docker\.sock|seccomp[=:]unconfined)' "$root/scripts/container-smoke.sh"; then
  fail 'container smoke requests a forbidden privilege or host integration'
fi
printf 'container-package: pinned amd64 build and least-privilege runtime contract passed\n'
