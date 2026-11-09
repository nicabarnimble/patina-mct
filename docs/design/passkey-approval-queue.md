# Local passkey approval queue

## Contract and caller usage

This is an operator-controlled transport for D-58 assertions, not a new authority
source. The caller queues work, waits for an authenticated foreground page, then
prepares a fresh intent through the existing CLI. The page shows the exact intent
and requests WebAuthn. The broker writes the returned request privately. Only the
waiting caller invokes bootstrap or sends it to Mother. Mother still checks every
signature, snapshot, nonce, expiry and required durable observation.

```text
serve-passkey-tap.py --session PRIVATE_SESSION --cert CERT --key KEY
passkey-approve.py --session PRIVATE_SESSION open
passkey-approve.py --session PRIVATE_SESSION register --output PIN
passkey-approve.py --session PRIVATE_SESSION --binary MCT bootstrap ...
passkey-approve.py --session PRIVATE_SESSION --binary MCT issue ...
passkey-approve.py --session PRIVATE_SESSION --binary MCT revoke ...
```

## Types and ownership sketch

```text
JobId = random 256-bit identifier
Operation = Registration(name) | RootAssertion
Phase = Queued | Ready | Prepared | Offered | Signed | Taken | Failed
PageSession = {HttpOnly cookie, in-memory page_id}
Offer = {random id, page_id, monotonic deadline}
Job = {id, operation, phase, deadline, prepared?, offer?, result_path?}
Broker.enqueue(operation) -> JobId             [private Unix socket only]
Broker.claim() -> Job                         [authenticated HTTPS page]
Broker.publish(id, prepared)                  [private Unix socket only]
Broker.renew(id, offer)                       [current owning page only]
Broker.release(id, offer)                     [owning offer capability only]
Broker.complete(id, offer, result) -> result_path [current owning page only]
Broker.take(id) -> result_path                [private Unix socket only, once]
```

The broker has no executable paths, Mother handles, subprocess API or admin
submission endpoint. The CLI owns action construction, pin installation, intent
preparation and submission. The browser owns the passkey ceremony, not admission.
Registration results are installed separately from genesis requests; bootstrap
cannot supply or replace its own trusted pin.

## Timing and delivery

Preparation starts after the visible, focused page claims queued work. The page
renders the pinned root identity and exact prepared intent, yields a paint, and requires an explicit
**Approve reviewed action** click before it calls WebAuthn. Login, page focus and
an offered job are not signing consent. Mother
still owns the five-minute intent timestamp; the browser cannot extend it.
This removes time spent waiting for the page, not time spent away from an already
visible approval. No signed request is automatically replayed or re-prepared.
A root intent retains its original expiry through every offer. An expired intent
requires a fresh CLI command and remains subject to Mother's refusal.

Each offer has one page owner and a 30-second lease, renewed every five seconds.
The page aborts WebAuthn after 20 seconds without a successful renewal. That gap
leaves room for the five-second HTTP timeout and one missed heartbeat. Pagehide,
hiding the tab, an error or a timeout aborts the pending ceremony. The page
acknowledges release only after the WebAuthn promise settles. It then stops rather
than automatically starting another prompt. The waiting CLI remains alive.

A fresh `open` and page authentication replaces the session and page identity.
This immediately fences old renewals and completions, even when tabs share an
updated cookie. The replacement page shows a bounded handoff wait. After the old
page acknowledges cancellation or its lease expires, the same job is reclaimable
with a new offer capability. Old capabilities cannot complete or release the new
offer. Registration returns to Queued; a root assertion returns to Prepared
without changing the signed intent. Signed and Taken jobs cannot be reclaimed.

The broker guarantees one valid offer, not remote control over another process's
UI. A frozen or non-cooperative browser/provider may retain an obsolete visual
prompt after its lease expires. Its result is fenced. Requiring proof that such
an external prompt disappeared would prevent recovery from a silent page. The
CDP handoff test verifies that a cooperative page's WebAuthn promise settles
before the replacement starts its call.

