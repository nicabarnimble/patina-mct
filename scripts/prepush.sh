#!/usr/bin/env bash
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# Git exports repository-local variables to hooks. Clear them before any nested
# test creates a scratch repository, or its commits can mutate the caller.
while IFS= read -r variable; do
  [[ -n $variable ]] && unset "$variable"
done < <(git rev-parse --local-env-vars)

cd "$repo_root"

classify_direct_updates() {
  local local_ref local_oid remote_ref remote_oid
  while read -r local_ref local_oid remote_ref remote_oid; do
    [[ -n ${local_ref:-} ]] || continue
    if [[ $remote_ref != refs/heads/main ]]; then
      continue
    fi
    if [[ $local_oid == 0000000000000000000000000000000000000000 ]]; then
      printf 'push-policy: deleting main is not low-risk; open a PR\n' >&2
      return 1
    fi
    node scripts/push-policy.mjs --route direct --base "$remote_oid" --head "$local_oid" || return 1
  done
}

if [[ ${MCT_PREPUSH_HOOK:-0} == 1 ]]; then
  classify_direct_updates || exit 1
elif [[ ${MCT_PUSH_ROUTE:-} == pr ]]; then
  node scripts/push-policy.mjs \
    --route pr --base "${MCT_PUSH_BASE:?MCT_PUSH_BASE is required}" \
    --head "${MCT_PUSH_HEAD:?MCT_PUSH_HEAD is required}" || exit 1
fi

status=0
bash scripts/ci-tier0.sh all || status=1
bash scripts/check-deploy-together.sh || status=1
exit "$status"
