# NIC-53 installation evidence

## Verdict

**PASS: the complete native hardware gate exits 0, including cleanup.** The
owner approved a system-level systemd unit running as `User=pi`/`Group=pi`.
The system manager supplies the filesystem sandbox without `PrivateUsers`;
Mother's existing ownership guard remains unchanged. No authority or Linux Git
confinement check was relaxed.

Accepted implementation: `06495a9bcd67555ea226787cc7eb40b483de569f`.
Acceptance ran on 2026-11-01 UTC (2026-10-31 host-local time). Earlier user-unit
failures below refer to `7479d9c8ad8760ed11d7b61795e29de43bcfc7b1`.
Nicamaster is a Raspberry Pi 4 Model B Rev 1.2 with four cores and 3794 MiB
reported RAM. The host runs Debian 12,
`aarch64`, kernel `6.1.21-v8+`, local ext4 storage and 4095 MiB swap. No claim
about SSD identity is inferred from filesystem size.

## Accepted system-unit gate

From the committed checkout, as `pi`:

```sh
nix run .#mother-install-smoke
```

Output, omitting Nix build chatter, the host locale warning and the private
artifact path:

```text
Software test authority only; not a 1Password ceremony or production identity.
PASS install/start the system service as pi:pi
PASS signed status
PASS second writer rejected
PASS revoke
PASS forced restart
PASS revoked key rejected
PASS WASI probe runs
METRIC cold readiness 398.513 ms (n=1, process-cold through signed status; OS cache not purged)
METRIC idle RSS 16932..16932 KiB (n=5); Mother peak RSS 20176 KiB
METRIC component compile 27.141 ms; compile peak RSS 10744 KiB (fresh probe process, n=1)
METRIC durable-write p50=3.647 ms p95=4.637 ms max=7.065 ms (n=40; append+sync_data, verified reopen)
PASS isolated service cleanup; private evidence preserved
exit=0
```

Before the run, load averages were 0.18/0.07/0.05 on four cores. Readiness includes
signing/client overhead and completes only after authenticated status succeeds.
Idle samples are 200 ms apart; peak RSS covers both resident tenures. The WASI
probe verifies the exported result after an actual host import. Every measured
append is awaited and all forty entries are validated after reopening. These are
single-run resource observations, not a speedup comparison or power-loss proof.

An earlier complete passing run at `bfa0e29` measured readiness 446.378 ms,
idle RSS 17616 KiB, peak RSS 19136 KiB, compile 27.059 ms/10192 KiB, and durable
p50/p95/max 12.841/13.877/31.668 ms. Another passing run at `3bfab55` measured
406.570 ms readiness, idle/peak RSS 18024/18348 KiB, compile 21.893 ms/10644 KiB,
and durable p50/p95/max 3.553/4.411/7.130 ms. This variation is not hidden by
averaging; no performance regression or improvement is inferred from these runs.

The gate checks the running process, not only unit text: pi UID/GID, original
user namespace, host-root UID 0, distinct mount namespace/private tmp, read-only
system root/home/store, writable canonical root, NoNewPrivs and zero effective,
ambient and bounding capabilities. Iroh owns a NETLINK_ROUTE socket and its
ticket contains only loopback endpoints, without a relay.

### Lifecycle regression and blast radius

The first system-unit run at `efdbac7` passed all authority/probe checks but
**exited 1 during cleanup**. Nix's newer systemctl attempted a D-Bus disable method
absent from Debian 12's manager:

```text
Unknown method DisableUnitFilesWithFlagsAndInstallInfo
```

Lifecycle operations now use the root-owned host `/usr/bin/systemctl`, with
root-owned, non-writable ancestry. A failing client-selection regression preceded
the fix. The stopped unit's exact generated contents and PID 0 were independently
verified before removal with the corrected helper. Its evidence remains private.

A later preflight-scope edit at `cc43462` exited 1 before fixture creation because
it referenced the unit name before allocation. The name is now allocated before
the preflight query; the complete gate was rerun successfully at `06495a9`.
Preflight queries only its own unit, not the system manager's environment.

