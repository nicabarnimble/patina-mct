# NIC-53 first Pi 3 acceptance

## Verdict

The physical nicanode0 smoke exited 0 on 2026-11-01 UTC. The Mother Rust package
is the unchanged package delivered by merged PR #91, `485bf80`. Installation
required the bytecode fix at `eda91ce0056ec91e30da6b30d48f3581772f5329`, on a
separate branch. This evidence does not claim that the unmodified merged
installer passed or that the correction has merged.

Only nicanode0 received OS/service changes. Nicamaster supplied a signed offline
Nix cache. No other fleet node was changed and no cache listener was opened.

## Host state

| Item | Observed result |
| --- | --- |
| Hardware | Raspberry Pi 3B, aarch64, four cores, 909 MiB reported RAM |
| OS | Debian 12 Bookworm, upgraded in place from Bullseye |
| Kernel | `6.1.21-v8+`, same legacy Raspberry Pi kernel as nicamaster |
| Kernel/bootloader packages | `1:1.20230405-1` |
| systemd | `252.39-1~deb12u2` |
| OpenSSH server | `1:9.2p1-2+deb12u10` |
| dhcpcd5 | `1:8.1.2-1+rpt9`, original static wired configuration |
| Nix | 2.35.4 daemon, `max-jobs = 0`, empty builders, signature checks enabled |
| Swap | 2 GiB `/swapfile`, enabled through fstab and verified after reboot; zero used at final sample |
| Idle host memory | 143 MiB used, 765 MiB available, after smoke and closure verification |
| Storage | Local ext4, 59 GiB root filesystem, approximately 46 GiB available |
| Installed Debian packages | 557, full inventory retained privately on the host |
| TCP listeners | SSH port 22 on IPv4 and IPv6 only |
| UDP socket | Existing dhcpcd client on UDP 68, also present on nicamaster |
| Smoke units/package roots | None loaded, installed or retained after cleanup |
| Linger / time synchronization | Disabled / synchronized |

Bluetooth, ModemManager, Avahi, triggerhappy, wpasupplicant and the camera/media
package chain were removed. Eighty unused packages were then removed, including
the old dphys swap manager and obsolete Python/media dependencies. Tailscale was
absent and was not installed or changed. NetworkManager remains installed but
disabled, matching nicamaster.

`raspi-config` remains `20231012~bullseye`, matching nicamaster. A newer version
is available but its simulated upgrade replaces the legacy Raspberry Pi support
libraries and adds EEPROM utilities. It was not forced. This is not a claim that
the legacy kernel or every optional utility is current.

The preserved UDP DHCP client means this is **not** a claim of zero UDP sockets.
The existing network configuration was not changed merely to hide that socket.

## Upgrade and SSH checks

Private recovery files are under `/home/pi/pre-upgrade-backup`. They include boot,
network and SSH configuration, authorized keys, network systemd/udev rules and
NetworkManager configuration. These archives contain sensitive material and were
not published. Operational logs and package inventories are under
`/home/pi/nic53-node0-work`.

The sequence was:

1. Back up configuration, inspect the installed k3s scripts, uninstall k3s-agent,
   then confirm a fresh dedicated-key SSH command.
2. Comment out the obsolete Bullseye security source. Upgrade dpkg and the
   Bullseye package set. Reboot and confirm dedicated-key login before continuing.
3. Switch to Bookworm sources, including its working security repository.
   Upgrade dpkg to `1.22.22~bpo12+rpt1` before the full distribution upgrade.
   Inspect networking maintainer scripts and temporarily mask the already-disabled
   NetworkManager so it cannot compete with the active dhcpcd service.
4. Resolve the camera-package collision without `--force-overwrite`, finish the
   upgrade, audit package state, verify access configuration and reboot. Confirm
   dedicated-key login before proceeding.
5. Purge the requested services/media packages, review and remove unused packages,
   replace the small dphys swap with 2 GiB, and install the verified Nix binary
   distribution used on nicamaster. Restore NetworkManager to disabled, not masked.
6. Import the signed package closure, realize it with local builders disabled,
   retain Mother in pi's Nix profile, then reboot. Confirm dedicated-key login,
   persistent swap and the Nix daemon socket before running the smoke.

All three reboots passed fresh dedicated-key login with strict host-key checking.
Boot files, static network configuration and authorized keys still match the
backup. OpenSSH replaced an unmodified distribution configuration file during
upgrade. Parsing both old and new files with the upgraded `sshd -T` produced
identical effective settings. The byte-for-byte comparison correctly reported
that file replacement; it was not treated as an upgrade failure or silently
ignored. Final `dpkg --audit` was empty and `sshd -t` passed.

One login during the package transition returned a password-expiry message.
Subsequent interactive and command key logins succeeded without submitting or
changing a password. `chage` reported the unchanged 2023 password-change date
and no password/account expiry. No reboot occurred while command access was
unverified. The underlying transient PAM cause was not established.

