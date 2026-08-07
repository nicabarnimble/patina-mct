---
id: reliability-doctrine
layer: core
status: active
created: 2026-08-06
revised: 2026-08-06
tags: [reliability, durability, authority, testing, operations]
references: [safety-boundaries, spec-driven-design, dependable-rust, grants-authority-v0, ledger-commit-recovery]
ratification: phase-j-g1
---

# MCT Reliability Doctrine

**Purpose:** Preserve the reliability law established by Phases G through I as a binding design constraint for every later MCT change.

**Ratification:** The operator accepted this document as written at Phase J Gate G1. It is active doctrine, not aspirational guidance; changes require an explicit governed amendment.

## Commit Means Surviving Canonical Truth

A fact is committed when either:

1. its durability acknowledgement succeeds; or
2. recovery finds its complete framed, identity-valid, sequence-valid, hash-valid entry in the maximal surviving validated prefix.

Caller acknowledgement is evidence about commitment, not the definition of commitment. A complete surviving valid frame remains canonical when acknowledgement was lost.

## Classify Failure; Never Guess

Failure is classified rather than collapsed into a stuck or silently repaired state:

- an unterminated final frame is preservable residue;
- terminated malformed data, broken identity, sequence, or hash is corruption;
- missing continuity with prior durable evidence requires an authenticated operator gate; and
- structurally or semantically unsafe history enters quarantine or degraded deny.

Never skip corruption, invent continuity, treat unreadability as absence, destroy forensic bytes, or rewrite committed facts. Preserve evidence before recovery changes residue placement. Unknown authority or durability state fails closed until the named recovery proof succeeds.

## Provenance Belongs in Types

Caller context and Mother authority are different facts with different provenance. APIs and types must preserve that distinction:

- caller-carried authority is expectation and correlation evidence;
- executing-Mother authority comes only from locally verifiable current policy, canonical grants, Mother time, and exact source proof; and
- no constructor, conversion, copied revision, or context bag may turn caller evidence into local authority.

The kernel decides from explicit authority values. Adapters perform only after receiving the corresponding admitted capability.

## Testability Is a Design Requirement

Every new long-lived writer, scheduler, supervisor, projection, recovery loop, or authority boundary ships with its injection seams. Tests must be able to control clocks, durability failures, commit uncertainty, projection lag, ordering points, process/adapter starts, and shutdown without retrofitting production-only hooks later.

A design that cannot deterministically reproduce its crash, race, stale-state, and shutdown boundaries is incomplete.

## Optimize Under the Net

Behavioral law and adversarial proof coverage precede restructuring. Performance work begins only after the current production path is covered, profiles that covered revision, and preserves the same proof obligations while changing mechanics.

Do not add caches, batching, group commit, or alternate authority sources to make an unproved path fast. First establish the net; then optimize beneath it.

## Durability Classes Are Specification Decisions

`BeforeEffect`, buffered, projection-only, and any future durability class express observable product guarantees. They are selected in specifications and reviewed at boundary call sites, never chosen ad hoc by an adapter or storage helper.

Canonical commitment precedes reconstructable projection and protected effects. Projection failure never undoes commitment and never creates a second source of truth.

## Standing Obligations

Changes to the physical write path carry their assurance work with them:

- generate and test crash states at every write, frame, acknowledgement, preservation, and publication boundary;
- fuzz strict parsers and framing/identity/sequence/hash validation with the changed format or parser;
- preserve exact failing artifacts and seeds; and
- measure coverage at each phase close-out and ratchet it forward rather than resetting the baseline.

These obligations apply to the change that creates the risk. They are not deferred cleanup.
