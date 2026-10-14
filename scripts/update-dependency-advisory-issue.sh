#!/usr/bin/env bash
# Open or update the single open issue titled "dependency advisories".
set -euo pipefail

title="dependency advisories"
repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
server="${GITHUB_SERVER_URL:-https://github.com}"
run_id="${GITHUB_RUN_ID:-}"
run_url="${server}/${repo}/actions/runs/${run_id}"
audit_log="${1:-}"
ignore_log="${2:-}"

clip() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    printf '(no log)\n'
    return
  fi
  # Keep a comment under GitHub's body limit and avoid breaking the fence.
  tail -c 20000 "$file" | sed $'s/\x1b\\[[0-9;]*[A-Za-z]//g; s/`/\'/g'
}

body=$(
  cat <<EOF
Nightly dependency audit on \`main\` failed.

Run: ${run_url}

The R2 policy in \`.cargo/audit.toml\` is unchanged: warnings are denied, and unmaintained and unsound informational advisories are evaluated.

### cargo audit

\`\`\`
$(clip "$audit_log")
\`\`\`

### ignore revisit check

\`\`\`
$(clip "$ignore_log")
\`\`\`
EOF
)

if [[ "${DRY_RUN:-}" == 1 ]]; then
  printf '%s\n' "$body"
  exit 0
fi

number=$(
  gh issue list \
    --repo "$repo" \
    --state open \
    --limit 50 \
    --search "in:title \"${title}\"" \
    --json number,title \
    | python3 -c 'import json,sys; title=sys.argv[1]; rows=json.load(sys.stdin); matches=[str(row["number"]) for row in rows if row.get("title")==title]; print(matches[0] if matches else "")' \
      "$title"
)

if [[ -n "$number" ]]; then
  gh issue comment --repo "$repo" "$number" --body "$body"
  printf 'updated issue #%s\n' "$number"
else
  gh issue create --repo "$repo" --title "$title" --body "$body"
  printf 'opened issue %s\n' "$title"
fi
