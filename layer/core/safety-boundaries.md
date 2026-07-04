---
id: safety-boundaries
layer: core
status: active
created: 2026-05-31
revised: 2026-05-31
tags: [safety, security, mct, authority, runtime]
references: [spec-driven-design, mct-build-boundaries, iroh-endpointid-is-transport-identity]
---

# MCT Safety Boundaries

**Purpose:** Make the safe default obvious before MCT starts running peers, children, toys, and adapters.

---

## Core Principle

MCT is an authority system before it is a networking system.

```text
If authority is unclear, deny.
If observation is required and unavailable, fail closed.
If a child asks for host power, require a ToyGrant.
```

## Hard Safety Rules

### 1. EndpointId is not authority

Iroh proves endpoint key possession. It does not grant MCT node identity, Vision membership, data access, child access, observation access, or Toy rights.

Use `MctPeerBinding` and `mct/hello/0` admission.

### 2. Children do not get raw host power

Children do not receive raw Iroh endpoints, raw filesystem roots, raw secrets, raw process handles, or raw database connections by default.

They receive WIT/Toy access only after `ToyGrant` evaluation mints an executable capability such as `AuthorizedToyCall`.

### 3. Observations before protected effects

Authority-critical effects require observations before the effect proceeds:

- peer admission/rejection;
- hello response;
- call authorization/denial;
- ToyGrant allow/deny;
- secret access;
- data movement;
- child assignment and invocation.

If the required observation cannot be durably recorded or reserved, the safe default is deny/fail closed unless an explicit degraded policy says otherwise.

### 4. Adapter errors are not invisible

Adapter errors must become observations or health degradation:

- Iroh stream reset;
- relay/path failure;
- WASM trap;
- process exit;
- storage append failure;
- telemetry export failure;
- toy backend failure.

### 5. No unsafe Rust by default

`unsafe` requires an explicit design reason, localized module boundary, test coverage, and review. The default MCT build should not need unsafe code.

### 6. No destructive or external operations without consent

Before mutating external state, starting daemons, using network services, pushing branches, deleting files, or running long background jobs, get explicit user consent unless the user already requested that exact operation.

## Privacy Boundaries

- `MctObservation` is local-first truth; exports are projections.
- Caller-safe responses must not reveal topology, candidate elimination, policy internals, secrets, or child inventory.
- Public relays, managed relays, self-hosted relays, and direct paths differ in metadata exposure; relay choice is not authority.
- Thought/observation replication must be Vision/audience-filtered.

## Build-Time Application

Before implementing a path, ask:

1. What authority fact allows it?
2. What observation proves the decision?
3. What adapter performs the effect?
4. What happens if observation or adapter effect fails?
5. What does the caller safely learn?

If those answers are absent, update Allium/Slate before coding.

## References

- [Spec-Driven Design](./spec-driven-design.md)
- [MCT Build Boundaries](./mct-build-boundaries.md)
- [iroh-endpointid-is-transport-identity](../surface/epistemic/beliefs/iroh-endpointid-is-transport-identity.md)
