# Pi Durable / MCT handoff

> **Imported 2026-10-18** from working notes (call of 2026-10-14). Status: record of a discussion. The four speed levers at the end are **pending, not accepted**. Nothing here is a decision beyond what [pi-durable-child.md](pi-durable-child.md) and [admin-keys.md](admin-keys.md) already record.


Call date: Wednesday, October 14, 2026. Companion to [Pi Durable as a Child](pi-durable-child.md).

## Lead Agent answers that landed on this call

### Part 1 — What Pi Durable needs that the model withholds or slows

- **Continue-from-memory resume.** Full re-run from the top; Wasmtime can't snapshot a running Child. Open-plus-resume ~136–165 ms either way. Snapshots cut storage (1.13 MB → 479 KB, or 321 KB with read-file as hash), not time.
- **In-place session writes.** Become a grant-gated Toy call that waits for the (e) check-in. No direct mutate of session storage.
- **Self-wake.** Closed: the Child asks through a Toy, Mother fires the wake, and the Child checks in before any model call. Mother unreachable is a hard stop under (c). Pi's Lifecycle alarm is not a second door.
- **Ambient model credentials.** Still open. Pi's harness builds a provider with OPENAI_API_KEY and calls streamSimple directly. MCT wants Mother-held credentials and a gateway that only forwards. Until closed, every model call is a Mother-gated Toy, not a local HTTP hop.
- **Fingerprint rule.** Resume refuses if the harness code hash changes, so an upgrade breaks runs already in flight.

What we can give for speed without weakening authority: the one replay path (recorded Toy results, including the recorded model answer); cheap reads at the ledger's hash level; warm Child load (open-plus-resume ~497 ms cold → ~181 ms warm); a short-lived run credential that shrinks chain bytes while Mother still re-verifies.

Design-note line: Pi Durable becomes a durable agent, not a fast one. The ledger is the system of record.

### Part 2 — Family model

Recommendation: use the older Child (teenager) that already exists. Don't invent an uncle or aunt.

- **Teenager (older Child):** courier for a group; shares only through Mother-minted borrowed handles; can weigh at the pause. Matches Pi's parent/subagent pattern if Mother reissues Toys and attenuation holds. Fits the older-child rules; (c), (d), (e), (f) unchanged.
- **Uncle / aunt** (a peer that can grant, wake, or keep the Child going when Mother is down): looks like a speed escape, but conflicts with Mother essential / no failover (c), handles coming only from the platform / Mother (d), and the closed Child-as-grantor rule. A lateral relative minting handles or firing wakes without Mother is a second grantor.

Speed comes from making the existing path cheaper: hash-level recording, warm instance, recorded model answers on replay, and Pi's subagents behind older-child grants with Mother-minted handles. If more autonomy inside the model is wanted, promote the Pi harness to older-child status for courier/subagent work, not adding an uncle.

Four open contradictions still needing rulings: ambient credentials vs the gateway; a JS Child via jco/StarlingMonkey vs WASI; subagent tool copies vs Mother-minted handles; background work that survives abort vs a stop.

## Speed levers discussed with Nicabar (not yet sent to the Lead Agent as design-note edits)

**Core insight:** the speed ceiling for Pi Durable is the ledger, not the family model. Every wake runs check-then-weigh-then-act through Mother, so even a trivial continuation pays the full grant round-trip — that is the tax. A teenager Child does not buy speed: it adds another ledger-gated node and the tax compounds. An uncle or aunt is the same story. The family model is the authority structure, not the performance lever.

**Proposed levers (not accepted, not executed):**
(a) Fewer wakes via batching — one wake covering more work.
(b) Caching session state so Mother is not re-weighing everything each wake.
(c) Pre-authorizing a small class of low-risk actions so Mother does not weigh them at all.
(d) A local scratchpad for session state instead of re-reading the ledger every wake.

**Pending:** last offer on the call was to have the Lead Agent fold these into the design note alongside the Limitations section. Not yet accepted or executed. Do not fold them into pi-durable-child-design.md in this turn; only record them here as pending.

## Conflicts with (a)–(f)

The four levers stay pending. This note does not accept them and does not fold them into the design note. Locked (a)–(f) stay as written: Mother is the only grantor, with the recovery grant as the revoke exception; iroh is the only admin path; Mother unreachable is a hard stop for a Child's work; a platform-given handle is the authorization and Mother mints every borrowed handle; wake check-in comes before the Child touches its own state, and a freeze resume counts as a wake; one pause, check then weigh then act, finish the current step, freeze at the next checkpoint.

**Lever (c).** Pre-authorizing a class of actions so Mother does not weigh them at all conflicts with (f) if that class skips the pause. (f) is one pause, and the check comes before weigh and act. A young Child's pause is that check with no weighing. Skipping the weigh is not skipping the check. This note does not decide that "does not weigh" only skips the weigh. If the Child is the one pre-authorizing, that is a second grantor under (a) and the closed Child-as-grantor rule. If the pre-authorization is a way in without a platform-given handle, it conflicts with (d). If those actions touch session state before a check-in, they conflict with (e). Left pending.

**Lever (d).** A local scratchpad instead of re-reading the ledger every wake conflicts with the ledger as the system of record. Session writes in Part 1 are grant-gated Toy calls that wait for the (e) check-in, not a direct mutate. A scratchpad Mother or the host keeps, so the wake need not read the ledger, is a second store of the Child's session and is Mother holding that state. A scratchpad the Child writes locally is the in-place mutate Part 1 already withholds. This note does not pick which scratchpad the lever is. Left pending.

**Lever (a).** One wake covering more work conflicts with (f) if that work runs past the next checkpoint without the pause. (f) finishes the current step and freezes at the next checkpoint. A batch that stays inside the current task and still pauses there does not. This note does not decide which reading the lever is. Left pending.

**Lever (b).** A cache of session state so Mother is not re-weighing each wake conflicts with (e) if the cache stands in for the check-in. The check-in is what proves policy is current before the Child touches its own state. (f) still runs check, then weigh, then act. This note does not decide that the cache is only a weigh shortcut. Left pending.

**Uncle / aunt.** Part 2 already names the conflict with (c), (d), and the closed Child-as-grantor rule. The recommendation on the call is to use the older Child. That does not add a second grantor and does not rewrite (c) or (d).

**Fingerprint rule.** Part 1 records that a changed harness hash breaks runs already in flight. (e) says a freeze resume is a wake. (f) says the Child continues its current task. What the Child does after the refusal is not ruled here.

**Run credential.** Part 1's short-lived run credential says Mother still re-verifies. A credential that authorizes a call without the platform-given handle conflicts with (d). This note does not decide that the credential is that handle.

(b) is untouched: none of these levers adds an admin transport. (a)'s recovery-grant exception is untouched: none of them revokes that grant.
