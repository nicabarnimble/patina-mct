---
id: spec-driven-design
layer: core
status: active
created: 2026-05-31
revised: 2026-10-18
tags: [governance, decisions, specifications, mct, build-process]
references: [dependable-rust, unix-philosophy, session-capture, mct-build-boundaries, mct-builds-on-iroh-substrate]
---

# Decision- and Evidence-Driven MCT Build

**Purpose:** Keep MCT implementation subordinate to settled product decisions,
reviewable work boundaries, and executable evidence without requiring a
separate specification language in the development path.

---

## Core Principle

MCT has four complementary design-time surfaces:

```text
DECISIONS.md says what is settled and its status.
An active ratified slice specification says what this change may do.
Beliefs, research, and sessions preserve why.
Executable tests prove what the implementation actually does.
```

Code executes inside those boundaries. A test passing cannot silently reverse
a settled decision, and a design note cannot claim behavior that has not been
built and tested.

The current governance surface is:

- `docs/DECISIONS.md`
- active specifications under `layer/surface/build/feat/`
- `layer/core/*.md`
- `layer/slate/work/*/work.toml`, when Slate tracking is used
- `layer/surface/epistemic/beliefs/*.md`
- focused evidence and session records under `layer/surface/` and
  `layer/sessions/`

The former behavior maps under `layer/allium/` are frozen historical evidence.
They are not a current authority source or CI gate.

Sessions are evidence and memory. They are not enough by themselves to
authorize a non-trivial implementation change.

## MCT Build Contract

Before building a non-trivial feature, identify:

1. **Decision** — which settled decision permits the behavior, or which new
   decision must be recorded?
2. **Work boundary** — which active slice specification or focused task owns
   the change and its non-goals?
3. **Evidence** — why is this direction chosen?
4. **Expected observations** — which `MctObservation` facts prove it happened?
5. **Exit test** — what command or integration test proves the slice?

A small, reversible, read-only experiment may begin before a full slice exists.
It must remain clearly experimental and gain a decision/specification before it
changes authority, persistence, compatibility, or externally supported
behavior.

## Current Implementation Authorization

Implementation should start where decisions and current slices are already
locked:

```text
mct-kernel domain records and evaluations
MctObservation ledger validation and projections
MctPeerBinding evaluation
mct/hello/0 admission gate
mct/call/0 peer call envelope
typed WIT Child invocation with Toy-gated host effects
```

Do not jump straight to:

- full thought mesh replication;
- arbitrary plugin frameworks;
- generalized storage abstraction;
- unapproved runtime breadth;
- relay fleet orchestration;
- privacy/ECH architecture;
- raw Iroh handles exposed to Children.

Those require explicit decisions and bounded implementation slices first.

## Change Pipeline

```text
problem or evidence
  → decision or bounded slice update, when needed
  → implementation
  → executable tests and repository checks
  → documentation/provenance update
  → commit and pull request
```

Use the shortest path that preserves the relevant guarantee. Do not create a
new governance artifact for a mechanical refactor or a disposable read-only
experiment when existing tests and documentation make the boundary clear.

For Slate changes, validate TOML before commit. For code changes, run the
workspace checks named by the active slice and CI.

## When to Stop and Ask

Stop and ask rather than guessing when implementation reveals:

- a missing authority rule;
- a new denial/retry class;
- a new observation kind;
- a new ToyGrant category;
- a Child needing raw substrate access;
- a conflict between Iroh convenience and MCT authority;
- a need for a trait with only one implementation;
- a privacy/security implication absent from settled decisions and the active
  slice.

The normal fix is to record the decision or amend the active slice, then
continue. The fix is not to add ceremony unrelated to the discovered risk.

## Decision Provenance

Meaningful authority code should trace like this:

```text
code and executable test
  → commit / pull request
  → active slice or focused task
  → settled decision
  → supporting evidence and discussion
```

This matters most for:

- peer admission;
- route authorization;
- ToyGrant checks;
- data movement;
- Child assignment;
- secret access;
- observation durability.

## Common MCT Mistakes

### Treating a conversation as a decision

Wrong:

```text
We discussed thought mesh, so implement a gossip protocol.
```

Right:

```text
Record the accepted behavior and non-goals in a decision or bounded slice.
Then implement the smallest proven path.
```

### Letting an adapter define the domain

Wrong:

```text
Iroh has EndpointId, so EndpointId is MCT identity.
```

Right:

```text
EndpointId is transport identity.
MctPeerBinding is MCT authority identity binding.
```

### Building a framework before the vertical slice

Wrong:

```text
Design a complete plugin/runtime abstraction before one peer call works.
```

Right:

```text
Build hello → admitted call → fake handler → observation ledger.
Then generalize only where pressure proves the seam.
```

## Frozen Allium Material

The files under `layer/allium/` remain in Git so earlier commits, session
records, and audit citations stay understandable. They are not updated for new
features, required by CI, or consulted as an override over current decisions.
Restoring them to the active path requires a new explicit governance decision.

## References

- [`docs/DECISIONS.md`](../../docs/DECISIONS.md)
- [MCT Build Boundaries](./mct-build-boundaries.md)
- [Dependable Rust](./dependable-rust.md)
- [Adapter Pattern](./adapter-pattern.md)
- [Unix Philosophy](./unix-philosophy.md)
- [Session Capture](./session-capture.md)
- [mct-builds-on-iroh-substrate](../surface/epistemic/beliefs/mct-builds-on-iroh-substrate.md)
