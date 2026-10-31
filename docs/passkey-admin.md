# Passkey/day-key administration

D-58 replaces the guardian/hybrid ceremony for new administration. Historical
code/tests remain; no old ledger is automatically promoted or rewritten.

## Current adapter

`mct-daemon passkey-admin` supports explicit bootstrap, signed lease issuance,
revocation and `/status`. Other scoped commands have a canonical BeforeEffect
host API but **are not connected to resident command adapters or Iroh yet**.
The existing UID/offline daemon refuses passkey-profile ledgers. This is not a
completed resident F1 cutover or a claim of hosted 1Password testing.

1. On an operator-controlled HTTPS RP origin, import `scripts/passkey-admin.mjs`
   and call `registerRoot(rpId, userName)`. Choose 1Password in the browser.
   Independently inspect/install the returned public registration as a private
   JSON pin file; never derive that pin from an incoming bootstrap request.
2. Use an empty private root and explicit Mother/ledger IDs. Prepare genesis:
   `mct-daemon passkey-admin prepare-genesis --root ROOT --mother ID --ledger-id ID --registered-root PIN.json`.
3. Review the returned `intent`, then call `signPrepared(prepared)` on the same
   HTTPS origin. It recomputes the challenge before requesting an assertion.
   Save the result as private JSON. Run `bootstrap` with the same options plus
   `--request SIGNED.json`. Mother verifies RP, origin, challenge, ES256, UP and UV.
4. `day-keygen --secret-output SECRET.json` creates a private Ed25519 seed file
   and prints only the public half. Do not replace/reuse existing secret files.
5. Prepare an action with `prepare --root ROOT --mother ID --ledger-id ID
   --action ACTION.json --signer root`. For issuance, ACTION is
   `{"action":"issue_day_key","grant_id":"day-a","public_half":PUBLIC_HALF,
   "scopes":["status"],"expires_at":"RFC3339 expiry"}`. Expiry is mandatory,
   later than Mother time and at most 24 hours away. Sign via the browser and
   submit with `apply --root ROOT --mother ID --ledger-id ID --request SIGNED.json`.
6. For a day-key command, prepare with `--signer day-a`, extract the returned
   `intent`, and use `sign-day --intent INTENT.json --grant day-a --secret SECRET.json`.
   Submit with `apply`. Status action is
   `{"action":"command","scope":"status","route":"/status","body":""}`.
7. Revoke with `{"action":"revoke_day_key","grant_id":"day-a"}`, signed by
   the root or an active `key_manage` day key. Revocation survives restart;
   unrelated grants remain active. Day keys cannot mint or extend leases.

All input files use private storage checks (regular, single-link, owner-private,
no-follow ancestry); provision private directories/files deliberately, e.g. with
operator `umask 077`. Production never changes umask or silently repairs modes.
Intents expire after at most five minutes and bind the exact current authority
snapshot. Reprepare after any authority change. A failed/uncertain admission is
not permission to retry an effect: complete surviving frames consume nonces.
Registration is an explicit operator trust boundary; no attestation CA policy,
root rotation, persistent external rollback anchor or hosted browser integration
is claimed. Synced passkey counters are not treated as monotonic freshness.

## Checks

`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all --check`, `git diff --check`, `bash scripts/ci-tier0.sh specs`.
Tier-0 specs also runs the browser helper's Node tests; Node 22+ is required.
