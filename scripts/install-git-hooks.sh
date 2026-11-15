#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
git config --local core.hooksPath .githooks
[[ $(git config --local --get core.hooksPath) == .githooks ]]
printf 'git-hooks: core.hooksPath=.githooks\n'

origin_url=$(git remote get-url origin 2>/dev/null || true)
if [[ $origin_url == ssh://* || $origin_url == *@*:* ]]; then
  if ! git config --local --get core.sshCommand >/dev/null; then
    git config --local core.sshCommand \
      'ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=120'
    printf 'git-hooks: configured SSH keepalive for the full pre-push gate\n'
  else
    printf 'git-hooks: preserved existing core.sshCommand; ensure it keeps long pushes alive\n'
  fi
fi
