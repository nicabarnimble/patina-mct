# Admin keys

> **Imported 2026-10-18** from working notes (last edited 2026-10-17). Body unchanged apart from link fixes.
> Status: decisions (a)–(f), the October 15 scheme v1 decisions, recovery-key lockout and hard cutover are **decided**. The decision model, replay Toy and WIT composition sections are **proposed** or **open** exactly as labelled inside.
> The "What exists" section describes `main` at `784c5ed` (2026-10-14). Since then: admin-key ledger facts (#48, #64), the route-scope grant check (#51), and optional hybrid scheme v1 signatures (#67) merged. The iroh admin ALPN, pairing redeem over iroh, and the hard cutover (deleting UDS / `peer_cred` / `open_local` / `execute_offline_*`) are **not built yet** on `main` at `49dfacd`. See [../DECISIONS.md](../DECISIONS.md).


Local and remote administration of a Mother uses admin keys. An admin key is an iroh/Ed25519 identity the admin side creates. Mother grants the public half, records it in the ledger, and can revoke it. Every admin call presents a granted key. Default-deny.

There is no admin-grant type in the tree today. The pieces that have to be reused, and the places that would regress, are below.

## Decided

a–f are locked. Questions they do not answer stay in [Open questions](#open-questions).

- **a. Admin keys.** The admin side makes the keypair. Mother stores only the public half. The handoff uses the existing iroh connection. Bootstrap is a pre-seeded admin public key in deploy config (`authorized_keys` style) or a single-use short-lived iroh pairing ticket redeemed over iroh. The owner-only secret file and the printed stdout enrollment token are not the bootstrap. The ticket is burned in the ledger. Both sides confirm an Ed25519+ML-DSA-65 fingerprint. Grants are scoped, ledgered, and revocable, except the recovery grant: under the lockout, nothing can revoke it. Day-to-day keys have mandatory expiry. The recovery key is the startup gate plus `key-manage` only, not every route. The admin check generalizes `evaluate_toy_grant_for_call`: the subject is a Child or an AdminKey, with route scopes. `/pando/record` stays on the admin surface and needs a scope.
- **b. Iroh only.** Iroh is the only admin transport. No UDS admin, no uid/`peer_cred` check, no `open_local`. `execute_offline_*` uses the same admin-key check or is removed.
- **c. Mother is essential.** A Child refuses to act without her. Mother unreachable is a hard stop, not failover of a Child's work. That no-failover does not cover admin recovery. The effort goes into Mother never dying: hot standby, replicated state, fast restart.
- **d. Handle.** A Child is a durable object. There is no local socket and no same-uid path. The only way in is a platform-given handle, and holding the handle is the authorization. Local calls, including `POST /calls`, move to the iroh call path. They are not left on peer credentials.
- **e. Check-in.** On wake, a Child checks in with Mother before it touches its own state. The handle proves identity. The check-in proves it is still her Child and that policy is current (revision/generation). Resuming from a freeze counts as a wake.
- **f. Checkpoints.** A Child continues its current task until it needs another one. Mother can interrupt by an iroh message, checked at step checkpoints. If Mother drops, the Child finishes the current step and freezes at the next checkpoint.

The code-change map is [admin-keys-code-map.md](admin-keys-code-map.md). Line numbers there are from origin/main `784c5ed` and will shift after #45 and #39–#44 merge.

The [family narrative](#narrative-the-family) is the picture, and the [composition note](#composition) is not a further decision. Sibling sharing is adopted as the courier rule under (a) and (d): only an older Child hands over a handle it already holds, and Mother stays the only grantor. "Do this next" and a fix are adopted under the ruling in the conflict section: the interrupt carries the next task, and a fix is a grant. "Waits for her" is the adopted resume: a freeze counts as a wake under (e). Where any other narrative line conflicts with a–f, a–f win. A [replay Toy](#proposed-replay-toy) for deterministic randomness is proposed, not locked. A [decision-model Toy](#proposed-decision-model) for older Children is proposed, not locked.

## Decisions (October 15, 2026)

These settle the two scheme v1 questions left open under [Hybrid Ed25519 + ML-DSA-65 (scheme v1)](#hybrid-ed25519--ml-dsa-65-scheme-v1) and the pairing-ticket fields. The decisions in (a)–(f) above stay as written.

1. ML-DSA-65 is load-bearing. It is the identifier that makes patina-mct distinct and keeps it safe against quantum attacks. It is not dropped, deferred, or optional.
2. The ML-DSA-65 public key lives on the admin grant fact in the ledger, next to the Ed25519 key. Both keys rotate together in one ledger write, so they cannot drift apart.
3. The hybrid signature cost is accepted, not a blocker. It is recorded as a cost only. Unsigned sizes are from 15 October 2026 on main `8684d5a`. Hybrid sizes and timings below replace that day's projections. They were measured on 17 October 2026 from real scheme-v1 facts.
   - Unsigned base, 15 October 2026: grant put 2.65–2.73 KB, revoke and ticket facts 2.4–2.5 KB each, five-fact admin session 12.7 KB.
   - Hybrid ledger, 17 October 2026. Each figure is the file-length delta of one `JsonlObservationLedger::open_authority` append (`mother-a`, `ledger-a`), newline included. KB is bytes/1000. The five facts are a recovery grant put (self-signed), a pairing-ticket issue, a pairing-ticket consume, a day-to-day grant put, and a day-to-day revoke. Recovery signs every fact. The issue fact carries a peer endpoint id and an expiry window. Grant put is 13.45 KB (day-to-day, 13445 B) and 13.52 KB (recovery, 13522 B), next to the 2.65–2.73 KB unsigned base and the 13.5 KB projection. Revoke is 9.26 KB (9257 B). Ticket consume is 9.33 KB (9328 B) and ticket issue is 9.51 KB (9505 B), next to the 2.4–2.5 KB unsigned base and the 9.3 KB projection. The five-fact session is 55.06 KB (55057 B), next to the 12.7 KB unsigned base and the 54.8 KB projection.
   - Hybrid primitives, 17 October 2026, `cargo run --release -p phase3-measure`. Workspace release profile: opt-level 3 (the profile does not set opt-level), thin LTO, one codegen unit, symbols stripped. Signer is `libcrux-ml-dsa` 0.0.11, default features off, with `std` and `mldsa65`, plus `simd256` on this x86_64 host (AVX2). Signing is the deterministic FIPS 204 variant, 32 zero random bytes. Message is 160 bytes. Percentiles are nearest-rank on sorted samples, index `round(p × (n − 1))`. Keygen p50 72.5 µs (p10 71.9, p90 80.1, 50 samples, 10 warmup). Sign, one key, p50 187.1 µs (p10 186.3, p90 191.4, 200 samples, 20 warmup). Sign across 64 keys, p50 114.5 µs (p10 76.9, p90 210.9; three warmup signs per key, then one timed sign). Verify p50 87.0 µs (p10 85.5, p90 89.8, 200 samples, 20 warmup). Wire sizes unchanged: Ed25519 public half 32 B, ML-DSA-65 public key 1952 B, hybrid signature 3374 B. Host: rustc 1.96.0, Intel Xeon under KVM, AVX2.
   - Witness bytes per run: patina-mct has no witness or head concept, so this cannot be measured.
   - The session ratio is about 4x (12.7 KB unsigned to 55.1 KB hybrid) because every admin fact carries the full 6748-character hybrid signature in hex, while wasi-pi puts its big key in only 2 of 26 entries.
   - In practice this is acceptable. Admin commands are rare. Sign p50 is 114–187 µs and verify p50 is 87 µs, and none of it is on a hot path. If size ever matters, the simplest fix is storing a digest of the signature instead of the whole signature, the same fix [`docs/QUANTUM.md`](https://github.com/nicabarnimble/patina-wasi-pi/blob/main/docs/QUANTUM.md) suggests for wasi-pi's run keys.
4. Transport stays plain Ed25519. iroh never sees the ML-DSA half. Verification stays one check.
5. Known gap: a future guardian quorum signing with FROST would not cover the ML-DSA half. This is a gap to revisit, not a contradiction.
6. The scheme v1 signature covers the command plus the grant, under both keys. The pairing ticket carries an expiry window and the peer's endpoint ID.

## What exists

### Peer bindings and proofs

`MctPeerBinding` (`crates/mct-kernel/src/peer/mod.rs`) binds an iroh endpoint id to one node, one Vision, and `allowed_alpns`. `expires_at` is mandatory. `BindingState` is `Pending | Admitted | Denied | Expired | Revoked`.

Admission is `evaluate_hello` → `evaluate_hello_internal` (`crates/mct-kernel/src/peer/internal.rs`). It denies on endpoint mismatch, missing binding, non-`Admitted` state, `expires_at <= now`, stale policy revision, Vision mismatch, and empty ALPN intersection (`accepted_alpns`). Denied `BindingState` is reported as missing, not as a distinct allow.

The proof is separate from admission. `sign_peer_binding_signature_ref` / `verify_peer_binding_signature_ref` (`crates/mct-iroh/src/identity.rs`) sign a JSON payload whose `context` is `mct-peer-binding-signature-v1`. The signature string is `mct-ed25519-binding-v1:` plus 64-byte hex. The signer must be the issuer endpoint. `enforce_hello_binding_signature` (`crates/mct-iroh/src/serve.rs`) runs only after hello already admitted, and any non-`Valid` result (missing, malformed, invalid) becomes `HelloReason::CapabilityInvalid`.

Live storage is the address book in config, not the authority ledger. `MctPeerAddressBookEntry::to_peer_binding` (`crates/mct-daemon/src/config.rs`) projects config into a binding. `MctDaemonConfigStore::upsert_peer` signs with the Mother node secret when `binding_signature_ref` is absent. `peers add` may also carry a `MotherIrohEndpointTicket` (`crates/mct-iroh/src/endpoint.rs`: endpoint id, direct addresses, relay URLs). That ticket is a dial hint. It is not an authorization.

### Grants

Three grant shapes, three stores.

| Shape | Check | Where the fact lives |
| --- | --- | --- |
| Child approval + assignment | `evaluate_child_call_authority` (`crates/mct-kernel/src/child.rs`) | Daemon config. `prepare_approved_and_assigned_child` / `prepare_revoked_child` (`crates/mct-daemon/src/config.rs`). Routes `POST /children/approve`, `POST /children/revoke`. |
| Toy grant | `evaluate_toy_grant_for_call` (`crates/mct-kernel/src/toy.rs`) | Authority ledger. `AuthorityChangeV1::ToyGrantPut` / `ToyGrantRemove`. |
| Watch scope | Watch scope state on the same authority ledger | `AuthorityChangeV1::WatchScopePut`. Routes `POST /watch/grant`, `/watch/supporting-grant`, `/watch/revoke`. |

`ToyGrantSubject` is a Child (name, artifact id, version, optional assignment and caller node). `ToyGrantScope.allowed_actions` is the capability list. `scope_matches` requires the requested action to be in that list; absent optional scope fields are wildcards. `ToyGrantConstraints.expires_at` is optional. `evaluate_toy_grant_for_call` denies unknown toy, non-authority-bearing catalog entry, policy mismatch, non-`Active` state, window miss, and scope miss. No match is `MissingGrant`. There is no allow-by-default branch.

`ChildApprovalState` is `Candidate | Approved | Blocked | Revoked | Deprecated`. Only `Approved` passes. `ChildAssignmentState` is `Active | Revoked`. Neither record has an expiry field.

Grant-shaping sources on an authority mutation (`GrantShapingSourceV1` in `crates/mct-observation/src/lib.rs`) are `OperatorDecision` (with `authenticated_principal_ref`, today `os-uid:<uid>`, and `GrantShapingCommandKindV1`: `AuthorizeSlate | AuthorizeSecret | CatalogChange | GrantChange`), `ChildApproval`, or `ChildAssignment`. No admin-key source.

`resident_mutation_handler` (`crates/mct-daemon/src/daemon/control.rs`) admits every mutation route after one uid check. Routes that are administration today:

- read: `GET /`, `/status`, `/state`, `/runs`, `/snapshot` (`handle_control_plane_path_result_with_auth` in `crates/mct-daemon/src/control.rs`)
- child: `/children/approve`, `/children/revoke`
- toy: `/toys/authorize-slate`, `/toys/authorize-secret`; watch grant/revoke; CLI `grant-watch`, `revoke-watch`, `grant-directory-read`, `grant-keyvalue`, `grant-observability`
- peer: `/peers/add`, `/peers/proof`, `/peers/revoke`, `/peers/remove`
- also behind the same uid: `/authority/import-toy-state`, `/lifecycle/fact`, `/blobs`, `/registry/install`, `/registry/sync`, `/artifacts/sources/create`, `/artifacts/sources/revoke`, `/artifacts/stage`, `/releases/acquire`, `/triggers/create`, `/triggers/revise`, `/triggers/revoke`, `/pando/record`
- `/identity/ensure` returns 409 while the daemon is running (`stop the daemon to create or rotate identity`)

`POST /calls` is local call ingress, not an admin route. It uses the same peer-credential preflight (`preflight_local_submission` in `crates/mct-daemon/src/daemon/resident/local_ingress.rs`). That is the current preflight. The end state moves local calls onto the iroh call path (d). Peer credentials do not remain an authorization path.

### Ledger append, hash, replay

`JsonlObservationLedger::append` (`crates/mct-observation/src/lib.rs`) sets `previous_entry_hash`, then `entry_hash`. `entry_hash` is blake3 of the JSON entry with `entry_hash` cleared. `append_frame` writes the line and `sync_data`s. A failed write or sync poisons the writer (`AppendCommitUnknown`).

The reader (`LedgerEntryIter::validate_entry`) recomputes the hash and checks the previous-hash link. A break quarantines the ledger.

Authority facts ride inside `MctObservation.detail_ref` with prefix `mct-authority-fact-v1:` and schema `mct-authority-fact/v1`. `replay_authority_entries` folds only `epoch_established`, `authority_mutation`, and `legacy_authority_import`. Any other `fact_kind` is `UnknownFactKind` and replay stops. Entries whose `detail_ref` lacks the prefix are skipped.

`execute_authority_mutation` replays, applies `apply_authority_changes`, refuses on prior-hash mismatch, and sets the new `GrantsAuthorityIdentityV1.generation` to prior generation + 1. `authority_state_hash` is blake3 of canonical JSON of `toy_catalog`, `toy_grants`, and `watch_scopes`. The mutation intent hash is blake3 of canonical JSON of `changes` plus `grant_shaping_sources`. The fact is appended with `append_before_effect` before the projection write.

Child and peer mutations do not enter that fold. `mutation_observation` and `peer_mutation_observation` set `detail_ref: None`. Their effect is `MctDaemonConfigStore::save`. Restart trusts the config file for those facts and the ledger only for toy/watch authority.

Epoch establishment (`open_authority_with_startup`) appends `EpochEstablishedFactV1` only when startup evidence matches a rescan. Virgin requires an empty ledger and no operator principal. `OperatorGatedNonvirgin` requires an empty ledger, predecessor `NoneAfterOperatorReinitialization`, and both a decision id and an `authenticated_principal_ref`.

### Revocation

| Act | Effect today | Replay |
| --- | --- | --- |
| Toy grant | `ToyGrantState::Revoked` (or `Superseded` / `Denied`) on `ToyGrantPut`, or `ToyGrantRemove` | Yes, via `replay_authority_entries`. `evaluate_toy_grant_for_call` returns `RevokedGrant` and mints no `AuthorizedToyCall`. |
| Watch scope | `WatchObservationScopeState::Revoked` | Yes. A terminal scope cannot be revised or resurrected (`apply_authority_changes`). |
| Child | `ChildApprovalState::Revoked` and `ChildAssignmentState::Revoked` in config | No. `evaluate_child_call_authority` denies `AssignmentRevoked` / `ApprovalNotApproved` from the loaded projection. |
| Peer | `MctDaemonConfigStore::revoke_peer` sets `BindingState::Revoked`. `remove_peer` deletes the row. | No. `evaluate_hello_internal` returns `BindingRevoked` from the config projection. |

Revocation is local to this Mother's store. Nothing pushes it. The next check on this process sees it after the config save or after the authority projection updates. A later process sees toy/watch revocation by replaying the chain, and child/peer revocation by reading config.

### UDS admin auth today

`serve_uds_control_stream_with_handlers` (`crates/mct-daemon/src/control.rs`) calls `UnixStream::peer_cred` and builds `MctUdsPeerCredentials { uid, gid, pid }`.

Mutations: `MctUdsControlMutationHandler::authenticate_owner` returns 401 when credentials are missing and 403 when `peer.uid != expected_owner_uid`. Success mints `MctUdsAuthenticatedOwner` (private field; only this check constructs it). `run_control_serve_uds_with_state_until` sets `expected_uid` from the socket file's uid, requires the config file uid to match, mode `0600`, and passes that uid into the handler. `resident_observed_mutation_handler` itself reads the uid from the config file's metadata.

Reads on that same socket do not use the owner check. The `ReadOnly` arm calls `handle_control_plane_path_result_with_auth` with `MctControlPlaneAuthPolicy::open_local()`. `authorize` allows the call when `required_bearer_token` is `None`.

HTTP control never sees peer credentials. `serve_http_control_loop` / `serve_http_control_once_with_snapshot_result` also use `open_local`. Those routes are read-only (`GET` only).

The isolated startup plane (`run_isolated_startup_plane` in `crates/mct-daemon/src/daemon/resident/serving.rs`, `MctIsolatedStartupPlaneV1::handle` in `crates/mct-daemon/src/startup.rs`) compares `peer_cred` uid to the service-root directory owner. `accept_operator_startup_gate` then requires `authenticated_principal_ref == required_owner_principal_ref` (an `os-uid:` string), the confirmation `reinitialize-missing-canonical-authority-v1`, and an unchanged inventory hash. That principal is stored on the epoch fact.

The CLI presents nothing. `try_resident_control_mutation` (`crates/mct-daemon/src/daemon/cli_admin.rs`) writes a bare HTTP request to the socket. If connect fails, `execute_offline_*` mutates config and ledger with no principal check beyond filesystem access. `run_status` / `query_resident_status` is the same unauthenticated socket read.

Node identity is not an admin key. `load_or_create_node_secret_key_hex` / `write_new_node_secret_key_file` create a 32-byte hex iroh secret, mode `0600` on `create_new` only. `MctDaemonConfigStore::ensure_local_identity` derives the endpoint id and stores the path in config. That key signs peer bindings. It does not appear in `GrantsAuthorityIdentityV1`.

## 1. Bootstrap

The first grant is the only admin act that cannot present an already-granted key. It has to be appended by the init process before the control plane listens, as one fact, and then the listener default-denies everyone else. Virgin epoch establishment already refuses operator evidence; the first grant cannot be smuggled in as a fake `os-uid` on that fact without changing `validate` of `EpochEstablishedFactV1`.

Two shapes are chosen. The admin side creates the keypair. Mother stores only the public half. Both sides confirm an Ed25519+ML-DSA-65 fingerprint. The owner-only secret file and the printed stdout token are not the bootstrap.

### Pre-seeded admin public key

Chosen. Deploy config lists admin public keys the way `authorized_keys` lists keys. Mother stores only that public half. Init appends a grant of that public key before the control plane listens, as one fact. The secret is not in the fact and not in Mother's config. The node secret stays the peer-binding signer. It is not this key.

Replay of a virgin ledger yields those pre-seeded grants. A secret written into the identity directory or into config is the rejected file below, not this shape. `MctDaemonConfigStore::save` still uses `fs::write` with no mode, so a public key list must not be a place the secret or an unburned ticket lands.

### Not chosen: owner-only secret file at init

Not chosen. It contradicts (a). Mother would generate the secret and store it. The admin side makes the keypair, and Mother stores only the public half. The paragraphs under this heading are the rejected analysis, kept for the hazards. They are not a bootstrap to build.

Init would generate an Ed25519 admin secret, write it the way `write_new_node_secret_key_file` writes the node secret (new file, mode `0600`), and append a grant of that public key before serving.

Threat model. Whoever can read the file, or the directory before the mode sticks, holds the key. `create_dir_all` on the identity parent does not set `0700` (contrast `ensure_private_directory`, which does). A shared container user is the same principal as the file owner. A copied volume copies the key. The node secret and this admin secret are different keys; using `load_or_create_node_secret_key_hex`'s file as the admin key would let a peer-binding signer administer the Mother.

Ledger. The grant is an authority mutation (or a field on the epoch fact) whose subject is the public key, with the scopes chosen in section 2. The secret is not in the fact. `grant_shaping_sources` needs a source that is not `os-uid:`. Replay of a virgin ledger yields exactly one such grant. `load_or_create_node_secret_key_hex` returns an existing file without checking that a grant for it exists; an admin-key loader that copies that function will accept an ungranted file.

Re-init. `create_new` will not rotate a surviving file. A surviving key file plus a wiped ledger looks like first boot and would grant the old key again unless init refuses when a key file exists and the startup class is not virgin, or when the public key was revoked in a predecessor head (`ValidatedHead`). `OperatorGatedNonvirgin` already refuses to establish an epoch without a principal; a key file alone must not satisfy `accept_operator_startup_gate`.

Container / VPS. No TTY required. Ephemeral root loses key and ledger together and a new empty ledger is virgin (a new Mother). A persisted ledger with an ephemeral key file locks administration out. A persisted key file with an ephemeral ledger re-grants on the new virgin epoch. This shape needed no stable OS user only if the file mode were `0600` and the parent `0700`. That is not enough to choose it. The chosen shapes do not put an admin secret on Mother.

### Not chosen: one-time printed enrollment token

Not chosen. A bearer written to stdout is not the bootstrap. The paragraphs under this heading are the rejected analysis, kept for the hazards. They are not a bootstrap to build.

Init would draw a single-use token, write it only to the process's original stdout, and refuse to grant any key until that token is redeemed with a public key.

Threat model. The token is a bearer. Docker logs, CI logs, and a captured stdout are copies. A second redeem must fail closed. A token that still works after the process restarts is a second unledgered secret. The token value must not be stored in the ledger; a hash of it, marked consumed, can be.

Ledger. Redemption is an authority mutation. The fact records the token hash, the presented public key, and the scopes. Replay rejects a second fact that consumes the same hash (`MutationIdConflict` is the existing pattern for a repeated mutation id; the token hash needs the same single-use property). The pre-redemption Mother has zero admin keys, so status and mutations both deny.

Re-init. A new token on an existing ledger is a new grant. That is the operator-gate problem again: `accept_operator_startup_gate` binds the confirmation to `expected_inventory_hash`. A reprinted token must be bound the same way, or re-init mints a parallel admin key over the old grants. Consumed tokens stay consumed across reopen because they are in the chain, not in memory.

Container / VPS. No TTY means the token goes to the orchestrator log or to a file. A file is the previous option. An unscaptured stdout is a lost token and a Mother that can never be administered. Ephemeral filesystem is fine if the ledger volume persists and the token was already redeemed.

### Pairing-style ticket

Chosen, beside the pre-seeded public key. The ticket is single-use and short-lived, redeemed over iroh, and burned in the ledger. It is not a dial hint left in the address book, and it is not a standing admin credential.

Init binds an iroh endpoint (the existing `MotherIrohEndpoint::ticket`) for a short window. The peer presents its public key over that path. Mother grants that key, burns the ticket, and the ticket stops being sufficient.

Threat model. Anyone who learns the ticket during the window completes enrollment. Tickets today are stored on `MctPeerAddressBookEntry.ticket` inside config, and `MctDaemonConfigStore::save` uses `fs::write` with no mode. A world-readable config publishes the ticket. Relay URLs on `MotherIrohEndpointTicket` expose the attempt to the relay. The ticket is a dial hint in current code; treating it as authority would make address-book contents an admin credential.

Ledger. The grant fact records the peer endpoint id, a hash of the ticket, the expiry of the window, and the scopes. Redeem burns that hash in the ledger. Replay denies a second consume of the same hash. An unredeemed ticket dies with the window. The admin grant has its own expiry (section 2). Replay checks the grant and the burn. It does not treat an address-book dial hint as the credential.

Re-init. A ticket is not an epoch. Reusing one against `NoneAfterOperatorReinitialization` must fail the inventory-hash check the operator gate already performs. A ticket that survives in config across a virgin re-init must not auto-grant.

Container / VPS. Works with no TTY and no shared uid, including from another machine. If the process exits before pairing, the window is gone. Ephemeral filesystem drops the endpoint secret from `load_or_create_node_secret_key_hex` and the ticket with it.

## 2. Scoped grants

### Capability names, taken from routes

One uid currently authorizes all of `RESIDENT_MUTATION_ROUTES`. Scoped grants have to split that list. Names below are the route or command-kind terms already in the code, not a new product language.

| Scope | Authorizes |
| --- | --- |
| `status` | `GET /`, `/status`, `/state`, `/runs`, `/snapshot` |
| `approve-child` | `POST /children/approve` |
| `revoke-child` | `POST /children/revoke` |
| `authorize-slate` | `POST /toys/authorize-slate` (`AuthorizeSlate`) |
| `authorize-secret` | `POST /toys/authorize-secret` (`AuthorizeSecret`) |
| `watch-grant` | `POST /watch/grant`, `/watch/supporting-grant`, and the CLI grant-* toys |
| `watch-revoke` | `POST /watch/revoke`, CLI `revoke-watch` |
| `peer-add` | `POST /peers/add` |
| `peer-proof` | `POST /peers/proof` |
| `peer-revoke` | `POST /peers/revoke` |
| `peer-remove` | `POST /peers/remove` |
| `trigger` | `POST /triggers/create`, `/triggers/revise`, `/triggers/revoke` |
| `artifact` | source create/revoke, `/artifacts/stage`, `/releases/acquire` |
| `registry` | `/registry/install`, `/registry/sync` |
| `blob` | `POST /blobs` |
| `lifecycle` | `POST /lifecycle/fact` |
| `authority-import` | `POST /authority/import-toy-state` |
| `key-manage` | issue, narrow, and revoke admin keys |

`/pando/record` stays on the admin surface and needs a scope (a). Today it is an operator write with no grant check beyond the uid. `/identity/ensure` is not key management; it is node-secret rotation and it is refused while serving.

`key-manage` is not implied by any other scope. A key without `key-manage` cannot grant or revoke keys.

### How a check would ride the existing machinery

Two evaluators already default-deny. Neither subject's type is an admin key.

`evaluate_toy_grant_for_call` already does the capability check: catalog must contain the toy, state must be `Active`, policy revision must match, optional time window must hold, and `scope_matches` requires the action in `allowed_actions`. Missing grant denies. Admin scopes map onto `allowed_actions` without a new matcher. The mismatch is `ToyGrantSubject`: it is a Child artifact, and `evaluate_toy_grant_for_call` also requires an `MctCall`. Using it unchanged would mean inventing a fake Child and a fake call for every admin request.

`evaluate_hello_internal` already authenticates an Ed25519 endpoint id, requires `Admitted`, enforces mandatory `expires_at`, and intersects a requested capability list (`allowed_alpns`) with policy. `enforce_hello_binding_signature` is the proof check. The mismatch is the scope type (`allowed_alpns` are protocol names) and the store: bindings are config rows, so a revoke does not replay through `replay_authority_entries`.

The ledger path that can carry a new grant without a second fold is `AuthorityChangeV1` inside `execute_authority_mutation`. A new change kind is refused today (`UnknownFactKind` / incoherent apply). Extending that enum is how toy grants became replayable.

The admin check generalizes `evaluate_toy_grant_for_call` (a). The subject becomes a Child or an AdminKey. Route scopes are the action list. That is the evaluator. `evaluate_hello_internal` stays the peer-admission check. It is not extended into the admin check. The generalization is the subject and the scope list, not a third function with its own allow rules, and not a fake Child plus a fake call. Toy grants keep authorizing Child Toy calls. Hello keeps authorizing peer admission.

Default-deny for the admin check: no grant, wrong key, wrong scope, revoked, expired, stale generation, or bad proof is a deny with no token. `MctControlPlaneAuthPolicy::open_local` is the opposite and is not an admin path (b).

### Expiry

Peer bindings deny when `expires_at <= now`, and the field is not optional. Toy grants skip the window when `expires_at` is `None`, and `toy_token_expiry` then falls back to the call deadline. Child approvals never expire. An admin grant that copies the toy field will be permanent whenever the issuer omits the timestamp.

Day-to-day admin keys use mandatory `expires_at`, the peer-binding rule (a). Optional expiry is not the rule for those keys. The recovery key is not a day-to-day key: its scope is the startup gate plus `key-manage` only, not every route.

### Break-glass and day-to-day

The only break-glass in the tree is the operator startup gate: one confirmation, one inventory hash, one principal, and it establishes an epoch. It is not a standing key, and it cannot call `/children/approve`.

A recovery key is a granted key whose scope is the gate plus `key-manage`, not the union of every route in `resident_mutation_handler`. Day-to-day keys carry the route scopes and do not carry recovery. A recovery key that can also `authorize-secret` and `peer-add` is the current uid check under a new name. A day-to-day key cannot revoke the recovery key, including a day-to-day key that holds `key-manage`. That lockout is decided under [Recovery key lost](#recovery-key-lost).

Custody of the recovery secret uses the same two shapes as bootstrap (a): a pre-seeded public key, or a single-use iroh pairing ticket. The secret file and the printed token are not a third shape. The epoch fact already stores `authenticated_principal_ref`; a recovery use stores the recovery public key there instead of `os-uid:`.

## 3. Key loss and compromise

### Revoke and reissue

A caller that still holds `key-manage`, and is not the key being revoked, appends an authority mutation that sets that grant's state to revoked and puts the replacement grant. Same generation increment as any other mutation (`prior + 1`), same prior-hash check, same `append_before_effect`. That does not let a day-to-day key revoke the recovery key. Recovery-key loss is a lockout, decided under [Recovery key lost](#recovery-key-lost).

Watch scopes are the strict pattern: `Revoked` is terminal and cannot be resurrected by a later put. Toy grants can be put again at the same `grant_id`, which is a resurrection. Admin revoke should follow the watch rule. Otherwise a replay that applies an older put after a revoke, or a buggy reissue that reuses the id, restores the compromised key.

`remove_peer` deletes the row so a later reader cannot tell revoke from absence. Admin revoke keeps the row in the revoked state so replay can show the deny. `ToyGrantRemove` is the wrong shape for the same reason.

The compromised key cannot be the signer of its own revoke. `authenticate_owner` does not have this property: the uid that was stolen is the uid that can revoke. The new check has to refuse when the presented key is the subject of the revoke.

In-flight work: each UDS request is authenticated separately in `serve_uds_control_stream_with_handlers`. There is no session token to outlive the revoke. A new admin path must not add one. Toy calls already minted as `AuthorizedToyCall` are a different cache; admin revoke does not unmint them.

### Recovery-key custody

The recovery secret is the only principal that can pass the gate and mint a new `key-manage` key after the day-to-day keys are gone. The admin side holds that secret. Mother stores the public half. It is not the node secret (that key signs peer bindings via `upsert_peer`). A recovery secret written into Mother's config or identity directory would be the rejected owner-only file. Losing that secret is a lockout. There is no second principal and no day-to-day path back in.

### Recovery key lost

Decided. For now, losing the recovery key is a lockout. A day-to-day key cannot revoke the recovery key, and if the recovery key is lost there is no way back in. This follows the iroh model (Signal minus the seven-day reset), because MCT has no centralization mechanism yet. iroh has no recovery, rotation, or revocation concept and puts authority above the transport (docs.iroh.computer, n0-computer/iroh discussion #4221). Signal never lets a linked device control the primary, and resets a lost PIN only after a 7-day registration-lock wait. That reset is not available now.

If the recovery public key is still `Active` in the ledger and the secret is gone, `accept_operator_startup_gate` has no second principal: mismatch returns `OperatorGateRefused` and writes nothing. A day-to-day key cannot revoke the recovery key and cannot mint a replacement, with or without `key-manage`. That Mother stays unadministrable for the rest of the epoch.

Wiping the ledger to get a virgin epoch is a new Mother, not a way back into this one. Grants, toy authority, and the old recovery public key are gone because `replay_authority_entries` on an empty file has no facts. `OperatorGatedNonvirgin` exists so a non-empty loss of canonical authority cannot silently become virgin. Losing the recovery key must not grow a bypass around that class.

Two future paths are multi-mother work. They are not current decisions, and they are not alternatives available now.

Future scale, not planned now: multi-Mother, with a ledger per Mother. This is not a current decision, and it is not path 1 or path 2.

Future path 1, Signal-style delayed visible reset. Any day-to-day key starts it, it waits about 7 days, it warns every enrolled admin device so the real owner can cancel, and then the old recovery key stops working. This becomes possible with the multi-mother model, where a quorum of Mothers runs the delay and the warning.

Future path 2, the Argent X guardian pattern. Instead of a timer, a quorum of trusted parties (Mothers) act as guardians to approve recovery. This relates to the deferred threshold work: FROST is classical-only and there is no threshold ML-DSA yet.

#### Conflicts with (a)–(f)

The two conflicts named here are resolved. (a) and (c) stay locked aside from those two sentences.

(a) is resolved. "Grants are revocable" now has a carve-out on the decided statement of (a): the recovery grant is the exception, and under the lockout nothing can revoke it. That is the lockout, not an open conflict. The recovery key is still the startup gate plus `key-manage` only. Future path 1 stays future multi-mother work. It is not available now, and it is not a current conflict with the carve-out.

(c) is resolved for those two paths. "No failover" on the decided statement of (c) covers a Child's work only, not admin recovery. The delayed reset and the guardian quorum therefore do not conflict with (c). They stay future multi-mother work.

Per-Mother ledgers versus (c) are future multi-Mother scale work, not a current blocker. (c) is not rewritten. If a Child whose Mother is unreachable kept working from another Mother's ledger, that would be failover of a Child's work. That is not planned now, so it is not an open conflict on the current design.

Future path 2 still conflicts with (a)'s fingerprint. Both sides confirm Ed25519+ML-DSA-65. FROST is classical-only, and there is no threshold ML-DSA yet, so a guardian quorum built on FROST does not cover the ML-DSA half. These two clarifications do not resolve that. This note does not drop that half.

(b), (d), (e), and (f) are untouched. Neither future path adds an admin transport, a Child handle, a wake rule, or an interrupt rule.

### How revocation propagates and replays

On this Mother, `execute_authority_mutation` updates the in-process projection after the append. A restart calls `replay_authority_entries`, which re-applies the revoke put and the generation. `authority_state_hash` changes, so a projection cursor from before the revoke fails `AuthorityStateHashMismatch`.

Child and peer revokes do not do this. Their observations are skipped by replay. An admin revoke implemented as `prepare_revoked_child` or `revoke_peer` (config save plus `detail_ref: None`) is invisible to `replay_authority_entries` and is the regression to avoid.

Other machines are not told. The next admin call to this Mother is denied when the check reads the new state. There is no export of admin grants; `ExportStatus` on the ledger entry is not an admin-revocation protocol.

### Hybrid Ed25519 + ML-DSA-65 (scheme v1)

What is signed today. Only the peer-binding payload in `sign_peer_binding_signature_ref`. Algorithm is iroh `SecretKey` (Ed25519). Encoding is 32-byte lowercase hex secret, public key as the endpoint-id string, signature as 64 bytes under `mct-ed25519-binding-v1:`. Ledger entries are not signed. Authority facts are not signed. `replay_authority_entries` checks the blake3 chain and structural coherence, not a signature. `GrantShapingSourceV1::OperatorDecision` records a uid string inside the hashed fact; that string is not a proof.

ML-DSA is not in the tree. No hybrid verifier exists.

The fingerprint algorithm is decided (a): both sides confirm Ed25519+ML-DSA-65. Coverage and key placement are decided in [Decisions (October 15, 2026)](#decisions-october-15-2026): scheme v1 signs the command plus the grant under both keys, and the ML-DSA-65 public key sits on the grant fact beside the Ed25519 key. The list below is what that scheme has to handle.

Scheme v1 has to cover at least:

- Key encoding. A hybrid public key is not an iroh endpoint id. `PublicKey::from_str` in `verify_peer_binding_signature_ref` will not parse it. Admin keys stay Ed25519 iroh identities for transport, with the ML-DSA-65 public key stored on the grant fact. A new combined encoding is not used.
- The signature bytes. `decode_64_hex` rejects anything other than 128 hex characters. A concatenated Ed25519 || ML-DSA-65 signature needs a new prefix. Old `mct-ed25519-binding-v1:` proofs must still verify.
- The signed message. `PeerBindingSignaturePayload` is the whole contract (`binding_id`, both endpoint ids, node ids, vision, ALPNs, policy revision, expiry). Adding a field invalidates old signatures. Admin-grant signing needs its own context string so it cannot be replayed as a peer-binding proof.
- The ledger fact. A new `fact_kind` or `AuthorityChangeV1` variant fails closed until `replay_authority_entries` and `apply_authority_changes` know it. The fact records public keys and scopes. It must not record secrets. If scheme v1 signs the fact, replay has to verify that signature; otherwise the signature is not what makes the fact authoritative. The hash chain stays the integrity check either way.
- Verification on replay. Replay today will accept a well-formed fact whose signature would have failed, because it never looks at one. Scheme v1 verification has to be per fact, using the algorithm id stored on that fact. A global switch that rejects Ed25519 would quarantine every existing ledger (`BrokenHashChain` or `UnknownSchema` depending on how the bytes change).

What stays stable. The blake3 preimage of an entry (JSON with `entry_hash` empty), the previous-hash link, schema `mct-authority-fact/v1`, the existing fact kinds, generation = prior + 1, the Ed25519 prefix, and the peer-binding payload context. New entries may use a new prefix. Old lines must still parse and hash.

## Migration

Decided. Hard cutover, with no switchover window. MCT isn't live, so there is no migration to protect. The old admin path is deleted as part of the build once the new iroh admin-key path is complete: the UDS socket (`run_control_serve_uds_with_state`, the resident UDS bind), the `peer_cred` OS-user check (`control.rs` `peer_cred`, `authenticate_owner`), `open_local`, the `os-uid` principals, and the `execute_offline_*` commands. There is no dual-auth window and no transitional period where a key and the OS user are both required. The code map already lists these as removal targets. They come out together with the new key path rather than lingering.

The end state is (b) and (d): iroh only, no UDS admin, no uid, no `open_local`, no same-uid path. This cutover takes (b)'s removal arm for `execute_offline_*`. It does not keep those commands behind a second check. `POST /calls` moves to the iroh call path. That path is the Child handle, not an admin key. Leaving it on uid is not an allowed end state.

Build order, not a live switchover:

1. Ledger the admin-grant fact and the deny-by-default check.
2. Serve admin routes over iroh and require a granted key. In that same build, delete the UDS socket, `peer_cred`, `open_local`, and the `os-uid` principals. Uid is not sufficient, and accepting either key or uid is a second path. There is no interval where key and uid are both required.
3. Point the read arms off `open_local` in that same build. `status` is a scope, including for `query_resident_status`.
4. Delete `execute_offline_*`, including `execute_offline_peer_mutation` and `execute_offline_administrative_mutation`. They are not kept as a checked fallback. Left in place, they are an unauthenticated writer whenever the socket is down.
5. Move `accept_operator_startup_gate`'s principal from `os-uid:` to the recovery public key in that same build.
6. The admin path is iroh. The sockets-blocked test exercises that path. A host that cannot open a Unix socket administers Mother over iroh.

What keeps working. Iroh hello, binding proofs, and `evaluate_hello_internal` for peer admission. Child and Toy evaluation for calls, with the admin check as the generalization of `evaluate_toy_grant_for_call`. The hash chain and toy/watch replay.

What breaks. `try_resident_control_mutation` and `query_resident_status`, which send no proof. HTTP control on `open_local`. The offline commands, which are deleted. The startup plane's uid compare. UDS admin. Any workflow that assumed same-user on a VPS, a container, or a service account. Fawn, where the socket is unavailable, is administered over iroh. There are no live ops scripts to protect.

#### Conflicts with (a)–(f)

No conflict with (a)–(f). (b) and (d) stay locked.

(b) already says iroh is the only admin path: no UDS admin, no uid/`peer_cred`, no `open_local`, and `execute_offline_*` uses the same admin-key check or is removed. This decision takes the removal arm and forbids a dual-auth window. It does not add a transport, and it does not keep uid as a credential. (b)'s "or" is not rewritten.

(d) already says there is no local socket and no same-uid path. Deleting the UDS admin socket and moving `POST /calls` to the iroh call path is that decision. The call path stays the Child handle, not an admin key.

(a), (c), (e), and (f) are untouched. The cutover does not change the recovery grant, a Child's hard stop, the wake check-in, or checkpoint interrupts.

## Tests that would prove it

- Missing proof on a mutation and on `GET /status` is 401, including from localhost and from the socket owner.
- A granted key with `approve-child` can approve and cannot `peer-add` or `key-manage`.
- Expired `expires_at` denies. Revoked state denies. A second put of the same grant id after revoke denies (no resurrection).
- Restart: append a revoke, open a new process, `replay_authority_entries`, the same proof denies. Tampering the line trips the existing broken-hash quarantine.
- The recovery key passes the operator gate and mints a day-to-day key. It does not `authorize-secret`.
- Key files and any ticket file are `0600`, parents `0700`. A `0644` key file is refused, including one created before the check (today `load_or_create_node_secret_key_hex` does not re-stat mode).
- `execute_offline_*` is gone. A process that can write config and the ledger still cannot administer except over the iroh admin-key path.
- Sockets-blocked job, Fawn-shaped: Unix sockets unavailable (seccomp or an equivalent that makes `UnixListener::bind` fail). Admin still completes over the granted-key path that is not a Unix socket. The job fails if that path is skipped or if it falls through to `execute_offline_*`.
- Scheme v1 fixtures: an old `mct-ed25519-binding-v1:` proof still verifies; a hybrid proof verifies under its own prefix; a hybrid proof presented as a peer-binding signature does not; replay of a pre-scheme ledger still succeeds.

## Lazy failures to avoid

**Unauthenticated localhost.** `MctControlPlaneAuthPolicy::open_local` and the `ReadOnly` arm of `serve_uds_control_stream_with_handlers`. Also `serve_http_control_once_with_snapshot_result`, which always passes `open_local`. A regression is any admin route that still calls `handle_control_plane_path` (no auth argument) or treats "bound to 127.0.0.1" as a credential. `require_bearer_token` is not a fix: the compare is a static string, not a ledger grant.

**Permanent all-powerful keys.** `authenticate_owner` is one uid for every route in `resident_mutation_handler`. `ToyGrantConstraints.expires_at` is optional and the window check is skipped when it is `None`. `ChildApproval` has no expiry. An admin grant with an empty scope list, a missing expiry, or a scope that expands to "all routes" recreates that uid.

**World-readable key files.** `write_new_node_secret_key_file` sets `0600` only on `create_new`. `load_or_create_node_secret_key_hex` does not check the mode of an existing file. Identity parent directories are `create_dir_all` with the process umask. `MctDaemonConfigStore::save` writes config, including tickets and signature refs, with `fs::write` and no mode. An admin secret that lands in config, or a ticket treated as a credential, is world-readable whenever the umask allows it.

**A second unledgered auth path.** `execute_cli_peer_mutation` falls through to `execute_offline_peer_mutation` when the socket connect fails, and that writer never consults a grant. Child and peer observations set `detail_ref: None`, so `replay_authority_entries` skips them and the config file is a parallel authority store. The startup gate's `os-uid:` principal is another path that is not a granted key. Keeping any of these as a way to administer after the cutover is the regression.

## Narrative: the family

This is a picture, not a further decision. a–f stay locked. Where the picture conflicts with them, the conflict is marked and the picture does not win.

### The family: how a Mother raises her Children

MCT isn't a process tree. It's a family. A Mother doesn't drive each move her Children make. She brings them into the world, gives them what they need, and lets them go off and live. Everything below follows from treating the relationship that way.

### 1. The Mother lets a Child play

A Child is set to go do the thing a child does. Once it has a task, it gets on with it, step after step, without asking permission for each one. That gives it more freedom than a pure controller model, where the parent signs off on every action and the child just carries out orders. The Mother trusts her Child to get on with its task, and that trust stays inside the bounds of the Toys she gave it. A Child can't touch anything it wasn't handed. It acts freely, but only within what it has been given.

That freedom still has a tether. When a Child wakes, it checks in with its Mother before it touches its own things. That check-in confirms it's still her Child and that the house rules haven't changed. If she can't be reached, the Child doesn't go looking for someone else to obey. It finishes the step it's on, then sits still and waits for her.

### 2. The Mother can direct, stop, and fix a Child

A good parent can step in. The Mother can direct a Child ("do this next instead"), stop a Child ("put that down"), and fix a Child (correct its state, change its rules, or take something away). She does this by speaking to it over their connection. The Child listens at natural pauses, between one step and the next. She doesn't pull it out mid-motion. Those lines are adopted on the ruling below. The Child keeps its own state. "Do this next" is the interrupt carrying the next task. A fix is a grant. Mother does not rewrite that state.

These are the exceptions, not the normal day. A Mother who steps in on every move isn't raising a Child, she's just running a process. Stepping in is always possible and always takes priority, and it is rare by design. Every intervention goes in the ledger, so the family history shows when and why she stepped in.

### 3. The Mother can give a Child new Toys

Toys are how a Child reaches the world. Each Toy is a WASI capability: a handle the Mother grants that opens exactly one thing, such as a single filesystem directory, a single network socket or a single clock. Without the handle, that thing doesn't exist for the Child. It can't find it, guess it or break into it.

As a Child grows into new tasks, the Mother can hand it new Toys. Every gift is written into the ledger with what was given, to whom, under what limits and until when. She can also take Toys back. A Toy is never the Child's by birthright. It holds it because the Mother gave it, and only for as long as she lets it.

### 4. Children can share Toys with each other

Siblings share. One Child can lend a Toy to another, which in capability terms means delegating a handle it already holds. Sharing follows the same rules as everything else in the family:

- A Child can only share what it has. It can pass along a Toy, or a smaller version of one (one subfolder of its directory, read-only instead of read-write). It can never share something bigger than it was given.
- Every share goes in the ledger: who lent what to whom, with what limits. Nothing changes hands out of the Mother's sight.
- The Mother stays in charge of the whole house. If she takes back a Toy, every borrowed copy goes with it, and she can end any lending between siblings at any time.

Sharing lets Children work together without running back to the Mother for every handoff. The ledger keeps that cooperation honest, so the family can always trace where every Toy came from.

**Adopted: a Child is a courier, not a grantor.** The story above is the picture, including "one Child can lend a Toy to another." That sentence is not permission for any Child to share. The rule is this. The Mother remains the only grantor, and she creates every borrowed handle. Only an older Child can ask her to share a handle it already holds, or a smaller version of it (one subfolder, read-only instead of read-write), never anything bigger than it was given. Every delegation is a ledger entry the Mother can see and revoke, recorded the same way grants are. When the Mother takes a Toy back, every borrowed copy goes with it, because the ledger knows the full chain from the original grant down to each borrower. The two things that would break the system are a Child granting a Toy it does not hold, or a share outliving the original grant. Both are ledger enforcement problems, not architecture problems. "Without running back to the Mother for every handoff" does not make the Child the grantor.

**Decided: Mother mints every borrowed handle.** Mother creates every borrowed handle. That keeps her the only grantor and preserves (d), that handles come only from the platform. The lending Child, an older Child, asks for the share. Mother mints the narrowed handle and records the delegation in the ledger. A Child minting a handle stays the closed Child-as-grantor conflict. This does not rewrite locked (a) or (d).

**Decided refinements of the courier rule.** These narrow who may share. They do not reopen (a)–(f).

Sharing is older-child-only. Only a Child that Mother has granted older-child status may share a Toy. Older-child status is itself a grant, recorded in the ledger. The rule set for that status is bundled with the grant and scoped to the Child's task. A young Child that wants to share asks Mother, and Mother either shares the Toy herself or promotes the Child first.

Group model. Take a group of one older Child and five younger Children solving a problem. The older Child acts as the group's courier. A young Child handing a Toy up is not a share. It is a request, and Mother reissues that Toy to the older Child as the older Child's own handle. The older Child holds separate handles, one per Toy. It never holds one combined handle covering the group's Toys. Young Children still play with their own Toys and do not lend them. Each reissue is a ledger entry, so Mother can see exactly which young Child's Toy went where, even though young Children never touch the delegation chain. The older Child shares outward only from a handle it holds, and Mother mints that narrowed handle. If the older Child is compromised or misbehaves, Mother revokes its status, and every share it made dies with it, because the whole chain traces back to its grant. A young Child that needs something the older one won't share asks Mother directly. The older Child is a convenience, not a gate.

**Decided: how a group handles Toys.** A young Child handing a Toy up is not a share. It's a request, and Mother reissues that Toy to the older Child as the older Child's own handle. The older Child holds separate handles, one per Toy. It never holds one combined handle covering the group's Toys, because a combined handle breaks attenuation. Carrying a parent task's decision history down to a subtask is fine as a record only. The subtask's capabilities still come from its own grants, and history can never add up to more power than any single original grant. Attenuation holds at every level. No Child or group ever holds more than any single grant gave out. This does not rewrite locked (a)–(f).

### Proposed: decision model

Proposed, not a locked decision. (a)–(f) stay locked. The courier rule and the older-child refinements stay as adopted. Who mints is decided with the courier rule: Mother creates every borrowed handle. Hand-up, the combined set, and history-as-capability are decided with the group ruling: a hand-up is a request Mother reissues, the older Child holds separate handles, and history carried down is a record only.

The decision model is a Toy that Mother grants to older Children. It is a WIT interface. The Child passes in a state plus typed questions and gets back one of three answers: a choice, up to 255 options, each with a probability; a score on an ordered scale; or a calibrated 0–1 probability, for yes/no gates. The Child's voice carries the rule set: its thresholds and risk posture. The model advises inside those rules. The model advises but never decides. The courier rule and the ledger decide what may be shared. The model only answers whether to share something that is already allowed. Every decision is recorded in the ledger with its probability, so Mother can audit it and tune the thresholds over time. That record is the model's output. A replayed run reuses it instead of calling the model again. It is not the replay Toy's seed and counter.

The reference model is Jev from TypeSafe AI ([introducing system-one models and Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev), [docs](https://www.jevtypesafeai.com/docs), `POST /v1/decide` with choice, score, and noul question types). It is hosted and runs only on TypeSafe's servers. It is one Toy in the catalog below. A Child does not call that endpoint itself.

Decision models are a catalog of Toys behind a thin gateway. Each model (Jev, Nimble, Local, or future ones) is its own Toy with the same WIT interface shape. Mother holds each one's credential and grants the Toy to the Child. The only difference between them is where the credential points (a hosted endpoint, a local key, etc.). Older-child status and the per-task limit are Mother's grant check, made before the handle exists. The gateway is a thin shim: it holds the credential, honors the handle Mother already granted, and forwards the request. It does not decide who may call, and it does not become a smart middleman. The Child sees one interface and never sees credentials. Adding a new model later must be just a new grant, not a redesign. Design the gateway so it can't paint us into a corner. Follow Mitchell Hashimoto's design philosophy: boring, composable, cheap primitives, and one interface with many implementations. Ship with one model first and prove the interface before adding a second. A Child calling the hosted endpoint itself, with the credential in hand, stays forbidden. The closed shape is Mother holds the credential and the Child holds only the granted Toy. This note does not read (a) so that this credential is an admin key or part of the recovery grant. This is the proposal's ruling. It is not a rewrite of locked (a)–(f).

The research cited with this proposal, arXiv 2610.01834, found Jev "behaves like a policy without a world model... does not look ahead" and concluded "Code should own the simulation, and Jev the evaluation." So code should compute the delegation chain, what the borrower already holds, and the revocation fallout, and pass those in as the state.

The decision model runs inside the task loop as that one checkpoint. A young Child reaches the pause with no weighing. For it, the pause is only Mother's interrupt check. "Straight through" for a young Child means no weighing, not no pause. An older Child reaches the same pause for a check, and if Mother is live and does not interrupt, weighs the move and acts.

The model's answer is a judgment, never an action. The Child still acts on it, and the ledger records both the judgment and what the Child did with it.

Mother's rules set the thresholds, the Child's voice shapes how it weighs things, and the model supplies the calibrated read. That is the split of the line above, where the Child's voice carries the thresholds and the risk posture.

The Toy is task-based. The decision-model Toy is granted per task, not per Child, so when the task ends, the judgment ends with it.

The per-task grant is a fresh, scoped handle Mother gives the Child at task start, and it dies when the task ends. Older-child status or the task by itself is not that handle. Mother grants it every time, which keeps (d) intact. The cost is a ledger entry per task and a check-in with Mother to receive it. This is the proposal's ruling. It is not a rewrite of locked (d).

Future extension, not the present shape of this proposal: stack task measurements so a parent task can carry its decision history down to its subtasks. That carry-down is a record only. The subtask's capabilities still come from its own grants, and history can never add up to more power than any single original grant.

The decision checkpoint and Mother's interrupt checkpoint from (f) are the same single pause, not two separate ones. This is a deliberate design choice, not a default. No second pause is planned. If one is needed later, it gets added then. This is the proposal's answer for where the weighing sits. It is not a locked change to (f).

Inside that one pause, the order is check first, then weigh, then act. Mother's interrupt check runs before the decision model weighs the move, so Mother can stop the action before it happens. The Child keeps its autonomy because it still weighs and decides when Mother is live and does not interrupt. The pause is a gate, not just a record. A young Child's pause is just the check. An older Child's pause is check, then weigh, then act. The judgment is still ledgered as advisory. The ledger still records what the Child did. Every Child reaches the pause. "Straight through" for a young Child means no weighing, not no pause. Acknowledged tradeoff: this cuts into the Child's autonomy a little. It is still the right call, given (f) (every Child pauses) and Mother's authority. This is the proposal's answer. It is not a locked change to (f).

The order inside the pause is check, then weigh, then act. No other order is kept.

No live Mother means no pass, so the Child freezes at the pause. Mother dropping is not the same as Mother not interrupting. If "no interrupt" were read as "no message came," an older Child could weigh and act while she's gone, which breaks (f). That reading is not kept. Inside the pause, the check needs a live Mother. If she's live and doesn't interrupt, the Child continues (weigh, then act, for an older Child). If she isn't reachable, every Child, young or older, waits, then freezes at the pause. That wait is the grace window below. It is not a pass. This matches the locked principle that Mother is essential and "unreachable" is a hard stop, not failover. (c) says no-failover covers a Child's work only, not admin recovery. This freeze is a Child's work. It does not narrow (c). This is the proposal's rule. It is not a locked rewrite of (f) or (c).

The grace window before a Child freezes when Mother is unreachable is not a fixed architectural number. It's a policy Mother sets, recorded in the ledger, and it can vary per Child or per task (a cautious Child gets a longer window, a fast one a shorter one). The freeze rule itself stays absolute: no live Mother means no pass. The window starts at the next checkpoint, so it never expires mid-step. It only decides how long the Child waits before freezing once it has reached the checkpoint. It does not cut the current step, and it does not let the Child weigh or act while Mother is unreachable. The window is waiting, not permission. In the pause order the check still comes first. Weigh and act still require a live Mother who did not interrupt. If the window expires, the Child freezes and does not weigh or act. The time limits in real systems (Signal's 7-day PIN lockout, Argent's guardian quorum, actor-supervisor restart policies) are all policy choices, not derived values. MCT treats the grace window the same way, as an explicit recorded policy rather than a hardcoded constant. This does not rewrite (c) or (f).

The two-clocks flag is closed. The Child finishes the current step, as (f) says, and the grace window starts at the next checkpoint. It cannot expire mid-step, and it cannot be a pass. (c) and (f) are not rewritten.

State goes to the model only after the Child has checked in with Mother on waking. The decision checkpoint already runs after the wake check-in, so this is already true in the design. It is not a rewrite of locked (e).

#### Conflicts

These are flags. They do not rewrite (a)–(f) or the older-child rules. Hand-up, the combined set, and history-as-capability are closed.

The direct-hosted-credential flag is closed. A Child calling the hosted endpoint itself, with the credential in hand, stays forbidden. Mother holds the credential and grants the Toy. The Child holds only that Toy and never sees the credential. (a) stays locked. This credential is not an admin key and is not part of the recovery grant.

A gateway that itself enforces older-child status and the per-task rule conflicts with the thin shim, which only holds the credential, honors the handle, and forwards. That reading is not kept. Enforcement is Mother's grant check. The gateway does not become a smart middleman.

If the model's answer is what performs the share, that conflicts with the courier rule. The courier rule and the ledger decide what may be shared. The model only answers whether to share something already allowed. An answer that mints a handle, or that lends for a young Child, is the closed Child-as-grantor conflict. Who mints is closed: Mother creates every borrowed handle. Hand-up, the combined set, and history-as-capability are closed with the group ruling.

Granting the Toy only to older Children fits the older-child rule only as a further grant. The call is authorized by the fresh per-task handle Mother gives, not by older-child status. Status is not that handle and is not a gate. (d) stays locked as written.

The recorded-probability flag is closed. The [replay Toy](#proposed-replay-toy) is a seeded random source: input that drives a run, giving the same random numbers every time so a Child's run can be replayed exactly. The recorded probability is output: what the decision model advised and how confident it was, written to the ledger for audit and tuning. These are two different mechanisms that both touch randomness. They don't collide, but they need different names. They meet in one place. If a replayed run called the decision model, the model's answer has to match or the replay breaks, so a replayed run reuses the recorded model answer from the ledger instead of calling the model again. The ledger line is not a seed and counter. The replay Toy is not the model's probability. Both stay proposals. This does not rewrite locked (a)–(f).

The state the code would pass in (delegation chain, what the borrower holds, revocation fallout) stays separate handles. A young Child's hand-up is a request, not a share, so it does not fill that state by lending. One handle bigger than any single original grant is not allowed.

(b) is untouched. The proposal does not add an admin transport. (e) stays locked as written. The flag that sending state to the model before the wake check-in conflicts with (e) is closed. State goes to the model only after that check-in, and the decision checkpoint already runs after it. (c) and (f) stay locked as written. The freeze rule above does not narrow (c) and does not rewrite (f).

The young-Child reading of "straight through" as no pause is closed. A young Child reaches the same pause with no weighing, and for it the pause is only Mother's interrupt check. Every Child pauses. "Straight through" means no weighing. That is what (f) requires of the pause, and this note does not leave the other reading open.

The worry that the Child could act before Mother can stop it is closed. The check runs first, so the weighing and the act come only if that check does not stop the Child.

The dropped-Mother flag is closed. No live Mother means no pass. A missing Mother is not "no interrupt," and an older Child does not weigh and act while she is gone. Every Child freezes at the pause after the grace window and waits for her. The window is not a pass.

The per-task (d) flag is closed. The decision-model Toy is a fresh, scoped handle Mother gives the Child at task start, and it dies when the task ends. Older-child status or the task by itself is not that handle. Mother grants it every time, so (d) stays intact: holding that platform-given handle is the authorization. The handle dying with the task does not take the Child's other handles. (d) stays locked as written.

A judgment the Child then acts on is not the model deciding, on the proposal's own words: the answer is a judgment, never an action, and the model advises and never acts. If "acts on it" means the judgment performs the share, mints a handle, or lends for a young Child, that is the closed Child-as-grantor conflict. Mother creates every borrowed handle. This note does not read the Child's action as the model deciding.

Hand-up, the combined set, and history-as-capability are closed. A young Child handing a Toy up is a request, and Mother reissues that Toy as the older Child's own handle. The older Child holds separate handles, one per Toy, and never one combined handle. Carrying a parent task's decision history down is a record only. The subtask's capabilities still come from its own grants, and history can never add up to more power than any single original grant. Attenuation holds at every level. No Child or group ever holds more than any single grant gave out.

### Why a family

A family is the closest picture of what MCT needs. Children have real independence, the parent has final authority, gifts and sharing follow lines you can trace, and when something goes wrong, the parent steps in rather than being consulted on everything. Keeping the model close to how people actually live makes it easier to reason about. When a design question comes up, ask what a good Mother would do.

### Where this conflicts with a–f

Sibling sharing is adopted under the courier rule in section 4, narrowed to an older Child. It does not by itself conflict with (a) or (d). "Do this next" and a fix are adopted on the ruling below. "Waits for her" is the adopted resume.

- **Sibling lending, closed by the courier rule.** The reading that is not adopted: a Child as grantor, or a handle a sibling invented, or any Child handing a Toy over. That reading fights (a), where Mother is the only grantor and grants are scoped, ledgered, and revocable, and (d), where the only way in is a platform-given handle and holding that handle is the authorization. The adopted reading is the courier rule, as refined. Only an older Child hands over a handle it already holds, or a smaller version of it, never anything bigger. Older-child status is itself a ledgered grant, with its rule set bundled and scoped to the task. A young Child that wants to share asks Mother. Mother remains the sole grantor. Every delegation is a ledger entry recorded the same way grants are. She can see it and revoke it. Taking the Toy back, or revoking the older Child's status, takes the shares that trace to that grant. A Child granting a Toy it does not hold, or a share outliving the original grant, is ledger enforcement, not a second architecture.

  The courier model still fits (a) and (d) on the point already decided. Mother still holds all grant authority. The borrowed handle is an attenuation of a platform-given handle, not a handle the Child minted. Holding that handle is still the authorization. Older-child status is not a second way in. The older Child is a convenience, not a gate.

  Who mints is closed. Mother creates every borrowed handle, as decided with the courier rule. The lending Child, an older Child, asks for the share. Mother mints the narrowed handle and records the delegation in the ledger. The handoff is itself a platform grant, so (d) holds. A Child minting a handle stays the closed Child-as-grantor conflict. (a) and (d) stay locked.

  Hand-up, the combined set, and history-as-capability are closed:

  - A young Child handing a Toy up is not a share. It is a request, and Mother reissues that Toy to the older Child as the older Child's own handle. Young Children do not lend and never touch the delegation chain.
  - The older Child holds separate handles, one per Toy. It never holds one combined handle covering the group's Toys, because a combined handle breaks attenuation. "Which young Child's Toy went where" keeps those handles distinct in the ledger.
  - Revoking older-child status is separate from revoking the underlying Toy grants, in the act the text names. Mother revokes the status, and every share that traces to that grant dies. Young Children still play with their own Toys. The text does not say those Toy grants are revoked. Reading "its grant" as the Toy grants instead would collapse the two. That reading is not adopted.
  - (d) is unchanged and stays locked. Authorization is holding a platform-given handle, not holding older-child status. A young Child's own Toys are still that kind of handle. A share outward is still a handle Mother minted. Status does not replace the handle, and it is not a gate past Mother. A combined handle is not held, and it is not the authorization. This note does not read (d) so that status becomes the authorization.

  (b), (c), (e), and (f) are untouched by these refinements.
- **"Do this next" and state-fix, closed.** The Child maintains its own state. Mother never reaches into it. "Do this next" is not a new power. It's the interrupt carrying a payload: Mother stops the Child and hands it the next task in the same message. That's one mechanism with two uses, and the Child still acts only at its checkpoints. Fixes happen only through grants: a fix Toy, a corrected input, or a restart with new parameters. Mother never rewrites the Child's state directly. Every fix is a grant, and the ledger records it. Taking a Toy back stays revocation under (a). "The Child listens at natural pauses" and "she doesn't pull it out mid-motion" match that checkpoint. This adopts those narrative lines. It does not rewrite locked (f). "Waits for her" is the adopted resume below.
- **"Sits still and waits," adopted as the resume.** Finishing the step it is on, and not looking for someone else to obey, matches (f)'s freeze at the next checkpoint and (c)'s hard stop with no failover. "Waits for her" is this resume. (c) still puts the effort into Mother never dying: hot standby, replicated state, fast restart.

**Decided: how a frozen Child resumes.** Resuming from a freeze counts as a wake under (e). A frozen Child that resumes reuses the wake check-in. Mother's answer is both the go-ahead and the policy re-check, and the Child continues from where it froze. The ledger records the freeze and the successful check-in. If she is still unreachable, or the check-in fails, the Child stays frozen. The rest of (e) is not rewritten. (c) and (f) stay as written.

The thaw-versus-(e) flag is closed. A frozen Child resuming is a wake, so the check-in is the one (e) already names.

### Narrative that does not contradict a–f

Labeled as narrative, not as new decisions.

- A Child gets on with its task inside the Toys it was given, without asking permission for every step. That is (f), inside a grant's scope.
- The check-in when a Child wakes, before it touches its own things, is (e).
- Mother handing Toys and taking them back, with the gift ledgered (what, to whom, limits, until when), matches scoped, ledgered, revocable grants. One WASI handle opening exactly one resource is picture detail, not a further decision.
- Writing every intervention into the ledger does not contradict (a). Every fix is a grant, and the ledger records it, as decided above. The ledger also records a freeze and the check-in that resumes it.
- "Ask what a good Mother would do" does not override a–f.

## Composition

Not a design, and not a decision. Nothing here wires components.

WIT components compose. One component's export plugs into another's import. `wasm-compose` can wire those plugs into one component. Several instances of the same component can each hold their own state.

That is why a Toy as a component instance, wired into the Child's imports, is worth asking about. The Child would see only the imports in the composition, which is the same boundary as a Child touching only what it was handed. Two instances of one component would not share state merely by being the same component. Whether that fit is the way Toys are handed to Children is the open question below.

## Proposed: replay Toy

Proposed, not a locked decision. (a)–(f) stay locked. The courier rule stays as adopted.

Add a separate replay Toy for deterministic randomness. The rule is "extend with a Toy, never fork the standard." Secure random stays on real entropy and is never bent. Replayable randomness is its own Toy: a seeded, deterministic source with a typed WIT interface, ideally a resource that holds the seed and a counter. A Child that wants determinism has to ask for the replay Toy explicitly. The ledger records which runs were replayable, so the intent is visible.

The replay Toy is a seeded random source. It's input that drives a run, giving the same random numbers every time so a Child's run can be replayed exactly. The recorded probability on a decision-model ledger line is output: what the model advised and how confident it was, written for audit and tuning. These are two different mechanisms that both touch randomness. They don't collide, but they need different names. They meet in one place. If a replayed run called the decision model, the model's answer has to match or the replay breaks, so a replayed run reuses the recorded model answer from the ledger instead of calling the model again. The ledger line is not a seed and counter. The replay Toy is not the model's probability. This stays a proposal. It does not rewrite locked (a)–(f).

The defect this aims at is wasi-pi seeding `wasi:random` `get-random-bytes` from a seed recorded in the ledger (`main.rs` near `random::wasi_sources`). Those files are not in patina-mct at `784c5ed`, and they are not in the public patina tree searched for this note, so that line citation is not verified here.

What is verified is the standard the proposal refuses to bend. In `wasmtime-wasi` 46.0.2, the version `Cargo.lock` pins, `wasi:random/random` `get-random-bytes` must return fresh unpredictable data, and a deterministic environment must omit the function rather than fill it with deterministic bytes (`src/p2/wit/deps/random.wit`). patina-mct does not ledger-seed that source. `build_wasi_ctx` in `crates/mct-daemon/src/wasm.rs` builds a `WasiCtxBuilder` and configures preopens only. It does not call `secure_random`, `insecure_random`, or `insecure_random_seed`. The supported import list includes `wasi:random/random@0.2.3`, and the linker is `wasmtime_wasi::p2::add_to_linker_sync`. wasmtime's default secure generator is `thread_rng` (`src/random.rs`). This Mother's secure source is the library default, not a ledger seed.

### Relation to `patina:pi@0.3.0` and `wasi:random` insecure

`patina:pi@0.3.0` is not in this checkout and not in the public patina WIT tree. There is no ledger-seeded `random` Toy here to compare against the proposal. Placement is decided below. The absence of that package does not leave the placement open.

`wasi:random/insecure` is not enough on its own. The interface is `get-insecure-random-bytes(len: u64)` and `get-insecure-random-u64()`. It has no seed parameter and no counter. `insecure-seed` is a different function, `insecure-seed: func() -> tuple<u64, u64>`, specified to be called once for hash-map DoS protection. It may be entirely deterministic. It is not a byte stream and it is not a counter. wasmtime keeps them apart: `get_insecure_random_bytes` draws from `insecure_random`, and `insecure_seed` returns the stored `insecure_random_seed` (`src/p2/host/random.rs`). The host can replace the insecure generator (`WasiCtxBuilder::insecure_random` in `src/ctx.rs`) and can set that 128-bit value (`insecure_random_seed`). `Deterministic` cycles a fixed byte vector (`src/random.rs`). That is host control of a stream, not a resource the Child holds with a seed and a counter. A Child cannot pass a ledger seed through this interface.

### Conflicts

No conflict with (a)–(f) is decided here, and none is resolved by adopting the Toy.

(a) and (d) fit this shape if the replay Toy stays a grant. The Child asks for it, so it does not hold the handle unless Mother granted it, and a ledger entry that a run was replayable is the same kind of record as a grant. The proposal does not make the Child a grantor, and it does not turn `wasi:random/random` into an authorization path.

(b), (c), (e), and (f) are untouched. The seed and the counter would be state, so the wake check-in (e) still comes before the Child touches them. The proposal does not say the Child keeps drawing when Mother is unreachable, and it does not move the interrupt off the checkpoint.

The wasi-pi migration plan is not written in this note, in patina-mct at `784c5ed`, or in the public patina tree searched for it. The only statement of it is the two priorities given with this proposal. Priority 1 is move wasi-pi's clock and random to the standards and split wake out as its own Toy. Priority 2 is fix the seeded secure random.

Priority 2 and the proposal point the same way: stop serving deterministic bytes from the secure source.

**Decided: where the replay Toy sits.** The replay Toy sits beside `wasi:random` as its own Toy, the standard plus the missing part and never a fork. It does not replace wasi-pi's random Toy (`patina:pi@0.3.0`) as a whole. It replaces the seeded-secure-random fork inside wasi-pi, which breaks the WASI spec's guarantee that the secure source is real entropy. The replay Toy is the seeded, deterministic source, and `wasi:random` stays on real entropy. The recorded probability stays separate, as already written: a replayed run reuses the recorded model answer. This does not rewrite locked (a)–(f). The arc of this note: started custom, embraced the standards, adapt only where necessary.

Priority 1 is closed by that ruling. "Move random to the standards" means the secure source becomes ordinary `wasi:random/random` and stays unpredictable. Replay is its own Toy beside the standard, not a fork of it and not a replacement of `patina:pi@0.3.0` as a whole. Splitting wake into its own Toy is a different Toy. This ruling does not merge wake and replay, and it does not decide wake.

## Open questions

- Recovery-key loss is a lockout, decided in [Recovery key lost](#recovery-key-lost). A day-to-day key cannot revoke the recovery key. The question is closed.
- Scheme v1 is decided in [Decisions (October 15, 2026)](#decisions-october-15-2026). It signs the command plus the grant under both keys, and the ML-DSA-65 public key sits on the grant fact beside the Ed25519 key. The question is closed.
- Hard cutover is decided in [Migration](#migration). There is no "key and uid" window. The question is closed.
- The replay Toy sits beside `wasi:random` and does not replace `patina:pi@0.3.0` as a whole. It replaces the seeded-secure-random fork inside wasi-pi. Decided in [Proposed: replay Toy](#proposed-replay-toy). `wasi:random/insecure` is not enough on its own; that part is answered there.
- How a frozen Child resumes is decided. Resuming from a freeze counts as a wake under (e). See [Where this conflicts with a–f](#where-this-conflicts-with-af).
- A decision-model Toy for older Children is proposed, not locked. See [Proposed: decision model](#proposed-decision-model). Pause order, the freeze, the per-task handle, and the gateway wording live there.

### Does WIT composition fit for handing Toys to Children?

The idea, not adopted: each Toy is a WIT component. A Child's set of Toys is a composed world. Mother's "fix a Child" swaps one component out of that composition without restarting the Child, which would be gentler than killing and respawning it. This note does not design that wiring, and it does not answer the question.

Read it against a–f, which stay decided:

- **(f)** Interrupts happen between steps, at checkpoints. When can a swap safely happen, and what happens to the state the Toy instance being replaced is holding?
- **(e)** On wake the Child checks in with Mother before touching its own state. The check-in proves policy and generation are current. Does a swapped composition count as a policy or generation change the Child must pick up on that check-in?
- **(a)** Grants are ledgered. A swap is a change of what was granted. Whether the ledger records that swap, and as what kind of fact, is part of the question.
- **(d)** The platform-given handle is the authorization, and a Child is a durable object. Whether that handle stays the same while the composition behind it changes is open. A sibling-lent Toy is not a handle the borrower invented. Under the courier rule, Mother remains the grantor, only an older Child may share, and a delegation is a ledgered attenuation of a handle that Child already holds. That does not answer whether WIT composition is how the handle is wired.
- **(b)** Iroh is the only admin path. If a swap is Mother acting on a Child, it rides that path. **(c)** Mother unreachable is a hard stop, so the Child does not recompose itself while she is gone.
- Whether wasmtime can swap a component in place, or a swap means re-instantiating behind the same handle, is open.
