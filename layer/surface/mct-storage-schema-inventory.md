---
id: mct-storage-schema-inventory
layer: surface
status: active
created: 2026-06-28
tags: [mct, storage, sqlite, mother-parity]
references: [mct-storage-core, mct-build-boundaries, adapter-pattern]
---

# MCT Storage Schema Inventory

Purpose: record the private MCT state store shape and the existing-Mother tables intentionally not copied into MCT core.

## Included in `.mct/state.sqlite`

MCT stores runtime facts that support Mother/Child/Toy authority and adapter recovery:

- `component_artifacts` — immutable child artifact identity, hashes, WIT export shape, runtime shape, verification status.
- `child_approvals` — explicit Mother approval state for artifacts.
- `child_assignments` — Vision/Node/project placement, guarded by SQL triggers requiring approved verified artifacts.
- `child_instances` — live generation/readiness state, guarded by SQL triggers requiring active assignments for ready instances.
- `peers` — peer address-book/binding projection for MCT-over-Iroh calls.
- `runtime_runs` and `runtime_run_observations` — runtime call/result snapshots and linked observations.
- `child_state`, `child_checkpoints`, `child_subscriptions`, `child_offsets` — child state/task-cycle support.
- `runtime_tasks` — queued/leased/running task records with dedupe key guardrail.
- `metric_points` — observation-derived metric snapshots.
- `child_registry_sources` — registry sync source state.
- `composition_runs` — pando/composition run records.
- `toy_catalog_contracts` — canonical toy contract snapshots.
- `toy_grant_snapshots` — durable ToyGrant snapshots, guarded by SQL triggers requiring authority-bearing catalog contracts for active grants.

## Intentionally excluded from MCT core

These existing integrated-Mother surfaces are reference material, not MCT-core state:

- Belief graph/index tables (`scry`, `assay`, `oxidize`, `scrape`).
- Session markdown/artifact tables.
- View-buffer/UI rendering tables.
- Broad API runtime/session state bags.
- Interface launcher/HITL transcript state beyond explicit future launcher slices.
- Domain/business application tables that belong behind child/toy contracts.

## Design rule

The storage adapter persists facts and enforces relational guardrails. It does not grant authority. Authority still comes from kernel records such as `ChildApproval`, `ChildAssignment`, `MctPeerBinding`, `ToyGrant`, `AuthorizedChildInvocation`, `AuthorizedToyCall`, and `AuthorizedRouteExecution`.
