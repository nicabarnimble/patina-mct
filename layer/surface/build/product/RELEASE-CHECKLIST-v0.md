# MCT v0 replacement release checklist

Scope: replacement-ready runtime core for local Mother/Child/Toy work. This is not a full Patina product release and does not include Belief/scry/assay/session/interface orchestration.

## Required closeout gates

- [x] Workspace tests pass: `cargo test --workspace`.
- [x] Clippy passes with warnings denied: `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Tier-0 script passes at final closeout: `./scripts/ci-tier0.sh`.
- [x] Allium product map parses with zero findings in final tier-0 run.
- [x] Active spec closeout: `mct-typed-wit-runtime-parity` is marked complete after 7/7 criteria passed.

## Runtime replacement security gates

- [x] Resident Mother requires signed peer binding presentations; unsigned/tampered proofs deny before hello admission.
- [x] Peer endpoint identity remains transport-only; authority requires MCT peer binding plus signature proof.
- [x] Child execution still consumes kernel-minted child/route authority and effect-boundary stale-revision guards.
- [x] Payload bytes are bounded and excluded from observations.
- [x] Secret values are excluded from toy observations and persisted grant snapshots.
- [x] Secrets authority is a canonical toy/grant-backed adapter, not ambient process state.

## Operator gates

- [x] Runbook documents identity, child approval, toy grants, signed peers, resident serve, JVM bridge, and inspection.
- [x] CLI help exposes peer signature refs, `toys authorize-secret`, and `jvm call-json`.
- [x] Start/stop v0 boundary is explicit: run `mct-daemon serve`; stop with SIGINT/SIGTERM or external supervisor wrapper.

## Deferred beyond v0

- [ ] System supervisor install/uninstall wrappers.
- [ ] Full JVM SDK/client distribution beyond `jvm call-json`.
- [ ] Interface launcher/session/HITL orchestration.
- [ ] Multi-Vision publication and cross-Mother remote route forwarding.
- [ ] Iroh blob transfer between Mothers and storage/network toy breadth.
