# Passkey/day-key administration

D-58 replaces the guardian/hybrid ceremony for new administration. Historical
code/tests remain; no old ledger is automatically promoted or rewritten.

## Current adapter

`mct-daemon passkey-admin` supports explicit bootstrap, signed lease issuance,
revocation and `/status`, locally or through the admin-only resident Iroh mode.
`mct/admin/1` never treats an EndpointId, peer hello or UID as admin authority.
The same resident ledger actor serializes verification, nonce consumption,
revocation and required durability. Other command adapters and the full
Child/Toy runtime cutover remain unbuilt: the legacy runtime still refuses
passkey ledgers. Doug reported a successful live 1Password run of the original
manual flow on build `ab193bb`; the new queued flow still needs its own live rerun.

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
snapshot. Reprepare after any authority change. Bootstrap validates the trusted
setup, signed intent and signature before exclusively creating its ledger. An
expired, malformed or incorrectly signed genesis therefore leaves a virgin root
available for a fresh prepare/sign/bootstrap ceremony, without deleting files.
An already existing ledger, including a zero-byte file, remains nonvirgin. No
automatic cleanup or reinterpretation of old empty files is performed. I/O failure
after admission can still leave uncertain storage and requires inspection.
A failed/uncertain admission is not permission to retry an effect: complete
surviving frames consume nonces.
Registration is an explicit operator trust boundary; no attestation CA policy,
root rotation, persistent external rollback anchor or hosted browser integration
is claimed. Synced passkey counters are not treated as monotonic freshness.

## Resident Iroh key-management slice

After bootstrap, run:
```sh
cargo run -p mct-daemon -- passkey-admin serve-iroh \
  --root "$HOME/.mct-passkey" --mother mother-a --ledger-id ledger-a \
  --ticket-output "$HOME/.mct-passkey/admin-ticket.json"
```
This is admin-only (no Child runtime, peer ALPN, UID listener or offline fallback).
Start it **before** preparing intents: every restart establishes a fresh epoch.
Prepare/sign using the steps above, then send instead of applying locally:
```sh
cargo run -p mct-daemon -- passkey-admin send-iroh \
  --ticket "$HOME/.mct-passkey/admin-ticket.json" --request SIGNED.json
```
The initial transport is direct/relay-disabled. Preparing intents currently
requires local access to the canonical ledger; remote snapshot acquisition is
not implemented. Ctrl-C closes serving and drains the ledger actor. An uncertain
reply is not permission to retry. Unmigrated routes deny, not route to legacy
mutation helpers.

## Approvals-only local queue

Use [the short RUN recipe](passkey-approval-RUN.md) for a fresh live run in your
default browser with 1Password. The opener uses the system default,
not a fixed browser application.

The `--session` mode of `serve-passkey-tap.py` serves the authenticated queue page,
not the file-picker page. `passkey-approve.py` queues registration or a root
operation and waits. It prepares only after a foreground page claims that work,
then receives the private signed request and invokes bootstrap or `send-iroh`.
The page never applies administration. Existing low-level CLI commands remain
available; they do not implicitly connect to a broker.

The CDP test driver completed the virtual-authenticator test with zero file
selections, downloads, Sign clicks or Approve clicks, and no transient user
activation. An injected provider-refusal test also passed with one Approve click
per root action. That injected refusal does not establish a browser requirement.
Actual 1Password provider selection, unlock and approval UI remain subject to
Doug's live rerun. The page offers at most one explicit retry after NotAllowedError,
which can mean refusal/cancellation as well as a gesture restriction.

Only the private Unix socket can enqueue work. The HTTPS page exchanges a
one-use fragment capability for an HttpOnly cookie and a separate page identity.
Each offer belongs to that page and has a renewable 30-second lease. Completion
must match the current page, offer capability and exact queued intent. See [the queue contract](design/passkey-approval-queue.md) for
states, limits, failure handling and trust boundaries. Keep the page focused.
Preparation is just in time, not an extension of Mother's five-minute window.
If Doug leaves an already visible prompt, it can still expire. An expired attempt
stops without automatic resubmission. If a page disappears or stops responding
before delivery, run `passkey-approve.py --session SESSION open` again. New page
authentication fences the old page immediately. The same unsigned job becomes
available after cancellation acknowledgement or lease expiry. Do not restart the
demo for this handoff. A root intent keeps its original expiry. Inspect canonical
state before any retry after a lost submission reply or interrupted run.

The page cancels WebAuthn when hidden or unloaded, or when it loses its lease.
A frozen external provider UI cannot be forcibly dismissed by the broker, but its
stale result is always refused. An active broker from an older revision does not
acquire this protocol through source edits; start a matching broker only after
finishing the existing live run.

## Original manual 1Password tap test

Prerequisites: your default browser with 1Password enabled and unlocked, Python 3,
and `mkcert`. The CA installation below is an explicit operator action, not
something the agent runs. Use a fresh test root; do not migrate an existing RP pin.

```sh
mkdir -p -m 700 "$HOME/.mct-passkey-tap"
mkcert -install
mkcert -cert-file "$HOME/.mct-passkey-tap/cert.pem" -key-file "$HOME/.mct-passkey-tap/key.pem" mct.localhost
python3 scripts/serve-passkey-tap.py --cert "$HOME/.mct-passkey-tap/cert.pem" --key "$HOME/.mct-passkey-tap/key.pem"
```

In another terminal:
```sh
open https://mct.localhost:8443
```

Click **Register a new passkey root**, selecting 1Password. Install the downloaded
`registered-root.json` as a private pin (0600 in a private directory), then use
the prepare-genesis command above. Save its output as private `prepared.json`.
On the page, select that file, inspect the target/scope/expiry, and click
**Sign the reviewed intent**. The real tap must return a downloaded
`signed-admin.json`; provision it privately before passing it to `bootstrap`.
For a fresh local test, after downloading the registration:
```sh
umask 077
mkdir -m 700 "$HOME/.mct-passkey-tap/mother"
install -m 600 "$HOME/Downloads/registered-root.json" "$HOME/.mct-passkey-tap/registered-root.json"
cargo run -p mct-daemon -- passkey-admin prepare-genesis --root "$HOME/.mct-passkey-tap/mother" --mother mother-a --ledger-id ledger-a --registered-root "$HOME/.mct-passkey-tap/registered-root.json" > "$HOME/.mct-passkey-tap/prepared.json"
```
Upload that prepared file to the page and tap. Then, selecting the newly
downloaded assertion (not an older duplicate download):
```sh
install -m 600 "$HOME/Downloads/signed-admin.json" "$HOME/.mct-passkey-tap/signed-admin.json"
cargo run -p mct-daemon -- passkey-admin bootstrap --root "$HOME/.mct-passkey-tap/mother" --mother mother-a --ledger-id ledger-a --registered-root "$HOME/.mct-passkey-tap/registered-root.json" --request "$HOME/.mct-passkey-tap/signed-admin.json"
```
Repeat prepare/sign for day-key issuance, then use `send-iroh` for the resident,
substituting this test Mother's root in the resident commands above.

Without `--session`, the original page is loopback-only HTTPS and serves only three fixed assets. Pins,
intents and assertions stay in browser-selected files; it has no upload or
Mother mutation endpoint. A browser must trust the certificate; do not bypass
certificate errors. This is a manual-test harness, not a production HTTPS
administration service. Doug reported this original flow passed on `ab193bb`;
that report is separate from automated tests and the queued-flow live rerun.

## Checks

`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all --check`, `git diff --check`, `bash scripts/ci-tier0.sh specs`.
Tier-0 specs also runs the browser helper's Node tests; Node 22+ is required.
