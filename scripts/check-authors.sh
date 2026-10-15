#!/usr/bin/env bash
# Fail if any commit in RANGE has a non-allowlisted author/committer email
# or a Co-authored-by trailer. Usage: scripts/check-authors.sh <rev-range>
set -euo pipefail
range="${1:?usage: check-authors.sh <rev-range>}"
allow=" nicabar@gmail.com 33350978+nicabarnimble@users.noreply.github.com nicabarnimble@users.noreply.github.com "
is_allowed() { [[ "$allow" == *" ${1,,} "* ]]; }
fail=0; n=0
while IFS=$'\x1f' read -r sha ae ce cn; do
  [ -z "$sha" ] && continue
  n=$((n+1))
  if ! is_allowed "$ae"; then
    echo "::error::$sha author <$ae> is not allowlisted"; fail=1
  fi
  if ! is_allowed "$ce"; then
    if [[ "$cn" == "GitHub" && "$ce" == "noreply@github.com" ]]; then :; else
      echo "::error::$sha committer $cn <$ce> is not allowlisted"; fail=1
    fi
  fi
  if git show -s --format=%B "$sha" | grep -qi '^[[:space:]]*co-authored-by:'; then
    echo "::error::$sha message contains a Co-authored-by trailer"; fail=1
  fi
done < <(git log --format=$'%H\x1f%ae\x1f%ce\x1f%cn' "$range")
if [ "$fail" -ne 0 ]; then
  echo "Author check FAILED: only NicabarNimble may author/commit (GitHub web-flow merges allowed), no co-author trailers." >&2
  exit 1
fi
echo "Author check passed: $n commit(s) in $range."
