# QR-to-phone approval queue

The approval queue is transport, not authority. It is reachable only on loopback
and over an owner-private Unix socket. It never stores an MCT root private key,
invokes Mother, or exposes a generic administration endpoint.

## Closed request model

```text
ApprovalRequest = GrantRequest | AccessProofRequest

GrantRequest = {
  asker, Mother, permission rows, fixed grant end
}

AccessProofRequest = {
  asker, title, wiki.nicabar.org path, reason,
  expected subject = nicabar, required group = wiki-admin
}
```

Each request owns a random 128-bit one-use URL capability, a separate display-only
capability, a four-character match code, a nonce and a ten-minute deadline.
The display capability can read status and render the QR; it cannot claim, approve,
decline, check or skip the request.

```text
Waiting -> Opened -> Prepared -> Signed -> Approved     (grant)
                  \-> Declined
Opened -> Passed | Failed | Skipped                     (proof)
Any unconsumed live state -> Expired
Any mismatch/replay/tamper -> Refused
```

Approved, declined, passed, failed, skipped and refused consume the code. Expiry
kills it. A failed access proof therefore requires a new QR.

## Why preparation follows authentication

The QR lasts ten minutes, while Mother permits an administration intent to live
for at most five minutes. The bot creates the QR first. After Pocket ID returns the
allowed subject, the queue moves to `Opened`; only then does the waiting CLI ask
Mother to prepare the exact intent. The phone remains on “You’re signed in” until
that intent is published. Mother’s existing time bound is unchanged.

## Independent capabilities

```text
Pocket ID session       may view one bound request
URL capability          identifies one ten-minute ceremony
MCT WebAuthn assertion  signs the exact prepared intent
Mother replay           makes the authority decision
wiki signed result      proves pass/fail for one path; grants nothing
```

Login alone creates no queue artifact, WebAuthn proof, grant or Mother command.
The browser cannot enqueue work or choose an output path. The queue structurally
binds the returned assertion to the published intent and credential before writing
one private artifact. The waiting CLI takes it once; Mother still verifies the
pinned credential, RP ID, exact origin, ES256 signature, UP/UV, challenge, authority
snapshot, nonce, expiry and action.

## Hosted boundary

The public gate is `https://mct-approval.nicabar.org`; that hostname is the fixed
MCT passkey RP ID. A scan opens `/a/<capability>`. Before Pocket ID, the page shows
only request kind and the match code. The gate stores the capability in a short-
lived server-side link session referenced by a `Secure; HttpOnly; SameSite=Lax`
cookie. OIDC state is bound to that session. The callback obtains the capability
from server state, not a query parameter, claims the queue once, and returns to the
same path.

Phone and desktop status use SSE. Reconnection reads canonical queue state; it
never repeats WebAuthn or an access check automatically. The queue remains bound
to `127.0.0.1`; reverse proxies must not expose it.

See [`docs/pocketid-approval.md`](../pocketid-approval.md) for deployment and the
Grok Life proof-handler contract.
