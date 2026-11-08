#!/usr/bin/env bash
# Runs every model config with TLC and compares each outcome with the
# expected one. Needs Java and tla2tools.jar (TLC 1.7.x): set TLA2TOOLS_JAR.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd -P)"
jar="${TLA2TOOLS_JAR:?set TLA2TOOLS_JAR to tla2tools.jar}"
out="${1:?usage: check.sh <new output directory>}"
mkdir "$out"
status=0
# spec config expected: "holds" or the first invariant that must break
while read -r spec cfg expected; do
  log="$out/$cfg.log"
  set +e
  (cd "$here/models" && java -XX:+UseParallelGC -cp "$jar" tlc2.TLC -workers 1 \
    -metadir "$out/meta-$cfg" -config "$cfg.cfg" "$spec.tla") >"$log" 2>&1
  code=$?
  set -e
  if [ "$expected" = holds ]; then
    if [ "$code" -eq 0 ] && grep -q "No error has been found" "$log"; then
      echo "ok   $cfg: all invariants hold"
    else
      echo "FAIL $cfg: expected all invariants to hold (exit $code)"; status=1
    fi
  else
    if grep -q "Invariant $expected is violated" "$log"; then
      echo "ok   $cfg: $expected breaks, as this counterpart must"
    else
      echo "FAIL $cfg: expected $expected to break (exit $code)"; status=1
    fi
  fi
done <<'TABLE'
PeerSplitReads PeerSplitReads-repaired holds
PeerSplitReads PeerSplitReads-no-kernel-check holds
PeerSplitReads PeerSplitReads-no-hello-recheck HelloNamesAdmittedIdentity
PeerSplitReads PeerSplitReads-no-pipeline-check RevokedPeerNeverDispatched
PeerSplitReads PeerSplitReads-no-generation-checks RevokedPeerNeverDispatched
PeerSplitReads PeerSplitReads-config-fallback HelloNamesAdmittedIdentity
PeerPublication PeerPublication-repaired-TRUE holds
PeerPublication PeerPublication-repaired-FALSE RevokedPeerNeverAdmitted
TABLE
exit "$status"
