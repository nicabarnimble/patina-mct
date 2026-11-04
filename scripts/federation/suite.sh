#!/usr/bin/env bash
set -euo pipefail
umask 077
if [[ $# != 2 || $1 != /* || $2 != /* ]]; then
  printf 'usage: bash suite.sh /absolute/private/fleet.json /absolute/new/evidence\n' >&2
  exit 64
fi
config=$1
evidence=$2
controller="$(cd -- "$(dirname -- "$0")" && pwd)/run.mjs"
test -f "$config"
mkdir -m700 -- "$evidence"
for slice in two three upgrade; do
  node "$controller" --config "$config" --slice "$slice" --evidence "$evidence/$slice"
done
printf '%s\n' '{"status":"passed","results":["two/result.json","three/result.json","upgrade/result.json"]}' > "$evidence/result.json"
