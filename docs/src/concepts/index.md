# Core concepts

## Mother

Mother is the authority of one deployable node. She owns authority decisions,
the observation ledger, Child execution, ToyGrant evaluation, and peer
protocols. Mother is a role, not merely the daemon process.

## Child

A Child is an application component identified by its WIT contract and
executed locally as a WASM component. Native and JVM software can use the same
WIT-shaped call model as an external caller or remote workload, but it does
not become a local Child. A manifest's `needs` entry requests capability; it
grants nothing.

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
projections of that ledger.

For the crate and protocol boundaries, continue to
[Architecture](../architecture/index.md).
