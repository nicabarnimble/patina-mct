# Spec-drift audit tasks

## Operator prompt (verbatim)

```text
You are running a spec-drift audit ("weed pass") of patina-mct. Since
the product map was elicited (2026-05-29), six phases of implementation
have landed without the spec ever being audited against them: resident
Mother daemon, payload data plane + local CAS, route wiring, typed-WIT
runtime parity, binding signature verification, and single-hop
multi-Mother forwarding. Your job is to find where
layer/allium/mct-product-map.allium and the implementation have
diverged, and report — this is an AUDIT, not a fix pass.

HARD CONSTRAINT: no code changes, no spec changes, no test changes.
The only files you create/modify are the report and its TASKS entry.
Resolution decisions are operator work at the gate after your report.

## Step 0 — Re-establish state (STOP and report if anything differs)

a) Branch `patina`, expected HEAD bcb4778 (docs: close multi-mother
   forwarding phase). Tree clean. If your session flow needs to commit
   session artifacts first, do that via the normal flow.
b) Confirm tooling: `allium --version` (expect 3.2.3), then run
   `allium check layer/allium/mct-product-map.allium` and
   `allium analyse layer/allium/mct-product-map.allium`. Record both
   outputs in the report; analyse findings (missing producers, dead
   transitions, deadlocks) are audit input, not things to fix.
c) Read: layer/core/what-is-mct.md, the full
   layer/allium/mct-product-map.allium, and each phase's SPEC.md under
   layer/surface/build/feat/ (resident-mother, payload-data-plane,
   route-wiring, mct-typed-wit-runtime-parity,
   multi-mother-route-forwarding) plus ROADMAP.md items 1-4 completion
   notes.

## Working principles (binding)

Always read code before asserting divergence — every finding cites
both the spec location (line or invariant/contract name) and the code
location (file:line), with the evidence stated. No speculative
findings: if you cannot point at code, it is not a finding. Scalpel
commits: the report lands as one commit, TASKS updates included.
Validation green after the commit:
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/ci-tier0.sh
(the audit changes no code, so this confirms a clean baseline).
Flake protocol: capture any failure verbatim before rerunning.

## The audit

Create layer/surface/build/spec-drift-audit/TASKS.md (this prompt
verbatim + checklist) and REPORT.md. Walk the product map section by
section — peer admission/hello, call protocol, payload/blobs, the
TwoPhaseRouting and NoRouteDecision contracts, NodeProfileAndTelemetry,
capability publication, ALPN/protocol decisions, child lifecycle,
observation/ledger decisions — and correlate each Decision, Principle,
invariant, and contract against the implementation. Classify every
divergence:

- CLASS A — code violates a spec decision or invariant. Highest
  priority; state the violated text, the code, and the observable
  consequence. If you believe a Class A is actually the spec being
  wrong, say so, but it stays Class A until the operator rules.
- CLASS B — spec under-describes what now exists: implemented behavior
  the map does not mention or describes too thinly (examples to check,
  do not assume: inline payload caps and the CAS; the effect-boundary
  revision guard; route_taken projection rules; structural/temporal
  denial classes; Ed25519 binding verification shape).
- CLASS C — spec describes what is not yet built (mct/thought/0,
  mct/observe/0, mct/federation/0, telemetry inputs, retry/grant-
  request/escalation...). Not drift; list them so the operator can
  confirm they are still intended, with any that new work has made
  stale flagged.
- CLASS D — semantics that exist ONLY in code, with no spec statement
  anywhere: peer-relationship rules the recent phases created.
  Verify and include at minimum: publication of a callable surface
  implies local execution ("publication means I execute");
  mct/call/0 arrivals are terminal by construction (single-hop
  forwarding, ResidentRemoteCandidateSource origin gating);
  forwarding rewrites caller identity (per-hop accountability model).
  Class D findings are input to a planned peer-ontology elicitation —
  describe the code semantics precisely; do not draft spec language.

For each finding: id (A1, B2, ...), spec ref, code ref(s), evidence
(one paragraph, quote the spec text), and a suggested resolution
DIRECTION (spec-ward, code-ward, or elicitation) — direction only, no
edits. End the report with a summary table ordered Class A first, and
a count per class.

## Boundary

One commit: `docs: report spec drift audit` (REPORT.md + TASKS.md).
STOP after it. Final report back: the summary table verbatim, the
allium check/analyse outputs, anything that could not be classified,
and validation results. Do not resolve findings, do not push, do not
open a PR.
```

## Operator amendment (verbatim)

```text
Step 0 mismatch resolved: the 3.2.3 expectation was stale operator-side
information — the CLI was updated on this machine after the prompt was
written. Allium 3.5.0 is ACCEPTED as the audit toolchain; your check
and analyse outputs are clean under it, so record those as the
baseline.

One addition from the mismatch: scripts/install-allium-ci.sh pins
ALLIUM_VERSION=3.2.3, so CI validates with an older CLI than local.
Record this in the report as a tooling finding (its own line in the
summary, no class needed) with a suggested resolution direction of
bumping the pin in a separate commit at the gate. Do not change the
script during the audit.

Proceed with the audit as prompted, expected versions updated:
allium 3.5.0 local, 3.2.3 pinned in CI.
```

## Checklist

- [x] Re-established branch `patina`, HEAD `bcb4778`, and a clean starting tree.
- [x] Accepted local Allium 3.5.0 under the operator amendment.
- [x] Ran and recorded clean `allium check` and `allium analyse` output.
- [x] Read the product authority sources, phase specifications, and ROADMAP completion notes.
- [x] Audited peer admission and hello behavior.
- [x] Audited `mct/call/0`, payload handles, inline payloads, and local CAS behavior.
- [x] Audited two-phase routing, no-route behavior, revalidation, and route projections.
- [x] Audited node profile/telemetry intent and capability publication.
- [x] Audited ALPN/protocol coverage and multi-Mother forwarding relationships.
- [x] Audited child/component lifecycle and typed-WIT/JVM alignment.
- [x] Audited observation coverage, ordering, and local ledger behavior.
- [x] Classified every evidence-backed item as A, B, C, D, or standalone tooling.
- [x] Cited both product-map and implementation locations for every finding.
- [x] Created only `REPORT.md` and `TASKS.md`.
- [x] Prepared the single audit commit and post-commit validation gate.
