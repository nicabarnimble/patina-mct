#!/bin/sh
set -eu
fail() { printf '%s\n' 'unsupported prebuilt proof command' >&2; exit 64; }
[ "$#" -ge 8 ] || fail
[ "$1" = test ] && [ "$2" = --locked ] && [ "$3" = -p ] || fail
package=$4
shift 4
case "$1" in
    --lib) key="$package:lib"; shift ;;
    --bin|--test) key="$package:$1:$2"; shift 2 ;;
    *) fail ;;
esac
case "$key" in
    mct-daemon:lib|mct-kernel:lib|mct-observation:lib|mct-daemon:--bin:mct-daemon|mct-daemon:--test:federation_fleet|mct-daemon:--test:mother_restart) ;;
    *) fail ;;
esac
filter=$1
shift
case "$filter" in ''|-*|*[!A-Za-z0-9_:-]*) fail ;; esac
[ "$1" = -- ] || fail
shift
if [ "$#" = 1 ]; then
    [ "$1" = --list ] || fail
elif [ "$#" = 2 ]; then
    [ "$1" = --nocapture ] && [ "$2" = --test-threads=1 ] || fail
else
    fail
fi
root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)
binary=$(awk -F '\t' -v key="$key" '$1 == key { print $2 }' "$root/artifacts.tsv")
[ -n "$binary" ] && [ -x "$root/$binary" ] || fail
printf 'prebuilt dispatch: %s %s\n' "$binary" "$filter" >&2
exec "$root/$binary" "$filter" "$@"
