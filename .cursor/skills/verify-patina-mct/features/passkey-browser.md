# Passkey browser ceremony

The passkey ceremony lets an operator register a root and sign genesis, issuance, and revocation in the default browser with 1Password. The Mother process does not perform that tap.

## Sub-features

- `passkey-register` registers a root in the default browser. The operator selects 1Password.
- `passkey-bootstrap` signs the prepared genesis and submits it.
- `passkey-issue` signs issuance of a day key.
- `passkey-revoke` signs revocation.
- `passkey-queue` runs the approvals queue in [docs/passkey-approval-RUN.md](../../../../docs/passkey-approval-RUN.md).

## How to get to it (user POV)

- Follow [docs/passkey-admin.md](../../../../docs/passkey-admin.md) on an operator-controlled HTTPS origin. Import `scripts/passkey-admin.mjs`, call `registerRoot`, and choose 1Password in the default browser.
- Review the prepared intent, call `signPrepared` on that same origin, and submit `bootstrap` with the signed request.
- For the queue, follow [docs/passkey-approval-RUN.md](../../../../docs/passkey-approval-RUN.md). The opener uses the system default browser. Approve registration, genesis, issuance, and revocation in 1Password.
- Day-key status after issuance uses `sign-day` and does not ask for another passkey tap.

## Driving it with verify-patina-mct

Preconditions:

- An operator is present.
- The default browser can reach the operator's 1Password.
- The run directory is one the operator created for this ceremony, outside any directory this skill uses for unattended drives.
- This VM does not satisfy the preconditions. Mark the feature manual and stop.

- **Do not fake the tap.** Do not call a virtual authenticator, do not inject a provider, and do not treat `drive-admin` as this feature. `drive-admin` stops at an unsigned challenge.
- **Operator result.** When the operator finishes, the expected lines are PASS for registration, bootstrap, issue, day-status completed, revoke, revoked-status rejected, and restarted-status rejected. The private directory from that run stays with the operator.
- **Unattended result.** Record `passkey-browser` as manual, with the unmet precondition `operator default browser with 1Password`. Exit this recipe. A missing PASS line is not a product failure until that run exists.

## Gotchas

- Choosing a password store other than 1Password does not satisfy the live rerun, even if a credential is created.
- An intent lasts at most five minutes. If the operator delays, prepare again. Do not reuse an expired signed request.
- A lost or uncertain submission is not permission to retry. Inspect canonical state first.
- The virtual authenticator test in the repository does not establish the default browser requirement and does not replace the 1Password taps.
- The page can offer one Approve retry after a refusal. That retry still needs the operator. Do not click it from an unattended session.
