---
name: correct
description: Find repeated post-merge mistake classes and replace them with structural ownership and rerunnable pre-merge proof. Use for /correct and correction passes over recent repairs, reverts, and review findings.
disable-model-invocation: true
---

# Correct

Correct the class, not only its latest instance.

## Method

1. Read the merged change, the repair, nearby commits, reverts, and review findings. Name the repeated mistake in one sentence.
2. Fix at the highest level that works:
   1. architecture: one owner, one supported path, one source of truth, old path deleted;
   2. types, lint, or CI: the failure names the supported replacement;
   3. behavior tests across the real boundary;
   4. docs and agent rules only after executable enforcement.
3. Fix the most frequent class now. Put each independent correction in its own commit.
4. For every new check, run it against a real past mistake and record the nonzero exit and named failure. Run the same command against the repaired tree and current head and record zero.
5. Use `scripts/prepush.sh` locally and in CI. Install `.githooks/pre-push` through `scripts/install-git-hooks.sh`; do not duplicate its gate list elsewhere.
6. Let `scripts/push-policy.mjs` decide direct-main eligibility. Do not restate or widen its path rules in agent judgment.
7. Keep exceptions on the affected line in this form:
   `correct-exception: reason=<why>; expires=YYYY-MM-DD; approved-by=@<human>`
   An exception without all three fields is invalid. Repository-wide or permanent exceptions are invalid.
8. Update the agent instruction rule table. Every written rule names its executable enforcer. If no executable enforcer exists, label the row `review only` and explain why structure cannot express it.

## Completion report

Report each mistake class with its evidence, the correction level, why no higher level worked, the commits, red and green commands with exit codes, the pull request, and current CI status. Do not merge during a correction pass unless the user separately authorizes it.
