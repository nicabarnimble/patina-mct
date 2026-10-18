# Admin keys code map

> **Imported 2026-10-18** from working notes (2026-10-14). **Stale line numbers**: they are from `main` at `784c5ed` and have shifted. Use it as a list of what has to change and what has to be deleted, then re-locate every symbol on current `main` before you edit. Phases 1–2 and the optional hybrid signature have landed since. The removal targets for the hard cutover (Phase 4) are still present on `main` at `49dfacd`.


Decisions a–f are locked in [admin-keys.md](admin-keys.md). This file is the code-change map for that note. The decision text below is the locked wording from the map, not a second set of proposals.

Line numbers are from origin/main `784c5ed` and will shift after #45 and #39–#44 merge.

The composition note and the open question “Does WIT composition fit for handing Toys to Children?” live in the design note: [admin-keys.md#does-wit-composition-fit-for-handing-toys-to-children](admin-keys.md#does-wit-composition-fit-for-handing-toys-to-children). That question is open. This map does not answer it and does not wire it.

Sibling sharing is adopted there as the courier rule, refined so only an older Child may share. Mother creates every borrowed handle. A hand-up is a request she reissues as the older Child's own handle, and that Child holds separate handles, not one combined handle: [admin-keys.md#4-children-can-share-toys-with-each-other](admin-keys.md#4-children-can-share-toys-with-each-other). That resolution does not change this map.

A replay Toy for deterministic randomness is proposed, not locked, in [admin-keys.md#proposed-replay-toy](admin-keys.md#proposed-replay-toy). It sits beside `wasi:random` and does not replace `patina:pi@0.3.0` as a whole. It replaces the seeded-secure-random fork. A recorded probability is ledger output, not this Toy's seed. A replayed run reuses that line instead of calling the model again. That proposal does not change this map.

A decision-model Toy for older Children is proposed, not locked, in [admin-keys.md#proposed-decision-model](admin-keys.md#proposed-decision-model). The (f) checkpoint line below points at that proposal. The citations are unchanged.

Recovery-key loss is a lockout, decided in [admin-keys.md#recovery-key-lost](admin-keys.md#recovery-key-lost). That decision does not change this map.

## 1. Decisions
a. Iroh auth: admin keypair is made on the admin's side, and Mother records only the public half. The handoff runs over the existing iroh connection, with no new transport. Bootstrap is either a pre-seeded admin pubkey in deploy config (authorized_keys style) or a single-use, short-lived pairing ticket redeemed over iroh. Either way the ticket is burned in the ledger and the fingerprint (Ed25519+ML-DSA-65) is confirmed on both sides. Grants are scoped, ledgered and revocable. The recovery key gets the startup gate plus key-manage only.
b. Iroh is the only admin path: no UDS admin, no uid/peer_cred check, no open_local. execute_offline_* goes through the same check or gets removed.
c. Mother is essential: children refuse to act without her. Mother unreachable means a hard stop, not failover. Investment goes into Mother never dying: hot standby, replicated state, fast restart.
d. Children are durable objects: no local socket. The only way in is a platform-given handle, and holding the handle is the authorization.
e. On wake, a child re-touches Mother before touching its own state. The handle proves identity, and the re-touch proves it's still her child and its policy is current (revision/generation).
f. A child continues its current task until it needs another one. Mother can interrupt via an iroh message, checked at step checkpoints. If Mother drops, the child finishes the current step and freezes at the next checkpoint.

## 2. Current code → required change
### Auth flow
- crates/mct-daemon/src/control.rs:171 `MctUdsControlMutationHandler`, :189 `authenticate_owner` (401/403 on peer uid) → replace with an admin-key grant check on an iroh ALPN (a,b).
- crates/mct-kernel/src/toy.rs:538 `evaluate_toy_grant_for_call` → generalize the subject to Child|AdminKey and add route scopes plus mandatory expiry for daily keys (a).
- crates/mct-kernel/src/peer/internal.rs:4 `evaluate_hello_internal`; crates/mct-iroh/src/serve.rs:1022 `enforce_hello_binding_signature`, :1264/:1300 `serve_concurrent_with_*` (ALPN dispatch ~:1381–1490) → add an admin ALPN (e.g. `mct/admin/0`, inferred name) next to MCT_HELLO_ALPN/MCT_CALL_ALPN (crates/mct-kernel/src/peer/mod.rs:11-13). The caller's endpoint id is the authenticated key (a).
- crates/mct-iroh/src/identity.rs:91 `sign_peer_binding_signature_ref` → reuse the pattern for the pairing-redeem proof (the new key signs) and the fingerprint. ML-DSA-65 half: not located in my read; verify (a).
- crates/mct-iroh/src/endpoint.rs:239 discarded `online()` timeout (fixed in #41 → RelayNotReady). Admin depends on the endpoint, so this fix is a prerequisite (b,c).
### Local / same-uid admin path (remove)
- control.rs:286/291 `MctControlPlaneAuthPolicy::open_local`; control.rs:519 `.peer_cred()` (`serve_uds_control_stream_with_handlers`) → delete the auth meaning, and drop or limit the UDS (b).
- daemon/control.rs:2430 `RESIDENT_MUTATION_ROUTES` (one uid check for all routes) → each route maps to a scope and is served over iroh. `/pando/record` needs a scope (a,b).
- daemon/control.rs:2990 `run_control_serve_uds_with_state` (UnixListener::bind :2998, mode 0600) → remove (b).
- daemon/resident/serving.rs:309 `run_resident_mother`, :400 UDS bind + :410 peer_cred (isolated startup plane), :831 `spawn_resident_control_task` → replace the startup operator gate's `os-uid` principal with a recovery-key principal over iroh (a,b).
- daemon/resident/local_ingress.rs:8 `LOCAL_CALL_ALPN`, :97 `authenticated_uid_subject`, :311 `preflight_local_submission`, :437 `resident_local_call_handler` (POST /calls over UDS) → local calls move to the iroh call path (b,d).
- daemon/supervisor_lifecycle.rs:565,973,996,1101,1168,1467,1638 `format!("os-uid:…")`, :246 `current_uid`; acquisition.rs:1112 `current_uid` → principal becomes `admin-key:<endpoint id>` (a,b).
- startup.rs `MctIsolatedStartupPlaneV1::handle` / `accept_operator_startup_gate` (per design note; not re-read) → recovery key (a).
### Mother process & state
- daemon/resident/serving.rs:309/482 `run_resident_mother[_with_trigger_runtime]`; resident/observation.rs:299-335 ledger writer `spawn*`; supervisor_lifecycle.rs launchctl (:159-191, :2265) is the only restart mechanism (macOS launchd) → (c) needs a Linux/container supervisor, a fast-restart budget, and a replicated ledger plus standby. None of that exists.
- federation.rs:26-192 `policy_revision` per surface → the source of the "policy current" value for re-touch (e).
### Child model & sockets/IPC
- Children are in-process wasm (wasm.rs:961 `MctWasmComponentRuntime`, :1094/:1125 `invoke_authorized_child_wit_export*`) or a spawned process with piped stdio (process.rs:53/63 `invoke_authorized_child[_bytes]`; supervisor.rs:72/93 `MctProcessSupervisor::spawn_authorized`, stdio null). No child-owned socket exists. → (d) the handle = an unforgeable `AuthorizedChildInvocation` plus `ChildInstance{generation}` (mct-kernel child.rs:246/256). Formalize it as a durable-object handle (inferred). Process children inherit the env/fs, so audit them.
- mct-kernel child.rs:467 / :832 policy_revision checks in `evaluate_child_call_authority` → the basis for re-touch (e). Child approval/assignment live in config, not the ledger (design note), so the re-touch can't be replayed → move them into the ledger (a,e).
- daemon/resident/execution.rs:97 `execute_authorized_resident_child`, :207 process, :616 wit → insert a Mother re-touch before any state access (e).
### Task execution loop
- cycle.rs:28 `run_child_task_cycle` (drain → tick → lease loop → `handle_task`); state.rs:3874 `lease_next_task`, :3916 `mark_task_running` → the checkpoint between tasks lives here. Add an interrupt/Mother-liveness check before each lease and a "frozen awaiting Mother" state (f,c). The decision-model proposal uses this same pause: check, then weigh, then act. No live Mother means freeze at the pause. A grace window is ledgered policy that starts at the next checkpoint, not a pass and not a mid-step expiry. A young Child's pause is only the check. The per-task grant is a fresh handle Mother gives at task start. Mother's grant enforces who may call. The gateway holds the credential and honors the handle. A recorded probability is ledger output, reused on replay, not the replay Toy's seed. Resume after a freeze counts as a wake under (e) and reuses that check-in. It does not add a second pause.
- wasm.rs:970 `epoch_interruption(true)`, :1042 `set_epoch_deadline(1)`, :1053 `increment_epoch` (deadline-only trap thread) → extend this into a Mother-interrupt source. Note: a trap is a kill, not a pause at a checkpoint. A graceful interrupt needs a host-call checkpoint (f).
- process.rs loop with try_wait + timeout → no interrupt or checkpoint protocol for process children (f).
- daemon/resident/pipeline.rs:533–828 `execute_resident_call*`; trigger_scheduler.rs:814 `spawn_firing`, :1657 `run_trigger_scheduler_with_runtime` → gate on Mother authority freshness (c,f).
### Ledger / replay
- mct-observation/src/lib.rs:121 `AuthorityChangeV1`, :919 `execute_authority_mutation`, :1934 `replay_authority_entries`, :828 `OperatorGatedNonvirgin` → add AdminKeyGrantPut/Revoke, PairingTicketIssued/Consumed (hash only, single-use), and a recovery-key principal. Add a `GrantShapingSourceV1` admin-key source. Replay rejects a second consume (a).
### CLI / offline
- daemon/control.rs:423,1032,1696,2256,2746,2881 `execute_offline_{peer_mutation,artifact_stage,registry_mutation,administrative_mutation,child_mutation,identity_mutation}`; daemon/triggers.rs:429 `execute_offline_trigger_mutation` → remove in the same build as the new iroh admin-key path. No switchover window (b).
- daemon/cli_admin.rs:730 `try_resident_control_mutation`, :625 `query_resident_status`, :620 `default_control_uds_path`, :84 `execute_cli_administrative_mutation` → the CLI becomes an iroh admin client that holds the local admin secret (a,b).
### Config / bootstrap
- config.rs:105 `MctDaemonConfigStore`, :405 `save`; `upsert_peer`/`ensure_local_identity`/`load_or_create_node_secret_key_hex` (per note) → add `authorized_admin_keys` (public keys only) to the deploy config. The init path appends the first grant before listening. Keep the node secret separate from admin keys (a).
### Tests / CI
- Many tests bind a UDS: daemon/control.rs:3483–5055, control.rs:882–1061, cli_admin.rs:1125, local_ingress.rs:630/659 → rewrite them against an iroh admin harness (crates/mct-iroh/src/test_support.rs). Add tests: deny-by-default, ticket single-use and replay, fingerprint mismatch, Mother-down freeze, interrupt at a checkpoint. .github/workflows/ci.yml (clippy -D warnings, audit, fuzz.yml).

## 3. Gaps (nothing exists)
- No admin-key grant type, admin ALPN, pairing ticket or fingerprint UX.
- No ML-DSA-65 code located (not searched exhaustively).
- No Mother→child interrupt channel. Epoch trap only fires on the deadline.
- No checkpoint/freeze/resume state for tasks (leases exist; resume after Mother loss doesn't).
- No child→Mother liveness/re-touch on wake. No "wake" concept: children are invoked per call.
- No hot standby, ledger replication or Linux supervisor. Restart is launchd only.
- Child/peer facts sit in config rather than the ledger, so revocations don't replay.

## 4. Order & risks
1. Land #45 then #39–#44 first. #43/#44 touch daemon/control.rs, serving.rs, local_ingress.rs, supervisor_lifecycle.rs and state.rs, so the line numbers above will shift.
2. Ledger: admin grant + ticket facts + replay (observation lib.rs).
3. Kernel: generalize the toy grant check to the admin subject + scopes.
4. mct-iroh admin ALPN + redeem flow + fingerprint.
5. Daemon: route table over iroh, and in that same build delete the UDS/peer_cred/open_local/offline paths and the os-uid principals, and move the CLI to an iroh client. MCT isn't live, so the old path does not linger.
6. Move child/peer facts into the ledger. Then re-touch on child execution (e).
7. Checkpoint/interrupt/freeze in cycle.rs + the wasm host checkpoint (f).
8. Mother HA: supervisor, replication, standby (c). Largest effort, and needs a separate design.

Risks:
- Lockout if the iroh endpoint fails, since there's no local escape hatch (accepted by decision b). Needs a fast-restart and a recovery-key runbook.
- Re-init with a surviving key file re-grants an old key (design note §1).
- Offline CLI removal does not protect live ops scripts. MCT isn't live. The old path (`execute_offline_*`, UDS, `peer_cred`, `open_local`, os-uid) comes out in the same build as the new iroh admin-key path.
- Treating an epoch trap as an interrupt would kill mid-step and violate (f).
- Hard stop on Mother loss turns any network blip into a fleet stall.
- A hot standby with two writers risks forking the hash chain, so it needs a single-writer lease.