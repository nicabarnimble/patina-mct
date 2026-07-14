# MCT `patinaMother` replacement finish TODO — 2026-07-09

Goal: make MCT replacement-ready for the `patinaMother` runtime surface, with an explicit v0 boundary. Code must stay authority-first: kernel decides, adapters perform, and Patina belief/scry/assay semantics do not become `mctMother` internals.

## Definition of done today

- [x] No active spec remains whose exit criteria are already satisfied.
- [x] Binding presentations are cryptographically verified before peer admission in the resident `mctMother` path; unsigned or invalid bindings fail closed.
- [x] A minimal secrets authority exists as an explicit toy/grant-backed adapter with redacted observations.
- [x] Operator lifecycle is clear enough for `mctMother` to replace `patinaMother` for local runtime work: identity, config, peers, children, toys, serve/status/stop docs or commands.
- [x] JVM bridge status is either implemented as a minimal call-envelope ingress or explicitly deferred behind a checked, documented non-blocking v0 boundary.
- [x] `mct-interface-launcher-control` and release hardening have an explicit unblock/close path.
- [x] `./scripts/ci-tier0.sh` passes at closeout.

## Today sequence

### 0. Bookkeeping and truth alignment

- [x] Complete `mct-typed-wit-runtime-parity` after confirming `patina spec check mct-typed-wit-runtime-parity --json` is green. Note: `patina spec complete` failed in spec-manager due unrelated historical SPEC/frontmatter parsing and workspace-version assumptions, so `SPEC.md` was closed directly after 7/7 check passed.
- [x] Resolve/commit or intentionally leave documented the untracked session file. Decision: keep `layer/sessions/20260709-091408-460632000.md` in the commit-ready set as the Pi session record.
- [x] Keep this TODO updated as tasks land.

### 1. Binding signature verification — P0 runtime gate

- [x] Define canonical peer-binding signature payload and `signature_ref` format.
- [x] Add signing/verifying helpers using the `mctMother` Iroh endpoint key.
- [x] Persist peer binding signature refs in config/address-book entries.
- [x] Send `signature_ref` in CLI/configured hello requests.
- [x] Deny resident hello admission when signature is missing, malformed, or invalid.
- [x] Tests: valid signed binding admits; missing/invalid signature denies with safe `not authorized` and internal `CapabilityInvalid`.

### 2. Secrets authority — P0 replacement gate

- [x] Add a closed canonical secrets toy identity.
- [x] Add local secret record/scope model without storing values in observations.
- [x] Implement minimal get/use path through `ToyGrant`/`AuthorizedToyCall`.
- [x] Tests: unauthorized secret access denied; authorized access succeeds; logs/ledger contain redacted markers only.

### 3. Operator lifecycle / packaging — P0 operator gate

- [x] Document exact replacement workflow: init identity, approve child, authorize toys, add signed peer, serve, inspect status.
- [x] Add/adjust CLI help if any required command is missing for the workflow.
- [x] Decide whether start/stop/install/uninstall are in-scope today or documented as wrapper/system-supervisor follow-up.

### 4. JVM bridge — P1/v0 boundary

- [x] Decide today: minimal stdin/stdout or HTTP/UDS bridge ingress vs defer.
- [x] If implementing: submit one call envelope into existing resident route path and receive caller-safe reply.
- [x] If deferring: mark the epic with explicit v0 non-blocker rationale and release follow-up.

### 5. Interface launcher and release hardening

- [x] Unblock or reclassify `mct-interface-launcher-control` relative to secrets authority.
- [x] Update release hardening blocked_by/block_reason so it matches current roadmap reality.
- [x] Final validation: `./scripts/ci-tier0.sh`.

## Validation notes

- 2026-07-09 final `./scripts/ci-tier0.sh` first attempt failed before tests because `run_jvm_call_json` used test-only `ResidentRequestPayload::local`; fixed to use non-test `ResidentRequestPayload::remote` for inline JVM payloads.
- 2026-07-09 final `./scripts/ci-tier0.sh` passed after the fix.
- 2026-07-09 `patina spec check mct-typed-wit-runtime-parity --json` passed 7/7.

## Current verified baseline

- CI tier-0 green before this TODO was created and green again at closeout.
- Slate baseline before work: 76 complete/completed, 3 active, 2 blocked.
- After today's closeout: 79 complete/completed, 0 active, 0 blocked, 2 paused/deferred (`mct-interface-launcher-control`, `mct-release-hardening`).
- Remaining non-v0 follow-ups: network/storage toy breadth, multi-Vision publication/remote forwarding, full JVM SDK packaging, interface launcher/session orchestration, production release perf/supervisor packaging.
