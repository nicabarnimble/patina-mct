#!/bin/sh
set -eu

state=/var/lib/mct
mother=${MCT_MOTHER_ID:?MCT_MOTHER_ID is required}
vision=${MCT_VISION_ID:?MCT_VISION_ID is required}
bind=${MCT_BIND_ADDR:-0.0.0.0:11204}
max_connections=${MCT_MAX_CONNECTIONS:-64}

case "$mother" in *[!a-z0-9-]*|'') echo 'MCT_MOTHER_ID must use lowercase letters, digits and hyphens' >&2; exit 64;; esac
case "$vision" in *[!a-z0-9-]*|'') echo 'MCT_VISION_ID must use lowercase letters, digits and hyphens' >&2; exit 64;; esac
case "$max_connections" in *[!0-9]*|'0'|'') echo 'MCT_MAX_CONNECTIONS must be a positive integer' >&2; exit 64;; esac

actual=$(stat -c '%u:%g %a' "$state" 2>/dev/null || true)
if [ "$actual" != '10001:10001 700' ]; then
  echo "state volume must be owned by 10001:10001 with mode 0700; found: ${actual:-missing}" >&2
  exit 78
fi

cd "$state"
umask 077
exec /usr/local/bin/mct-daemon serve \
  --identity "$state/identity/iroh-secret.hex" \
  --config "$state/config.json" \
  --children-dir "$state/children" \
  --state "$state/state.sqlite" \
  --ledger "$state/observations.jsonl" \
  --uds "$state/control.sock" \
  --node-id "$mother" \
  --vision "$vision" \
  --bind "$bind" \
  --max-connections "$max_connections"
