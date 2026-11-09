# MCT QR-to-phone approval

This is the authoritative NIC-62 approval UX. Pocket ID authenticates page access;
it is never MCT authority. The MCT passkey signs the exact request, and Mother
independently decides every grant.

## Operator flow

The bot prints and may display a version-4/M QR, a match code, the request summary,
a ten-minute countdown and the fallback URL. Scan it directly:

```text
https://mct-approval.nicabar.org/a/<one-use 128-bit capability>
```

1. Confirm the phone and bot show the same short code.
2. Sign in at Pocket ID. No request detail is disclosed before this step.
3. Review the request on the phone.
4. For an MCT grant, tap **Approve with MCT passkey** and select the dedicated MCT
   credential in 1Password. Pocket ID login alone grants nothing.
5. For a NIC-132 check, tap **Check I can open it**. The browser performs a top-level
   redirect through the wiki proof handler. The bot receives only pass/fail.

The QR capability survives OIDC through a server-side link session referenced by a
`Secure; HttpOnly; SameSite=Lax` cookie. It is never placed in the callback query.
Capability URLs must not be copied to shell traces, chat, tickets or durable logs.

## Processes

The queue stays loopback-only:

```sh
umask 077
python3 scripts/serve-passkey-tap.py \
  --session "$SESSION" \
  --origin 'https://mct-approval.nicabar.org' \
  --listen '127.0.0.1:9443' \
  --cert "$QUEUE_CERT" --key "$QUEUE_KEY"
```

The gate requires:

```sh
export MCT_APPROVAL_ORIGIN='https://mct-approval.nicabar.org'
export MCT_APPROVAL_LISTEN='127.0.0.1:8443'
export MCT_APPROVAL_UPSTREAM='https://127.0.0.1:9443'
export MCT_APPROVAL_UPSTREAM_CA_FILE='/private/queue-ca.pem'
export MCT_APPROVAL_TLS_CERT_FILE='/private/approval-cert.pem'
export MCT_APPROVAL_TLS_KEY_FILE='/private/approval-key.pem'

export MCT_OIDC_ISSUER='https://id.nicabar.org'
export MCT_OIDC_CLIENT_ID='mct-approval'
export MCT_OIDC_CLIENT_SECRET_FILE='/private/oidc-client-secret'
export MCT_OIDC_ALLOWED_SUBJECTS_FILE='/private/allowed-subjects.json'
export MCT_OIDC_TOKEN_AUTH_METHOD='client_secret_basic'
export MCT_APPROVAL_SESSION_SECONDS='900'

export MCT_APPROVAL_PROOF_SIGNING_KEY_FILE='/private/mct-proof-ed25519.pem'
export MCT_APPROVAL_WIKI_VERIFY_KEY_FILE='/private/wiki-proof-ed25519.pub.pem'
export MCT_APPROVAL_WIKI_PROOF_URL='https://wiki.nicabar.org/library/_mct-proof'

node scripts/pocketid-approval-gate.mjs
```

Register this exact callback in Pocket ID:

```text
https://mct-approval.nicabar.org/oidc/callback
```

All private files must be single-link, owner-private regular files. The subject
allowlist contains immutable OIDC `sub` values, never email addresses. The gate
validates Authorization Code + PKCE, state, nonce, issuer, audience, `azp`, JWKS
signature and token timing. It stores no MCT root key and has no Mother route.

## Bot commands

The CLI creates the QR itself; there is no separate `open` handoff:

```sh
python3 scripts/passkey-approve.py --session "$SESSION" --binary "$MCT" \
  bootstrap --root "$MOTHER" --mother unraid-mother --ledger-id ledger \
  --registered-root "$PIN"

python3 scripts/passkey-approve.py --session "$SESSION" --binary "$MCT" \
  issue --root "$MOTHER" --mother unraid-mother --ledger-id ledger \
  --ticket "$TICKET" --grant bot-day --public-key "$PUBLIC" \
  --scope status --lease-seconds 86400

python3 scripts/passkey-approve.py --session "$SESSION" \
  proof --title 'The Example Handbook' \
  --path '/library/example-handbook.pdf' \
  --why 'Checking the library release works for you (NIC-132)'
```

Exit codes are fixed:

| Kind | Outcome | Exit |
|---|---|---:|
| grant | approved | 0 |
| grant | declined | 10 |
| grant | expired | 11 |
| grant | refused | 12 |
| proof | pass | 0 |
| proof | skipped | 10 |
| proof | expired | 11 |
| proof | refused | 12 |
| proof | denied | 20 |
| proof | not found | 21 |

A registration pin is created independently at the exact approval origin. A
command cannot register or replace its own trusted root.

## Grok Life wiki handler contract — specification only

This repository implements only the `mct-approval` side. Grok Life owns the wiki
handler and Caddy routing. No wiki or Caddy change is included here.

`GET https://wiki.nicabar.org/library/_mct-proof?t=<ticket>` must:

1. verify the Ed25519 ticket from mct-approval;
2. require `kind = mct-access-proof-ticket/v1`;
3. enforce its two-minute expiry, exact request ID, nonce, path, fixed return URL,
   expected subject `nicabar`, and required group `wiki-admin`;
4. atomically consume the request ID and nonce before returning a decision, and
   reject every later use of that ticket;
5. verify the Cloudflare Access JWT issuer, audience and expiry;
6. verify the Access identity is nicabar **and** belongs to `wiki-admin`;
7. run the same authorization/existence decision as a real read of the exact path,
   without returning file bytes;
8. return one of `allowed/ok`, `denied/denied`, or
   `not_found/not_found` in a signed result;
9. issue a 303 to the ticket’s fixed return URL with `r=<signed-result>`.

The `t=` ticket is present in the wiki request URL. It can therefore enter browser
history and request logs held by the wiki origin, its reverse proxy, and Cloudflare.
Treat those logs as sensitive: redact query strings, keep only the minimum retention,
and never copy a ticket into diagnostics. `Referrer-Policy: no-referrer` prevents the
approval page URL from being sent as a referrer, but it cannot hide the wiki request
URL from the systems serving that request. The ticket expires after two minutes and
is one-use under the handler contract above. After the first signed result,
mct-approval also consumes the approval code, so another result from the same ticket
cannot create a second pass/fail outcome or any MCT authority.

The signed result schema is:

```json
{
  "kind": "mct-access-proof-result/v1",
  "request_id": "exact ticket request",
  "nonce": "exact ticket nonce",
  "path": "/library/example-handbook.pdf",
  "subject": "nicabar",
  "outcome": "allowed | denied | not_found",
  "reason": "ok | denied | not_found",
  "iat": 0,
  "exp": 0
}
```

The result lifetime is at most 60 seconds. mct-approval verifies the wiki signature,
request ID, nonce, path, subject, outcome pair and times server-side. Valid failure,
forgery, mismatch, expiry and replay all leave no authority; accepted pass/fail and
refusal consume the code. A retry requires a new QR.

## Verification boundary

The hosted browser regression uses non-localhost HTTPS names, a mocked Pocket ID
issuer, a headless Chrome virtual authenticator, the real queue and CLI, Mother,
and a mocked wiki handler implementing the contract above. It is not proof of real
Pocket ID, Cloudflare Access, 1Password selection/unlock, DNS, reverse proxy or
Unraid behavior. Those remain owner-operated target checks.
