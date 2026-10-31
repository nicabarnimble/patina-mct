# CI dependency findings

PR #86's fresh advisory databases found issues absent from the earlier cached
local audit. Both root and fuzz lockfiles are checked.

- Wasmtime 48's optional compiled-module disk cache was never configured by MCT.
  Its feature is now disabled; all other former default features are retained.
  This removes the affected zstd family rather than ignoring its advisories.
- The affected inline-buffer implementation is upgraded from block-buffer
  0.12.0 to 0.12.1.
- GHSA-qwgh-2vcv-g2f7 currently declares every version below 0.12.1 affected.
  The upstream [changelog](https://github.com/RustCrypto/utils/blob/block-buffer-v0.12.1/block-buffer/CHANGELOG.md)
  places inline cursor storage and ReadBuffer's introduction at 0.11.0.
  Version 0.10.4 instead has a separate cursor field; its generation callback
  writes a temporary block before publication. Wasmtime still pulls in 0.10.4.

`scripts/scan-osv.py` preserves the complete OSV report and classifies only that
exact advisory/package/ecosystem/version combination as not affected. It requires
both expected lockfile reports, fails on mixed findings or scanner errors, and
expires on **2027-01-29**. No ID-wide or package-wide ignore was added.
Rust panic regressions exercise 0.10.4's cursor invariants; Python regressions
cover the classifier's refusal cases. These are not a general memory-safety proof.

Remove the classification and its exact-version test dependency when upstream
corrects the advisory range or Wasmtime no longer needs the older buffer.

## PR #87 runtime patch

The failed PR #87 scan reported Rust lockfile dependencies, not vendored skill
files. Merging main at `cfd0d1f` brought in PR #86's zstd removal and block-buffer
update. A fresh scan still failed on these Wasmtime advisories:

- [RUSTSEC-2026-0420](https://rustsec.org/advisories/RUSTSEC-2026-0420.html):
  WASIp3 UDP send could use an address permitted only for receiving.
- [RUSTSEC-2026-0422](https://rustsec.org/advisories/RUSTSEC-2026-0422.html):
  insufficient typed-function argument checks could panic.
- [RUSTSEC-2026-0423](https://rustsec.org/advisories/RUSTSEC-2026-0423.html):
  cross-engine confusion of host-created struct types with supertypes.
- [RUSTSEC-2026-0424](https://rustsec.org/advisories/RUSTSEC-2026-0424.html):
  unchecked memory usage when transferring borrowed component resources.

All four are fixed in the 48.x maintenance release **48.0.8**. The workspace
minimums for `wasmtime` and `wasmtime-wasi` and both root/fuzz lockfiles now select
that patch and its coordinated Wasmtime/Cranelift dependencies. The feature set
is unchanged, including the disabled disk cache. No new suppression or scan
exclusion was added. These are upstream runtime advisories; this update does not
claim every affected API is reachable through MCT. The same OSV v2.7.0 scan
changed from exit 1 before the patch to exit 0 afterward.
