# Pocket ID-gated MCT root approval

Pocket ID authenticates access to the approval page. It is **not** MCT authority.
The page still signs the exact prepared intent with the independently pinned
WebAuthn root, and Mother still performs every canonical verification.

```text
Pocket ID login                 page access only
one-use queue fragment          claim one prepared job
explicit Approve reviewed action click
WebAuthn root assertion         cryptographic signing evidence
Mother verification             authority decision and nonce consumption
```

The gate has no Mother submission endpoint and stores no MCT signing key.

## Credential decision

Use a dedicated MCT signing credential in the same 1Password account/vault for
now. Register it at the exact HTTPS approval origin and independently install its
`registered-root.json` (`credential_id`, ES256 public key, RP ID and origin).

Do not claim that the passkey used to log in to Pocket ID can be reused. A
WebAuthn credential is RP-scoped, and no current evidence proves that the
Pocket ID credential has the algorithm, public-key export, credential ID, RP ID
and exact origin that Mother pins. Reuse needs a separate reviewed enrollment
and compatibility proof.

## Processes and private files

The existing queue remains loopback-only. Run it on a private port while keeping
its configured origin equal to the public approval origin:

```sh
umask 077
python3 scripts/serve-passkey-tap.py \
  --session "$SESSION" \
  --origin 'https://approval.example.test' \
  --listen '127.0.0.1:9443' \
  --cert "$QUEUE_CERT" --key "$QUEUE_KEY"
```

`QUEUE_CERT` must be valid for the approval hostname. The OIDC gate verifies it
using `MCT_APPROVAL_UPSTREAM_CA_FILE`; certificate verification is never disabled.
The queue still requires its private Unix socket for enqueue/open operations.

Provision these outside Git, with private ownership and mode 0600:

- OIDC client secret file;
- JSON subject allowlist, for example `["opaque-pocket-id-sub"]`;
- gate TLS private key;
- queue TLS private key;
- MCT root pin and signed artifacts.

The allowlist uses exact immutable OIDC `sub` values, not email or display name.

## Gate configuration

```sh
export MCT_APPROVAL_ORIGIN='https://approval.example.test'
export MCT_APPROVAL_LISTEN='127.0.0.1:8443'
export MCT_APPROVAL_UPSTREAM='https://127.0.0.1:9443'
export MCT_APPROVAL_UPSTREAM_CA_FILE='/private/queue-ca.pem'
export MCT_APPROVAL_TLS_CERT_FILE='/private/approval-cert.pem'
export MCT_APPROVAL_TLS_KEY_FILE='/private/approval-key.pem'

export MCT_OIDC_ISSUER='https://pocket-id.example.test'
export MCT_OIDC_CLIENT_ID='mct-approval'
export MCT_OIDC_CLIENT_SECRET_FILE='/private/oidc-client-secret'
export MCT_OIDC_ALLOWED_SUBJECTS_FILE='/private/allowed-subjects.json'
export MCT_OIDC_TOKEN_AUTH_METHOD='client_secret_basic'
export MCT_APPROVAL_SESSION_SECONDS='900'

node scripts/pocketid-approval-gate.mjs
```

Register this exact callback in Pocket ID:

```text
https://approval.example.test/oidc/callback
```

The implementation uses Authorization Code + PKCE S256, state and nonce. It
bounds pending logins and live sessions to 64 each, bounds requests/responses,
and limits concurrent HTTPS connections. It validates the provider signature
through discovery/JWKS, exact issuer, client
audience, `azp` for multi-audience tokens, expiry/not-before/issued-at bounds,
nonce and the exact subject allowlist. Supported ID-token algorithms are RS256
and ES256. Configure `client_secret_post` only when the reviewed Pocket ID client
requires it.

A reverse proxy may expose the gate, but it must preserve the exact HTTPS Host
and origin. The queue itself stays unreachable except from loopback. Pocket ID,
router, DNS and Unraid changes are target operations owned by Unraid Grok and are
not performed by this repository change.

## Ceremony

1. Authenticate to Pocket ID.
2. Open the one-use URL returned by `passkey-approve.py ... open`. Login alone has
   no queue capability and cannot claim a job.
3. The page exchanges the URL fragment for the independent queue cookie/page ID.
4. Review the pinned root identity (credential ID, RP ID and origin) and complete
   prepared intent displayed on the page, including Mother, action, scopes,
   issued/expiry times and nonce.
5. Click **Approve reviewed action**. This explicit click is required before every
   WebAuthn request. Pocket ID login is not signing consent.
6. Select the dedicated MCT root credential in 1Password and approve required user
   verification.
7. The page returns the assertion to the waiting CLI. The CLI submits it; Mother
   recomputes and verifies the canonical challenge, pinned root, RP ID, exact
   origin, ES256 signature, UP/UV, authority snapshot, nonce and expiry.

Tampering with the action, scopes or expiry changes the challenge and is refused
before WebAuthn. Replay is refused by both the one-use OIDC login transaction and
Mother's canonical nonce consumption. A lost reply is uncertain, never permission
to retry automatically.

## Local development compatibility

The original loopback-only queue can still serve the page without the OIDC gate.
The page labels that mode explicitly. It remains a local development/live-tap
path, not the Pocket ID-hosted deployment.

## Verification

```sh
node --test scripts/passkey-admin.test.mjs scripts/pocketid-approval-gate.test.mjs
PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-passkey-queue.py
node --test scripts/passkey-approval.e2e.test.mjs
cargo test --locked -p mct-daemon --test passkey_admin
bash scripts/ci-tier0.sh specs
```

Independent BOX testing must repeat wrong issuer, audience/authorized-party,
nonce, signature, expiry, subject, CSRF, origin, queue-fragment and callback
replay negatives. Studio output is not BOX proof.