The first distribution upgrade stopped because `rpicam-apps-core` tried to
replace `/usr/bin/camera-bug-report`, still owned by the old `libcamera-apps-lite`.
The authorized camera chain was removed and the interrupted GnuPG dependencies
were completed using a reviewed apt plan. The resumed upgrade passed package
health checks. Original failed logs remain on the host.

## Immutable package input defect

The first signed cache import failed closed:

```text
error: hash mismatch importing path '/nix/store/hhfzdw280vjfvmj13k1ipc86as896181-scripts'
```

Nicamaster's registered scripts input had acquired root-owned
`mother_systemd.cpython-314.pyc` and `passkey_queue.cpython-314.pyc` files.
`nix-store --verify-path` independently reported the modified path. The privileged
Python installer could write these files even though the Nix directory had
read-only mode bits. Its registered hash therefore disagreed with its exported
bytes. Signature and content-hash checks were not disabled to import it.

The correction sets `sys.dont_write_bytecode` before the installer imports its
local modules. A subprocess regression stages writable script inputs and proves
that invoking the actual installer leaves their names and bytes unchanged. It
failed with an extra `__pycache__` directory before the fix and passed afterward.
The smoke now verifies the scripts store path before installation and after
privileged cleanup. Existing corrupt artifacts were not repaired or deleted.
A new, clean scripts input was built from the corrected branch.

Nicamaster verified the corrected closure before signing/export. Nicanode0
imported it with the corresponding public key scoped to that copy command,
without installing a new global cache trust key. Recursive signature/content
verification passed before execution. Recursive content verification also
passed after the complete privileged lifecycle on the Pi 3.

## Build provenance and compilation limits

The unchanged Rust package and derivation are:

```text
/nix/store/43c376cj22mfa100nwfivkcki38iirkh-mct-admin-mother-0.2.0-nic53
/nix/store/83mbwa651b4vayhzhkpmkqmj17q0l04a-mct-admin-mother-0.2.0-nic53.drv
```

Nicamaster built only the corrected shell wrappers and supplied the complete
runtime closure. Nicanode0 used:

```sh
nix build .#mother-install-smoke --max-jobs 0 --builders "" \
  --out-link /home/pi/nic53-smoke-package
nix run --offline --max-jobs 0 --builders "" .#mother-install-smoke
```

There was no local Rust or package compilation. Realization logs show only a
Nix source substitution and an overlay source unpack. Neither `cargo` nor `rustc`
is on the host PATH. The local derivation logs are Nix user-environment link
generation, not compiler builds. The smoke ran offline with its package closure
already present.

The required WASI measurement **does compile its embedded 336-byte component
locally** through Wasmtime. Zero Wasm compilation would contradict that probe
and its compile-time/peak measurement. No deployment component build or AOT
artifact loading was performed. This distinction was stated before the run.

## Smoke output

The committed corrected implementation produced:

```text
Software test authority only; not a 1Password ceremony or production identity.
PASS install/start the system service as pi:pi
PASS signed status
PASS second writer rejected
PASS revoke
PASS forced restart
PASS revoked key rejected
PASS WASI probe runs
METRIC cold readiness 430.800 ms (n=1, process-cold through signed status; OS cache not purged)
METRIC idle RSS 18272..18272 KiB (n=5); Mother peak RSS 19696 KiB
METRIC component compile 33.854 ms; compile peak RSS 10940 KiB (fresh probe process, n=1)
METRIC durable-write p50=3.645 ms p95=5.242 ms max=5.485 ms (n=40; append+sync_data, verified reopen)
PASS immutable installer inputs
PASS isolated service cleanup; private evidence preserved
exit=0
```

Pre-run load averages were 1.67/0.37/0.12 on four cores shortly after reboot.
This is one hardware acceptance run, not a comparative benchmark. Readiness ends
at authenticated status, the WASI result is checked, and all forty acknowledged
durable writes are validated after reopen. Raw samples and software test
authority remain private on the host. No production Mother was provisioned.

## Blast radius and remaining scope

The fix changes installer interpreter behavior and adds an integrity assertion.
It changes no Rust, authority format, canonical ownership rule, durability rule,
Darwin path, browser flow or Linux Git permission. The unchanged Rust derivation,
regression-first subprocess test, physical smoke, and post-lifecycle closure
verification establish this boundary directly. Twelve installer tests pass
normally and under `umask 000`. `cargo fmt --check`, `bash scripts/ci-tier0.sh specs`,
`bash scripts/build-docs.sh`, `git diff --check` and
`nix flake check --all-systems --no-build` pass. Full Rust and browser suites were
not rerun for this Python-only correction; no Rust or browser source changed.
Final hosted CI still requires review before merge. No independent subagent
review was available.

This proves the bounded admin-only gate on one Pi 3. It does not prove full Child
workloads, large-history startup, power-loss recovery, replication, AOT safety or
fleet capacity. The installer correction requires its own review and merge;
no merge or additional host rollout was authorized.

## Subsequent delivery

PR #92 later merged at `d3e93b5` with owner authorization. The next authorized
host, nicanode1, passed the same sequence on that merged implementation.
See the [nicanode1 evidence](nicanode1-install-evidence.md). The original failed
imports and this first-host evidence remain historical records.
