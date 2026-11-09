# Owner-operated QR approval rerun

Use the default browser with 1Password enabled and unlocked. The production MCT
passkey RP is `mct-approval.nicabar.org`. Never bypass a certificate warning.

## One-time root registration

Register the dedicated MCT credential independently at the approval origin and
store its public pin as an owner-private `registered-root.json`. Registration is
not an approval request and is deliberately not one of the queue’s two request
kinds. Verify the pin contains the exact credential ID, ES256 SEC1 public key,
RP ID and origin before using it.

## Start the private queue and public gate

Follow [`docs/pocketid-approval.md`](pocketid-approval.md). The queue binds only to
`127.0.0.1`; Pocket ID, DNS, reverse proxy, certificate and Unraid changes are
owner operations.

## Run the ceremony

```sh
umask 077
python3 scripts/passkey-approval-demo.py \
  --session "$SESSION" \
  --directory "$PRIVATE_RUN" \
  --binary "$MCT" \
  --registered-root "$REGISTERED_ROOT"
```

Each root action prints a fresh QR. Scan it, confirm the short code, sign in with
Pocket ID, review the phone screen, then tap **Approve with MCT passkey**. Select
the dedicated MCT credential in 1Password. Login is not approval.

Expect bootstrap, issue and revoke to end with the packet’s terminal lines. The
day-key status checks still need no root passkey. Preserve the private run
directory; it contains credentials, signed requests and replies and must not be
uploaded.

For NIC-132, run the separate `proof` command documented in
[`docs/pocketid-approval.md`](pocketid-approval.md). A denied or not-found answer
uses up the code. Fix the wiki release, then request a new QR.

A cancelled 1Password sheet may be tried again from the same unconsumed review
page. An uncertain network or Mother result must be inspected; never sign or
submit blindly a second time. Report real Pocket ID, Cloudflare Access and
1Password UX separately from the Chrome virtual-authenticator evidence.
