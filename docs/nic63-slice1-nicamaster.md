# NIC-63 slice 1: existing proofs on nicamaster

Source revision `f79eca1aeeff491b14febaa3d275b76dead34855`, from `origin/main`.
Run date 2026-11-03 ET, 2026-11-04 UTC. No runtime source, proof selection,
assertions, timeout, ignored-test policy or completion status was changed.

## Owner decisions

Doug's words from the owner-relayed chat at 7:18 PM ET, with no calendar date
specified in the request:

- "Yes, Pis are disposable: Mother kills, network cuts, reboots are fine"
- "Linux arm64 is a supported target"

The [D-52 amendment](DECISIONS.md#nic-63-amendment-supported-target-and-disposable-pi-testing)
records these decisions. Linux `aarch64-unknown-linux-gnu` is supported alongside
macOS arm64. Support is not a claim that the release inventory is complete.
No `rpi-update` or EEPROM changes are authorized. A hung Pi is recovered by
power cycling only. This run did not require recovery or host disruption.

## Hosts and artifact delivery

| Host | Environment |
| --- | --- |
| Mac baseline | macOS 27.0.1, arm64, Rust/Cargo 1.96.0 |
| Linux builder on the Mac | Native ARM64 Docker VM, Debian 13.6, Rust/Cargo 1.96.0, target `aarch64-unknown-linux-gnu` |
| nicamaster | Raspberry Pi 4 Model B Rev 1.2, Debian 13.7 trixie, aarch64, kernel `6.18.50+rpt-rpi-v8`, glibc 2.41 |

Nicamaster reported 3789 MiB RAM and 4095 MiB swap before testing. The Mac's
existing ARM64 Docker image and Cargo download caches avoided a Pi build.
The container used two Cargo jobs, a private target volume, a 6 GiB memory
limit and a 7 GiB combined memory/swap limit. Both builds disabled debug info
and incremental compilation. No Rust compiler was installed on the Pi.

The builder image was
`rust@sha256:1bcff4befb740599103a2c7cb51058e14479b2e35e3a34a3f0dc4ede09927488`.
Rust 1.96.0 was installed inside that container to match the Mac compiler.
The [build commands](evidence/nic63-slice1/build-linux.sh) used `--locked` and
compiled six test executables plus the real `mct-daemon` executable.

The source archive and executables were copied over SSH to the private
`/home/pi/mct-nic63-slice1` tree. All seven executable hashes matched the
[hash manifest](evidence/nic63-slice1/binaries.sha256) on the Pi.
The daemon is an AArch64 ELF using `/lib/ld-linux-aarch64.so.1`; its required
system libraries resolved on nicamaster. Source and target paths match the
container paths because fixtures embed `CARGO_MANIFEST_DIR` and
`CARGO_BIN_EXE_mct-daemon`. This bundle is test evidence, not a release package.

## Commands and checker behavior

Mac baseline, from the source revision above:

```sh
umask 077
export TMPDIR=/private/tmp CARGO_TARGET_DIR=/private/tmp/nic63-macos-target
export CARGO_BUILD_JOBS=4 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
python3 scripts/check-finish.py proofs
```

On nicamaster, from `/home/pi/mct-nic63-slice1/source`:

```sh
umask 077
export TMPDIR=/tmp PATH=/home/pi/mct-nic63-slice1/tools:$PATH
python3 scripts/check-finish.py proofs
```

`tools/cargo` is the attached [prebuilt dispatcher](evidence/nic63-slice1/prebuilt-cargo.sh),
not a Cargo compiler. Its [artifact map](evidence/nic63-slice1/artifacts.tsv)
selects the six shipped executables. It accepts only the checker's target and
libtest argument shapes and executes the real test binary without rewriting
output or exit status. Unsupported commands, including `--ignored`, are refused.
The unchanged checker lists each filter, enforces its minimum, runs it serially,
and requires every selected test to pass with zero ignored or measured tests.
It retains its 900-second per-command timeout and process-group cleanup.

## Results

Both `check-finish.py proofs` runs exited **0** on the first attempt. Each
passed **17 groups and 147 tests**, with zero failures, ignored or measured
tests. Selected test names and counts matched for every group.

| Proof group | Mac passed | Nicamaster passed |
| --- | ---: | ---: |
| genesis-writer | 17 | 17 |
| genesis-runtime-deny | 1 | 1 |
| recovery-writer | 18 | 18 |
| recovery-policy | 18 | 18 |
| issuance-kernel | 15 | 15 |
| issuance-writer | 20 | 20 |
| issuance-runtime-deny | 1 | 1 |
| recovery-runtime-deny | 1 | 1 |
| recovery-kernel | 25 | 25 |
| revisions | 3 | 3 |
| local-stale | 1 | 1 |
| fleet | 1 | 1 |
| two-mother | 11 | 11 |
| refresh | 4 | 4 |
| dispatch-parser | 5 | 5 |
| dispatch-writer | 4 | 4 |
| sqlite-rollback | 2 | 2 |
| **Total** | **147** | **147** |

The [Mac receipt](evidence/nic63-slice1/mac-proofs.txt) and
[Pi receipt](evidence/nic63-slice1/pi-proofs.txt) retain the selected names and
actual result summaries. No selected test required a Linux-specific fix,
retry, skip or timeout increase. The daemon library's unselected tests differ
by platform, so its filtered-out count differs. That is not a missing selected
proof. Timings also differ; this was a correctness run, not a benchmark.

Additional checks passed: 17 proof-runner self-tests on each host, Tier-0
`specs`, mdBook including link checks, ShellCheck for the attached scripts,
unsupported-dispatch refusals and `git diff --check`. No slice test processes
remained on nicamaster after completion. The full workspace suite, Clippy and release
acceptance were not rerun for this documentation/evidence-only change.

## Scope and retained evidence

The fleet test runs six real Mother processes on one host. It exercises 30
remote routes and six local routes, revocation, process kills, survivors and
restart. Running those processes on one Pi is not a six-host partition test.
The fixture uses the legacy owner-only UDS and offline revision setup, not the
full passkey application runtime. Linux Git confinement remains unavailable.
No network cut, host reboot, EEPROM operation, firmware update, production
Mother mutation or installed service change was needed. Only fixture Mothers
were killed. Multi-host partitions and rolling upgrades remain open work.

Raw logs and build outputs remain in `/private/tmp/mct-nic63-evidence` on the Mac.
The Pi retains its source, test bundle and logs in `/home/pi/mct-nic63-slice1`.
Do not publish whole Mother roots or test credential directories. The committed
receipts contain only test names, result summaries, commands and artifact hashes.
