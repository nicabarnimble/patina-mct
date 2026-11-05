#!/usr/bin/env bash
set -euo pipefail
umask 077
if [[ $# -eq 0 ]]; then
  config=${MCT_FIRST_REAL_APP_CONFIG:-}
elif [[ $# -eq 2 && $1 == --config && -n $2 ]]; then
  config=$2
else
  echo 'usage: first-real-app-smoke.sh [--config /absolute/private/config.json]' >&2
  exit 2
fi
if [[ $config != /* ]]; then
  echo 'an absolute config path is required' >&2
  exit 2
fi
cd "$(dirname "$0")/.."
root=$PWD
scripts/first-real-app/build-child.sh >&2
CARGO_TARGET_DIR="$root/target" cargo build --locked --offline -p mct-daemon --bin mct-daemon --example first_real_app_host >&2
exec node scripts/first-real-app/run.mjs --config "$config"
