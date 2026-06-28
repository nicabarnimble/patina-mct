# Add typed kernel errors

## Story
Add typed kernel errors and validation contracts at the `mct-kernel` boundary without leaking adapter/runtime/storage types.

## Why
The kernel already returns typed authority decisions for denials. What is still missing is a narrow typed error surface for malformed domain values: JSON edge decode failures, blank WIT operation fields, and inconsistent payload handles that should not enter authority evaluation as if they were valid calls.

## Direction
- Keep denials as decision records.
- Treat malformed domain construction as typed kernel errors.
- Prefer simple validation helpers over local ad hoc checks.
- Avoid speculative traits, context bags, and adapter-specific error variants.

## Context
- `layer/core/dependable-rust.md`
- `layer/core/safety-boundaries.md`
- `layer/core/adapter-pattern.md`
- `layer/slate/work/mct-kernel-crate/work.toml`

## Notes
This is a boundary-hardening slice, not a private-field migration for every kernel record.
