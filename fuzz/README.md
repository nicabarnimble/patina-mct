# MCT hostile-input fuzzing

This is a standalone `cargo-fuzz` workspace so sanitizer builds and bounded
fuzz smoke runs stay outside the root workspace and Tier 0.

List the landed targets with:

```bash
cargo +nightly fuzz list --fuzz-dir fuzz
```

Committed corpus seeds are copied from or generated from the existing unit and
integration fixtures cited by each target. Crash artifacts and coverage output
remain local under `fuzz/artifacts/` and `fuzz/coverage/`.
