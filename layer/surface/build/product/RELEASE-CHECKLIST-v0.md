# MCT v0 replacement release checklist

Scope: replacement-ready runtime core for local Mother/Child/Toy work. This is not a full Patina product release and does not include Belief/scry/assay/session/interface orchestration.

## Required closeout gates

- [x] Workspace tests pass: `cargo test --workspace`.
- [x] Clippy passes with warnings denied: `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Tier-0 script passes at final closeout: `./scripts/ci-tier0.sh`.
- [x] Allium product map parses with zero findings in final tier-0 run.
- [x] Typed-WIT closeout: `mct-typed-wit-runtime-parity` is marked complete after 7/7 criteria passed.
- [x] Resident ingress closeout: `resident-call-ingress` is marked complete after 9/9 criteria passed, including authenticated UDS calls, durable replay/reopen proof, and real resident status.

## Runtime replacement security gates

- [x] Resident Mother requires signed peer binding presentations; unsigned/tampered proofs deny before hello admission.
- [x] Peer endpoint identity remains transport-only; authority requires MCT peer binding plus signature proof.
- [x] Child execution still consumes kernel-minted child/route authority and effect-boundary stale-revision guards.
- [x] Payload bytes are bounded and excluded from observations.
- [x] Secret values are excluded from toy observations and persisted grant snapshots.
- [x] Secrets authority is a canonical toy/grant-backed adapter, not ambient process state.

## Operator gates

- [x] Runbook documents identity, child approval, toy grants, signed peers, resident serve, authenticated UDS `POST /calls`, resident-backed status, compatibility JVM evidence, and inspection.
- [x] CLI help exposes peer signature refs, `toys authorize-secret`, compatibility `jvm call-json`, and resident-backed `status`; the owner-authenticated UDS exposes the production local call ingress.
- [x] Start/stop v0 boundary is explicit: run `mct-daemon serve`; stop with SIGINT/SIGTERM or external supervisor wrapper.

## Completed v0 distributed execution

- [x] Single-hop cross-Mother route forwarding with bilateral authority, fresh publication evidence, current revalidation, typed replies, and two-ledger observations.

## Deferred beyond v0

- [ ] System supervisor install/uninstall wrappers.
- [ ] Full JVM SDK/client distribution beyond `jvm call-json`.
- [ ] Interface launcher/session/HITL orchestration.
- [ ] Multi-Vision publication and transitive/brokered routing; implemented `mct/call/0` forwarding remains deliberately single-hop.
- [ ] Iroh blob transfer between Mothers and storage/network toy breadth.
