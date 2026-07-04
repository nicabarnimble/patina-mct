---
id: spec-driven-design
layer: core
status: active
created: 2026-05-31
revised: 2026-05-31
tags: [governance, allium, slate, mct, build-process]
references: [dependable-rust, unix-philosophy, session-capture, mct-build-boundaries, mct-builds-on-iroh-substrate]
---

# Spec-Driven MCT Build

**Purpose:** Keep MCT implementation subordinate to the product map, Slate work, beliefs, and evidence. Do not let code outrun authority.

---

## Core Principle

MCT has three design-time authorities:

```text
Allium says what MCT is.
Slate says what work is ready.
Beliefs/evidence say why.
```

Code executes inside that boundary.

The current authoritative product surface is:

- `layer/allium/mct-product-map.allium`
- `layer/slate/work/*/work.toml`
- `layer/surface/epistemic/beliefs/*.md`
- `layer/surface/iroh-substrate-evidence.md`
- `layer/sessions/*.md`

Sessions are evidence and memory. They are not enough by themselves to authorize a non-trivial implementation change.

## MCT Build Contract

Before building a feature, identify:

1. **Allium anchors** — which entities/contracts/invariants authorize the behavior?
2. **Slate work item** — which build item owns the work and proof plan?
3. **Beliefs/evidence** — why is this direction chosen?
4. **Expected observations** — which `MctObservation` facts prove it happened?
5. **Exit test** — what command or integration test proves the slice?

If any of these are missing for non-trivial work, update the design surface first.

## Current Implementation Authorization

The next implementation should start only where the design is already locked:

```text
mct-kernel domain records and evaluations
MctObservation ledger validation and projections
MctPeerBinding evaluation
mct/hello/0 admission gate
mct/call/0 peer call envelope
typed WIT/process child invocation with toy-gated host effects
```

Do not jump straight to:

- full thought mesh replication;
- arbitrary plugin framework;
- generalized storage abstraction;
- additional runtime breadth beyond the current typed WIT/process slices;
- relay fleet orchestration;
- privacy/ECH architecture;
- raw Iroh handles exposed to children.

Those need their own Allium/Slate tightening first.

## Change Pipeline

```text
session discussion
  → Allium/Slate/belief/evidence update
  → implementation
  → tests/checks
  → session update
  → commit
```

For Allium changes, run:

```bash
allium check layer/allium
```

For Slate changes, validate TOML before commit.

For code changes, add the relevant Rust checks once crates exist.

## When to Stop and Ask

Stop and ask rather than guessing when implementation reveals:

- a missing authority rule;
- a new denial/retry class;
- a new observation kind;
- a new ToyGrant category;
- a child needing raw substrate access;
- a conflict between Iroh convenience and MCT authority;
- a need for a trait with only one implementation;
- a privacy/security implication not in Allium.

The correct fix is usually: update Allium/Slate/belief/evidence, then continue.

## Decision Provenance

Every meaningful code path should trace like this:

```text
code
  → commit
  → Slate work item
  → Allium anchor
  → belief/evidence
  → session discussion
```

This matters most for MCT authority paths:

- peer admission;
- route authorization;
- ToyGrant checks;
- data movement;
- child assignment;
- secret access;
- observation durability.

## Common MCT Mistakes

### Treating a session as a spec

Wrong:

```text
We talked about thought mesh, so implement a gossip protocol.
```

Right:

```text
Capture thought mesh behavior in Allium/Slate first.
Then implement the smallest authorized slice.
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

## References

- [MCT Build Boundaries](./mct-build-boundaries.md)
- [Dependable Rust](./dependable-rust.md)
- [Adapter Pattern](./adapter-pattern.md)
- [Unix Philosophy](./unix-philosophy.md)
- [Session Capture](./session-capture.md)
- [MCT product map](../allium/mct-product-map.allium)
- [mct-builds-on-iroh-substrate](../surface/epistemic/beliefs/mct-builds-on-iroh-substrate.md)
