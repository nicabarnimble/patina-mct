# Board ToyGrant: one signed grant, ordinary Wasmtime linking

Design sketch, not implementation. Implementation anchors verified against `origin/main` at `58b1e3e93f452f882c2c6c145d1bc8fcfad9e522`. One Mother, one approved Child, one board; approval once, one post per invocation, repeated invocations without another approval. No new VM, guest authorization token, or Diplomat.

## WIT and stored grant

```wit
package mct:board@0.1.0;
interface board {
    enum kind { decision, question, status }
    record entry { id: string, kind: kind, text: string, topic: string, refs: list<string> }
    record receipt { sequence: u64, entry-hash: string }
    enum post-error { invalid, conflict, denied, unavailable }
    post: func(value: entry) -> result<receipt, post-error>;
}
interface child {
    use board.{entry, receipt, post-error};
    run: func(value: entry) -> result<receipt, post-error>;
}
world board-child { import board; export child; }
```

Toy ID `board.post` maps to `mct:board/board@0.1.0`, function `post`. Mother binds the board ID in the host import. Guest data cannot select another board or supply attribution. Host limits: 280 Unicode scalar values for text, 128 UTF-8 bytes each for ID/topic, eight refs of 512 bytes; no ref fetching.

Reuse `crates/mct-kernel/src/toy.rs:ToyGrant`: unique grant ID; exact subject `{child_name, artifact_id, artifact_version, assignment_id, caller_node_id}`; toy `board.post`; scope `{vision_id, node_id, resource_id=board_id, allowed_actions=[post]}`; `not_before` maps to `constraints.starts_at`; mandatory exclusive `expires_at`; `max_uses=None`. Mother assigns state, revisions and observation provenance. `issuer_id` records the verified signer. Signature and nonce stay in the issuance fact through `crates/mct-kernel/src/admin_sig/day_key.rs:PasskeyAdminRequestV1` and `crates/mct-kernel/src/admin_key/passkey.rs:PasskeyAdminIntentV1`, not guest arguments.

## Issue/check/revoke path

| Step | Design and existing HEAD anchors |
|---|---|
| **Issue** | The 1Password passkey approves an Ed25519 admin lease; that lease signs issuance once. Add a dedicated fixed-template board-grant scope and issue/revoke routes. Extend signature/scope/nonce checking in `crates/mct-kernel/src/admin_sig/day_key.rs:evaluate_passkey_admin_transition` and dispatch in `crates/mct-daemon/src/passkey_admin.rs:execute_passkey_administration`. |
| **Persist** | One signed canonical fact consumes the nonce and installs the grant, explicitly admitting the fixed catalog entry. Extend `crates/mct-observation/src/passkey_admin.rs:JsonlObservationLedger::commit_passkey_at` and `crates/mct-observation/src/passkey_admin.rs:replay_fact`. No separate authoritative SQLite write after nonce consumption. |
| **Check before linking** | Reuse `crates/mct-daemon/src/daemon/cli_runtime.rs:authorize_cli_toy` and `crates/mct-kernel/src/toy.rs:evaluate_toy_grant_for_snapshot`. Require independently approved Child/artifact authority. Persist admission before instantiation. Denial means no board adapter and no executable instance. |
| **Link/run/drop** | Extend `crates/mct-daemon/src/wasm.rs:validate_wit_host_imports_for_adapters` and `crates/mct-daemon/src/wasm.rs:link_wit_host_import_adapters`. Ordinary component linking and Canonical ABI supply the authorized import. `crates/mct-daemon/src/wasm.rs:MctWasmComponentRuntime::invoke_wit_export_after_contract_check` invokes the Child and drops its invocation-local Store. No permissive stub. |
| **Post** | Admit and append through the existing ledger owner, using `crates/mct-daemon/src/authority_order.rs:MotherAuthorityOrderV1::admit_effect` and `crates/mct-observation/src/lib.rs:JsonlObservationLedger::append_durable_batch`. Add a versioned post fact with host attribution/grant ID, never authority changes. Receipt follows sync. Replayed `(board, subject, post-id)` deduplication returns the original receipt for identical content; changed content conflicts. The board is a projection. |
| **Revoke** | A fresh signed nonce records terminal `Revoked` through the same commit/replay path. Future pre-link checks refuse the import. Retain `crates/mct-kernel/src/toy.rs:AuthorizedToyCall::admit_effect_with_snapshot` for revoke/expiry races after linking; existing authority ordering decides whether revoke or append wins. |

## Decided policy

Doug's decisions, 2026-11-01. Testing phase; keep limits loose.

1. ToyGrant expiry is mandatory and is **not capped by the issuing day key**. The default lifetime is **30 days**, which the issuer can override.
2. Revoking the day key stops **new issuance only**. Existing ToyGrants stay valid until their own revocation or expiry.
3. **Grant IDs are never reused**, including after revocation or expiry.

## Deviations and one proving test

Pre-link checks and conditional linking already exist. Wasmtime cannot retract a linked import; current per-effect Mother checks preserve live revocation beyond link-time admission. Keep them, rather than invent a new runtime mechanism. Current buffered effect reports must become synchronous board append. D-58 lacks ToyGrant transitions and application execution: `crates/mct-daemon/src/startup.rs:classify_authority_startup` still refuses the legacy-runtime shortcut. Add bounded D-58 invocation wiring; defer portable handles and remote ingress.

**One proposed integration test:** real Wasmtime Child, temporary Mother ledger, synthetic WebAuthn/Ed25519 signer. Deny before issuance without linking. Issue once and reopen; post A and drop Store. Exercise issuing-day-key expiry and revocation: new issuance fails, but a fresh instance can post B using the same unexpired ToyGrant. Retry A after lost acknowledgement and require its original receipt. Revoke; deny both the next instantiation and a paused, already-linked invocation before append. Reject expired ToyGrants and reused issuance nonces. Check the 30-day default and an issuer override. Reject grant-ID reuse after revocation or expiry. Rebuild the board and require exactly A and B. This proves the in-process grant-to-link-to-ledger path, not live 1Password UI.

**Design choice:** Model the Domain favors a board-bound import over generic ledger-write access, which exposes too much authority. No implementation or test execution is claimed.

**Verification.** All 16 cited implementation symbols were checked against the `origin/main` revision named above with `git show <revision>:<file> | rg -n <declaration>`. This records source verification, not execution of the proposed integration test.