The safety fact is that elevated publication does not elevate Mother or redefine
canonical ownership. The real service passes signed administration and restart
while the unchanged Rust ownership check sees UID 0 at `/` and Mother runs as pi.
Revoked keys and a second writer remain refused. No Rust file changed during
this system-service correction; historical formats, Darwin lifecycle/release,
queue/browser behavior and Linux Git refusal therefore remain unchanged.

Eleven installer tests cover escaping, exclusive publication, private ancestry,
package verification failure, root-owned metadata, refusal to publish without
root, changed-unit cleanup refusal, host-client selection and rejection of
existing transient/vendor unit names. They pass normally and under `umask 000`.
The manager-name check prevents a new `/etc` unit from shadowing an existing
unit from another search path. Live confinement checks also cover the system
root mount and capability bounding set.

After the service correction, `cargo fmt --check`, `bash scripts/ci-tier0.sh clippy`,
`bash scripts/ci-tier0.sh specs`, `bash scripts/build-docs.sh` and
`nix flake check --all-systems --no-build` passed. The complete Rust suite and
browser E2Es below were run earlier in this PR, not rerun for these Python/unit
changes; no Rust or browser source changed during the correction. Final
hosted checks must still be evaluated on the pushed head before any merge.

Independent post-run listings found zero loaded/installed smoke units and zero
smoke package roots. Linger remains disabled. Test services are not enabled at
boot; no production service, identity, firewall, kernel or security module was
changed. Package verification remains fail-closed, without repair. Private
canonical state, test credentials, receipts and logs are retained, not uploaded.

Remaining limits: admin-only scope, host/root and hostile same-UID processes
outside the isolation boundary, no host-reboot/power-loss test, no large-history
or concurrent-Child workload, and no physical Pi 3 acceptance. A changed or
partially published unit is preserved for inspection rather than auto-repaired.
No independent subagent review was available; this was a direct blast-radius
review with regression tests and hardware execution.

## Native build

A native build using the committed Nix inputs and Rust 1.96.0 succeeded:

```text
Finished `release` profile [optimized] target(s) in 71m 19s
buildPhase completed in 1 hours 12 minutes 33 seconds
```

The package contains both `mct-daemon` and `mct-install-probe`:

```text
/nix/store/43c376cj22mfa100nwfivkcki38iirkh-mct-admin-mother-0.2.0-nic53
```

Its derivation is
`83mbwa651b4vayhzhkpmkqmj17q0l04a-mct-admin-mother-0.2.0-nic53.drv`.
The build used two Cargo jobs and retained the existing release optimization
profile. `nix-store --verify-path` passed before tool execution. Initial source
transfer and the later committed Git checkout resolved to the same Rust package
derivation. Final gate attempts used the committed checkout.

## Historical user-unit gate failure

Command:

```sh
nix run .#mother-install-smoke
```

Result at the earlier user-unit revision:

```text
Software test authority only; not a 1Password ceremony or production identity.
RuntimeError: user service failed; inspect its journal
exit=1
```

The corresponding journal cause, with the private path omitted, is:

```text
Error: observation ledger io error at <isolated root>/observations.jsonl: untrusted filesystem root
Caused by:
    untrusted filesystem root
```

`crates/mct-observation/src/private_fs.rs:43-45` requires the filesystem root
to belong to UID 0 and not be group/world writable. A separate transient user
unit reproduced the namespace mismatch using the same namespace and capability
settings:

```text
sandbox_root_uid 65534 euid 1000
uid_map 1000       1000          1
host_root_uid 0 euid 1000
```

Unprivileged `PrivateUsers=yes` maps only the calling user here. Host root is
unmapped inside the namespace, so its ownership becomes UID 65534. Accepting
that overflow UID as trusted root would discard the ownership proof and is not
an acceptable fix. Merely removing `PrivateUsers` also fails: the unprivileged
manager cannot apply the requested capability setup, returning exit 218.

A Landlock ABI probe returned `-1`, errno 38 (`ENOSYS`). This kernel cannot use
Landlock as an alternative unprivileged filesystem restriction. No kernel,
security-module configuration, linger setting, firewall or network exposure was
changed.

