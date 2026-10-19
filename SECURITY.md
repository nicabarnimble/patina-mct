# Security policy

## Report a vulnerability privately

Use this repository's **Report a vulnerability** button under the GitHub
**Security** tab. It opens a private security advisory visible to the reporter
and repository maintainers:

<https://github.com/nicabarnimble/patina-mct/security/advisories/new>

Do not open a public issue for an undisclosed vulnerability. No security email
address is published for this repository.

GitHub private vulnerability reporting was verified enabled on 2026-07-23 with:

```bash
gh api repos/nicabarnimble/patina-mct/private-vulnerability-reporting
# {"enabled":true}
```

The endpoint above is the live-state verification surface for the reporting
channel; this file does not claim an email or alternate intake path.

## Supported release boundary

MCT 0.2.0 is a pre-GA release whose supported artifact target is
`aarch64-apple-darwin`. Linux/systemd/signing, Apple notarization activation,
network release acquisition, and into-the-wild GA remain outside this release
([release checklist](layer/surface/build/product/RELEASE-CHECKLIST-v0.md),
[R3 close-out](layer/surface/build/feat/release-discipline/CLOSEOUT.md)).

## Landed R2 security closure

The R1 review recorded four high and two medium release findings in
[`RELEASE-REVIEW-R1.md`](layer/surface/build/product/RELEASE-REVIEW-R1.md).
The following landed changes close those specific findings; this is not a
claim of exhaustive vulnerability absence.

| Reviewed finding | Landed closure evidence |
|---|---|
| Administrative UDS authentication and bounded connection/header/body reads | Commit `6e22857`; shared peer-owner capability and bounded reads in [`control.rs`](crates/mct-daemon/src/control.rs) and [`daemon/control.rs`](crates/mct-daemon/src/daemon/control.rs). |
| Parent-directory durability after supervisor publication | Commit `6e22857`; staged-file sync, rename, and parent-directory `sync_all` in [`supervisor_lifecycle.rs`](crates/mct-daemon/src/daemon/supervisor_lifecycle.rs). |
| Standing-source authority correlated to canonical ledger evidence | Commits `0b4b475` and `64dd4ee`; verification and projection hardening in [`acquisition.rs`](crates/mct-daemon/src/acquisition.rs), [`state.rs`](crates/mct-daemon/src/state.rs), and [`artifact.rs`](crates/mct-kernel/src/artifact.rs). |
| Excess `fire_late_bounded` recovery range receives terminal accounting | Commit `7d0869a`; production-bound partition and terminal range evidence in [`trigger_scheduler.rs`](crates/mct-daemon/src/daemon/resident/trigger_scheduler.rs). |
| Vulnerable dependency closure and explicit audit policy | Commit `979921f`; patched dependency graph in [`Cargo.lock`](Cargo.lock), policy in [`.cargo/audit.toml`](.cargo/audit.toml), and audit invocation in [`scripts/ci-tier0.sh`](scripts/ci-tier0.sh). |
| Raw Git stderr excluded from caller-safe and durable output | Commit `979921f`; closed failure projection and marker regression in [`toy.rs`](crates/mct-daemon/src/toy.rs). |

## Dependency-audit policy

GitHub CI installs exactly `cargo-audit 0.22.2`. Fmt, Clippy, tests, the
vocabulary/release/documentation checks, and `cargo audit` are separate jobs in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml), so one failure does not
hide the others. [`scripts/ci-tier0.sh`](scripts/ci-tier0.sh) still runs the
same checks; `audit` is one of its subcommands.

A pull request fails the audit job only when it changes `Cargo.lock` or a
`Cargo.toml`. Any other pull request still runs `cargo audit` and publishes a
warning annotation. A nightly schedule on `main` runs the same
[`.cargo/audit.toml`](.cargo/audit.toml) policy strictly and opens or updates
one issue titled "dependency advisories" when it fails. That nightly run also
flags ignore entries whose `revisit-by` date has passed.

The repository policy is [`.cargo/audit.toml`](.cargo/audit.toml):

- warnings are denied;
- unmaintained and unsound informational advisories are evaluated;
- `RUSTSEC-2024-0436` is explicitly ignored with
  transitive-dependency rationale, a revisit date, and a revisit condition in
  the policy file; and
- yanked-package checking is disabled with the pinned `spin 0.10.0` transitive
  rationale recorded in the policy file.

Changes to those exceptions require an explicit policy edit; CI does not apply
an unrecorded fallback or blanket advisory waiver. The nightly audit and any
pull request that touches dependencies keep this policy.
