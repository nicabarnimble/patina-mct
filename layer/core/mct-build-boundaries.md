---
id: mct-build-boundaries
layer: core
status: active
created: 2026-05-31
revised: 2026-05-31
tags: [mct, build, authority, iroh, wasm, observations]
references: [spec-driven-design, adapter-pattern, dependable-rust, safety-boundaries, mct-builds-on-iroh-substrate]
---

# MCT Build Boundaries

**Purpose:** State what Patina MCT is building and what must stay out of the first implementation slice.

---

## Product Shape

Patina MCT is a whole-app, local-first Mother/Child/Toy runtime over Iroh.

```text
Mother  = local authority, ledger, peer protocols, runtime host
Child   = WIT/WASI component or WIT-shaped adapter
Toy     = explicit host capability governed by ToyGrant
Iroh    = public-key-addressed connectivity substrate
Ledger  = local-first MctObservation truth
```

MCT is not a generic plugin loader, not an Iroh fork, not a SaaS control plane, and not a Belief/scry/assay runtime.

## Boundary Laws

### 1. Mother owns authority

Mother decides:

- peer admission;
- call authorization;
- child assignment;
- ToyGrant evaluation;
- data movement;
- secret access;
- observation visibility.

Adapters do not decide these things.

### 2. Iroh owns connectivity

Iroh/noq owns:

- endpoint key transport identity;
- QUIC streams;
- relay/direct path behavior;
- NAT traversal;
- multipath/path continuity;
- discovery reachability;
- path facts and metrics.

MCT owns:

- `MctPeerBinding`;
- `mct/hello/0`;
- `mct/call/0`;
- Vision scope;
- ToyGrants;
- observations;
- thought semantics.

### 3. Children use WIT/Toys

Children do not receive raw substrate power. They call WIT interfaces and receive host powers only through `ToyGrant` evaluation and kernel-minted `AuthorizedToyCall` capabilities.

### 4. Observations are first-class

MCT does not rely on logs as truth. Authority decisions and adapter effects create `MctObservation` facts that can reconstruct traces.

### 5. Build the vertical spine first

The first implementation should prove:

```text
kernel records
  → observation ledger
  → peer binding evaluation
  → mct/hello/0 admission
  → mct/call/0 to fake handler
  → caller-safe result
  → trace reconstruction
```

Only after that should we deepen thought mesh, federation, additional storage backends, rich CLI surfaces, or further runtime splits.

## Do Not Build Yet

Until separately specified:

- full thought mesh replication;
- production relay fleet management;
- ECH/OHTTP privacy edge;
- generalized plugin marketplace;
- raw Iroh access for children;
- full banking/domain-specific model;
- multiple storage backends;
- complex UI/inspector surfaces;
- additional WASM/runtime backend depth beyond the current typed WIT and process slices.

## First Implementation Checklist

Before writing code for a slice:

- [ ] Which Allium anchors authorize it?
- [ ] Which Slate work item owns it?
- [ ] Which belief/evidence explains it?
- [ ] Which observations prove it?
- [ ] What is the smallest integration test?
- [ ] What stays fake/concrete until the seam is proven?

## References

- [Spec-Driven Design](./spec-driven-design.md)
- [Adapter Pattern](./adapter-pattern.md)
- [Dependable Rust](./dependable-rust.md)
- [Safety Boundaries](./safety-boundaries.md)
- [mct-builds-on-iroh-substrate](../surface/epistemic/beliefs/mct-builds-on-iroh-substrate.md)
