# NIC-53 installation evidence and blocker

## Verdict

**Blocked. The complete hardware gate exits 1, not PASS.** The native Nix package
builds, but the proposed user-service sandbox changes filesystem ownership as
seen by Mother. The existing ownership guard correctly refuses startup. No
ownership, authority or Linux Git confinement check was removed to force a pass.

Tested implementation: `7479d9c8ad8760ed11d7b61795e29de43bcfc7b1`.
Hardware runs occurred on 2026-10-31 on nicamaster, a Raspberry Pi 4 Model B
Rev 1.2 with four cores and 3794 MiB reported RAM. The host runs Debian 12,
`aarch64`, kernel `6.1.21-v8+`, local ext4 storage and 4095 MiB swap. No claim
about SSD identity is inferred from filesystem size.

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

## Hardware gate failure

Command:

```sh
nix run .#mother-install-smoke
```

Latest gate result:

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

The owner must choose a supported service/confinement design before completion:

- A system-level systemd unit **running as `pi`, not root**, can use the system
  manager to establish the filesystem sandbox without changing user namespaces.
  This changes the requested user-service scope and has not been implemented or
  tested here.
- Retaining a user unit needs a different confinement implementation and host
  support, such as a kernel with suitable Landlock support. Upgrading the kernel
  alone does not fix the current namespace-based unit.

## Standalone native measurements

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

## Regression and blast-radius evidence

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

## Cleanup and retained evidence

The second failed gate stopped and removed its own unit and package root. The
first malformed unit could not be stopped as a loaded unit. After independently
confirming PID 0, inactive/bad-setting state and the exact generated contents,
its unit and journal were copied to its private evidence directory. Only that
verified unit and its package root were then removed. A final listing showed
zero loaded or installed `mct-admin-smoke-*.service` units.

Private test authority, canonical files and build/gate logs remain on the host
for inspection. They were not uploaded. No production identity, protected live
browser-run directory, or existing authority was inspected or changed.
