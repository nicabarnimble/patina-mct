# Core concepts

## Mother

Mother is the authority of one deployable node. She owns authority decisions,
the observation ledger, Child execution, ToyGrant evaluation, and peer
protocols. Mother is a role, not merely the daemon process.

## Child

A Child is an application component identified by its WIT contract. WASM is
the design center. Process-backed and JVM-backed Children can participate in
the same WIT-shaped call and admission model, but that does not itself provide
OS confinement. The current process harness authorizes launch without
sandboxing the resulting process. A manifest's `needs` entry requests
capability; it grants nothing.

## Toy

A Toy is a host capability through which a Child affects the world. Toys form
a closed canonical catalog. In the WASM/WIT path, a mediated host effect can
proceed only after Mother evaluates an explicit, scoped, revocable ToyGrant
against the current call.

## Four lifecycle facts

```text
ComponentArtifact  what the code is
ChildApproval      whether it may be used
ChildAssignment    where it may run
ChildInstance      what is running now
```

Existence is not permission. Permission is not placement. Placement is not
readiness.

## Calls and observations

An accepted call is filtered by authority before any route is ranked. Runtime
optimization may choose only among already-authorized candidates. MCT
authority decisions and mediated effects emit typed observations into an
append-only, hash-chained ledger; logs, metrics, traces, and dashboards are
projections of that ledger. The ledger does not observe arbitrary OS activity
performed by an unsandboxed process.

For the crate and protocol boundaries, continue to
[Architecture](../architecture/index.md).