The owner subsequently approved the system-level unit **running as pi, not
root**. Its implementation and successful acceptance are recorded above. The
alternative—retaining a user unit—would need a different confinement
implementation and host support. A kernel upgrade alone would not fix the
rejected namespace-based template.

## Historical standalone native measurements

These probes ran outside the failed service, against the verified native package.
They are **not** a successful hardware gate or a substitute for Mother readiness.

| Measurement | Result | Scope |
| --- | --- | --- |
| WASI component compilation | 29.558366 ms | One fresh probe process; 336-byte component |
| Compile-boundary peak RSS | 10632 KiB | Whole helper-process VmHWM |
| WASI execution | success | Actual random host import, checked exported result |
| Durable-write p50 | 3.577396 ms | Forty single-frame append+sync_data samples |
| Durable-write p95 | 4.426946 ms | Nearest-rank percentile |
| Durable-write maximum | 4.706174 ms | All forty entries validated after reopen |
| Mother cold readiness | unavailable | Service refused startup |
| Mother idle/peak RSS | unavailable | No successful resident tenure |

Both standalone probe processes exited 0. No power-loss, large-history,
concurrency or Pi 3 fitness claim follows from these measurements.

## Earlier regression and blast-radius evidence

The safety fact is that installation cannot replace canonical authority or
reinterpret filesystem ownership. It is established through real CLI tests,
unchanged historical-profile tests, and the running Linux service's refusal
above. The latter is a correct refusal, not an acceptance pass.

- `bash scripts/ci-tier0.sh test`: 875 primary passes and four helper invocations,
  zero failures, two existing ignores. Local dev/test debug info disabled as in
  hosted CI; test selection and assertions retained.
- `cargo fmt --check` and `bash scripts/ci-tier0.sh clippy`: passed.
- `bash scripts/ci-tier0.sh specs`: passed, including five installer tests.
  Installer tests also passed under `umask 000`.
- `scripts/build-docs.sh` and `nix flake check --all-systems --no-build`: passed.
- Three existing CDP E2Es passed with a canonical macOS `TMPDIR`. The first run
  with its symlink spelling failed before broker readiness. No browser code was
  changed and no additional provider timeout work was added.
- A local real-CLI exercise of the Python ceremony passed bootstrap, signed
  status, invalid-signature refusal, revocation, SIGKILL/restart and fresh
  revoked-key refusal. This was not the Linux systemd gate.
- A live nicamaster transient unit proved read-only home/store mounts, writable
  test state and no-new-privileges. Those checks alone missed the UID remapping;
  the full Mother gate exposed it.
- The first address-family filter denied Iroh's required NETLINK_ROUTE socket
  with errno 97. A real transient-unit reproducer failed first, then passed after
  adding only AF_NETLINK. Effective/ambient privilege remains separately checked.
- The first full gate exposed invalid quoting of `WorkingDirectory`. Its unit
  failed before execution. A failing renderer assertion preceded the fix to a
  literal read-only `/` working directory. The next gate reached Mother's own
  filesystem guard and found the blocker above.
- Content-verifier failure is injected in the shared program-validation test;
  an exception prevents executable use. Native package-content verification
  passed. Publication tests reject replacement, symlinks, unsafe ancestry and
  cleanup after a changed unit.

No independent subagent review was available. The blast-radius review and live
reproducers were performed directly; they do not replace review of this PR.

## Earlier user-unit cleanup and retained evidence

The second failed gate stopped and removed its own unit and package root. The
first malformed unit could not be stopped as a loaded unit. After independently
confirming PID 0, inactive/bad-setting state and the exact generated contents,
its unit and journal were copied to its private evidence directory. Only that
verified unit and its package root were then removed. A final listing showed
zero loaded or installed `mct-admin-smoke-*.service` units.

Private test authority, canonical files and build/gate logs remain on the host
for inspection. They were not uploaded. No production identity, protected live
browser-run directory, or existing authority was inspected or changed.
