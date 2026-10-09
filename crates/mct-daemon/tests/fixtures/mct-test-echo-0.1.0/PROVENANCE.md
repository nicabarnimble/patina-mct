# `mct-test-echo` fixture provenance

This fixture is repository-authored test code. It exports the WIT-shaped operation
`patina:mct-test/echo@0.1.0.echo`, accepts one signed 32-bit integer, and returns
that same integer. The output therefore depends on the input without using host
imports or ambient authority.

`mct-test-echo.wat` is authoritative source. `mct-test-echo.wasm` is the
component-model binary generated from it. The test
`committed_echo_fixture_is_reproducible_and_invocable` recompiles the WAT with
the lockfile-pinned `wat` crate, requires byte identity with the committed
component, verifies both SHA-256 sidecars through the strict Child loader, and
invokes the component through `MctWasmComponentRuntime`.

To regenerate manually from the repository root with a compatible
`wasm-tools`:

```bash
wasm-tools parse \
  crates/mct-daemon/tests/fixtures/mct-test-echo-0.1.0/mct-test-echo.wat \
  -o crates/mct-daemon/tests/fixtures/mct-test-echo-0.1.0/mct-test-echo.wasm

for file in \
  crates/mct-daemon/tests/fixtures/mct-test-echo-0.1.0/child.toml \
  crates/mct-daemon/tests/fixtures/mct-test-echo-0.1.0/mct-test-echo.wasm
do
  shasum -a 256 "$file" | awk '{print $1}' >"$file.sha256"
done
```
