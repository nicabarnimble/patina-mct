#!/usr/bin/env bash
set -euo pipefail
umask 077
here="$(cd -- "$(dirname -- "$0")" && pwd)"
if [[ ${1-} == done ]]; then
  if [[ $# != 2 || $2 != /* ]]; then
    printf 'usage: bash suite.sh done /absolute/new/evidence\n' >&2
    exit 64
  fi
  exec node "$here/done.mjs" --evidence "$2"
fi
if [[ $# != 2 || $1 != /* || $2 != /* ]]; then
  printf 'usage: bash suite.sh /absolute/private/fleet.json /absolute/new/evidence\n' >&2
  printf '   or: bash suite.sh done /absolute/new/evidence\n' >&2
  exit 64
fi
config=$1
evidence=$2
controller="$here/run.mjs"
test -f "$config"
mkdir -m700 -- "$evidence"
for slice in two three upgrade; do
  node "$controller" --config "$config" --slice "$slice" --evidence "$evidence/$slice"
done
printf '%s\n' '{"status":"passed","results":["two/result.json","three/result.json","upgrade/result.json"]}' > "$evidence/result.json"