A lost submission reply, process crash or service restart never implies success.
Private signed artifacts remain available for canonical-state inspection.
An output-write failure retains its evidence and fails the job rather than
reoffering it. This protocol requires a broker started with the matching revision;
it is not an in-place upgrade of an active session.

## Boundaries and bounds

- Bind HTTPS only to 127.0.0.1. Serve fixed assets with exact Host, restrictive CSP,
  no framing, no caching and no referrer. Mutating APIs require exact Origin,
  application/json, and an authenticated page cookie plus page identity. No CORS
  access. Release alone also accepts a superseded page's matching offer capability
  as a cancellation acknowledgement; it grants no renewal or completion rights.
- A local CLI receives a one-use, short-lived page capability over an owner-private
  Unix socket. It opens the capability at the distinct `/approve/` path in the URL
  fragment, never a query string. The distinct path forces a fresh page load after
  a hosted login landing page. The page removes it from history and exchanges it
  for a Secure, HttpOnly, SameSite=Strict cookie and a separate page identity
  carried in X-MCT-Page.
  One current page identity at a time; restart invalidates it.
- The page cannot enqueue, replace prepared intent fields or choose output paths.
  Completion must match the queued intent and registered credential. Mother,
  not the broker's structural checks, establishes cryptographic validity.
- At most eight active jobs, 96 KiB JSON messages, sixteen concurrent HTTPS
  connections, five-second HTTPS connection deadlines, and a one-hour queue lifetime.
  These are local harness resource limits, not authority policy.
- Session directories are owner-private; socket and artifacts are private.
  Existing files are not overwritten or chmod-repaired. No process-wide umask
  changes. Same-UID hostile processes and a compromised browser/host are outside
  this harness isolation boundary.

## Optional Pocket ID page gate

The hosted integration adds a separate Node OIDC gate in front of this private
loopback queue. It validates Authorization Code + PKCE, state, nonce, issuer,
audience/authorized-party, JWKS signature, time and an exact `sub` allowlist. It
also requires a short-lived server-side session and CSRF token before proxying a
bounded queue POST. The OIDC gate strips its own cookies and forwards only the
queue cookie/page ID.

These are independent capabilities: Pocket ID login permits page access; the
one-use fragment creates a queue page session; the offer identifies one prepared
intent; WebAuthn signs it; Mother decides authority. Hosted operation authenticates
first, then obtains and opens `/approve/#...`; cross-origin OIDC redirects are never
asked to carry the fragment. The queue derives the exact accepted RP ID from the
configured public origin hostname. The OIDC gate has no Mother submission route
and no MCT root private key. See
[`docs/pocketid-approval.md`](../pocketid-approval.md).

## Synthesis decision

An in-daemon Rust HTTPS queue would couple browser sessions and certificates to
the authority owner. A separate Python broker reuses the current harness and
leaves canonical verification and submission unchanged. Choose the broker.
The private Unix socket also separates enqueue authority from the browser cookie;
there is no browser-accessible generic command or queue-creation endpoint.

## Verification and limits

Unit tests cover queue transitions, silent-page takeover, heartbeat expiry,
shared-cookie fencing, stale completions, substitution, replay, authentication,
bounds and private files. A CDP test uses a WebAuthn virtual authenticator through
registration, genesis, issuance, signed status, revocation and fresh rejection
before/after restart. It checks that there are no file inputs or Sign clicks.
A separate hosted-topology CDP regression starts the real non-localhost HTTPS
queue and gate, follows a mocked OIDC redirect, obtains the capability through the
headless CLI, registers the exact origin-derived RP, and completes the same Mother
ceremony. This establishes test-driver behavior with the virtual authenticator and
a mock issuer, not actual Pocket ID or 1Password behavior. Doug's live rerun is
separate. Local CA installation remains an explicit one-time operator action; no
certificate bypass in live use.
