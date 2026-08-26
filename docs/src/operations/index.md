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
- Perform rollback explicitly; upgrade never silently rolls back.

## Current platform

The proven 0.2.0 lifecycle is a macOS user-launchd service on
`aarch64-apple-darwin`. Other platforms are not implied by portable Rust source
alone.

Detailed procedures will be added only with executable validation or linked
release evidence. Focused tasks belong in [How-to guides](../how-to/index.md).
