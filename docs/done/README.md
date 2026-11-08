# Done and proven: the C1-C7 harness

Parent: [NIC-123](https://linear.app/nicabar/issue/NIC-123). This piece:
[NIC-124](https://linear.app/nicabar/issue/NIC-124).

## Run

```sh
bash scripts/federation/suite.sh done /absolute/new/evidence-directory
```

The command prints one line per check, with its steps beneath it, and writes
`result.json`, `summary.txt` and per-probe cargo logs into the new directory.

| Exit | Meaning |
| --- | --- |
| 0 | All seven checks pass in the governed profile. MCT is done. |
| 1 | Not done. Every non-passing step matches `known-failures.json` exactly. |
| 2 | Tracking drift or a harness error. A failure is untracked, a tracked failure moved, or the frozen text changed. |

## Rules

- The check text is read from section 4 of
  [`docs/design/mct-shape-review-opus.md`](../design/mct-shape-review-opus.md)
  and pinned by SHA-256 in `scripts/federation/done.mjs`. Editing it exits 2.
  Scope changes need Doug.
- A check passes only when every **governed** step passes. **Baseline** steps
  are legacy-profile or local evidence. They can make a check `partly`, never
  `pass`, and the output labels them "legacy-profile baseline, not
  governed-profile proof".
- A step with unmet `needs` is `blocked` and its probe does not run.
- Each step's basis is `observed` (a cargo test or checked-in evidence ran),
  `declared` (nothing exists to observe yet, so the reason is stated), or
  `blocked`.
- Cargo probes run with `umask 077` and a resolved `TMPDIR`, the same as
  `scripts/ci-tier0.sh`. Ignored probe tests (`done_*`) are desired behavior
  that fails today with a `DONE-REASON:` line.
- [`known-failures.json`](known-failures.json) lists every non-passing step
  with its exact reason and owning piece. The piece that changes a reason
  updates its entries, and un-ignores and keeps its probe test, in the same
  change. A declared probe is replaced by an observed one before its step can
  pass.

Baseline results are recorded in
[`docs/evidence/nic124-done-baseline/`](../evidence/nic124-done-baseline/).
The governed profile and its pending live-tap and Pi commands are in
[governed-profile.md](governed-profile.md).

After NIC-125 (mandatory Toy-grant expiry), C2's expiry step passes:
[`docs/evidence/nic125-done-after-expiry/`](../evidence/nic125-done-after-expiry/).
