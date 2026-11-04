# Operations

Operating MCT means controlling a sovereign local Mother and retaining enough
evidence to explain every authority decision and effect.

## Operational principles

- Prefer machine-readable status for automation.
- Verify release artifacts before installation.
- Treat artifact acquisition, approval, assignment, and execution as separate
  facts.
- Treat denials as expected fail-closed outcomes, not permission to bypass an
  authority boundary.
- Preserve the observation ledger when diagnosing failures.
- Treat legacy process/JVM runtime rows as inert history, not recoverable local
  execution configuration.
- Perform rollback explicitly; upgrade never silently rolls back.

## Supported platforms

MCT supports macOS `aarch64-apple-darwin` and Linux `aarch64-unknown-linux-gnu`.
The macOS lifecycle uses user launchd. Linux has a bounded Nix/systemd
admin-only lifecycle. The repository contains its procedure in
`docs/linux-admin-install.md` and hardware evidence in
`docs/linux-admin-install-evidence.md`. NIC-63 slice 1 compares same-host proof
tests in `docs/nic63-slice1-nicamaster.md`. It does not establish multi-host
partitions, rolling upgrades or the full passkey Child runtime.

Detailed procedures will be added only with executable validation or linked
release evidence. Focused tasks belong in [How-to guides](../how-to/index.md).
