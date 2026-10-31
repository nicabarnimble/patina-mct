# Doug's approvals-only 1Password rerun

Use your default browser with 1Password enabled and unlocked. Select
**1Password**, not iCloud or the browser's password store, for the new registration.
Save a new test credential.
This is a fresh run; never reuse or modify `~/.mct-pr88-tap`.

## Once per machine

Run `mkcert -install` yourself and approve the OS trust prompt. Do not bypass a
certificate warning. Test CDP certificates are not used for this live run.

## Terminal 1: fresh setup and broker

From the PR checkout, after building `mct-daemon`:

```sh
T=$(mktemp -d "$HOME/.mct-approval-run.XXXXXX")
mkdir -m 700 "$T/session"
install -m 700 target/debug/mct-daemon "$T/mct-daemon"
mkcert -cert-file "$T/cert.pem" -key-file "$T/key.pem" mct.localhost
printf 'Use this same T in terminal 2: %s\n' "$T"
python3 scripts/serve-passkey-tap.py --session "$T/session" \
  --cert "$T/cert.pem" --key "$T/key.pem"
```

If port 8443 is occupied, stop the older harness yourself. Do not delete its
private evidence. A broker restart needs a new session directory.

## Terminal 2: open your default browser with 1Password and run the ceremony

Set `T` to the fresh directory printed above. The opener uses your system default
browser. Run from the same PR checkout:

```sh
python3 scripts/passkey-approve.py --session "$T/session" open
python3 scripts/passkey-approval-demo.py --session "$T/session" \
  --directory "$T/run" --binary "$T/mct-daemon"
```

Keep the authenticated page focused. Approve registration, genesis, issuance,
and revocation in 1Password. There are no files to choose, no downloads and no
Sign button. The CDP virtual-authenticator test needed no Approve clicks.
If the real browser/provider refuses the automatic request, the page offers
**Approve** once for that action. This fallback does not bypass provider consent.

Expect PASS for registration, bootstrap, issue, day-status completed, revoke,
revoked-status rejected, restarted-status rejected, and the full ceremony.
The day-key status calls need no passkey approval. The demo stops Mother cleanly.
Then Ctrl-C the broker in terminal 1. Preserve this private directory; do not
upload it. It contains the day-key secret, TLS key, signed requests and replies.

For a broker started with this revision, an abandoned page no longer requires
killing the run. Run only the `open` command again. Keep the demo and broker
running. The new page takes over the unsigned job after the previous page cancels
or its 30-second lease expires. Late completions from the old page are refused.
Do not restart an active older broker to apply source changes during a live run.

Preparation waits for the page, but an already displayed root intent still has
its original five-minute limit. Handoff does not extend it. On intent expiry or
uncertain submission, stop and inspect. Do not rerun the demo against its existing
run directory or automatically retry an uncertain action.
Report any provider-selection, unlock, or extra confirmation UI separately from
the three root approvals. Actual 1Password behavior is the point of this rerun.
