#!/usr/bin/env bash
set -euo pipefail

# Comparative files identify themselves by discussing the integrated Patina tree or
# by using the transitional namespace tokens. Historical evidence is immutable and
# intentionally excluded. The glossary is excluded because it defines and contrasts
# both preferred and prohibited forms.
pattern='\b(MCT[[:space:]]+Mother|Patina[[:space:]]+Mother|(existing|current|legacy|original)([[:space:]]+(integrated|Patina))?[[:space:]]+Mother|(existing|current|legacy|original)-Mother)\b'
legacy_checkout_pattern='(/Users/[^/]+/Projects/Sandbox/AI/RUST/patina|~/Projects/Sandbox/AI/RUST/patina)'
status=0

if rg -n "$legacy_checkout_pattern" AGENTS.md README.md layer \
  --glob '!layer/sessions/**' \
  --glob '!layer/events.jsonl' \
  --glob '!layer/slate/events.jsonl' \
  --glob '!layer/slate/work/**/work.toml'; then
  printf '%s\n' 'workstation-local integrated Patina link found; use a pinned GitHub URL' >&2
  status=1
fi

while IFS= read -r file; do
  if rg -n -i -U "$pattern" "$file"; then
    status=1
  fi
done < <(
  rg -l \
    'patinaMother|patinaChild|patinaToy|integrated Patina|github.com/NicabarNimble/patina' \
    AGENTS.md README.md layer \
    --glob '!layer/core/migration-vocabulary.md' \
    --glob '!layer/sessions/**' \
    --glob '!layer/events.jsonl' \
    --glob '!layer/slate/events.jsonl' \
    --glob '!layer/slate/work/**/work.toml' \
    || true
)

if ((status != 0)); then
  printf '%s\n' 'ambiguous comparative Mother vocabulary found' >&2
  printf '%s\n' 'use mctMother/patinaMother in comparative contexts; see layer/core/migration-vocabulary.md' >&2
  exit 1
fi
