# NIC-53 nicanode1 acceptance on merged main

## Verdict

Nicanode1 passed the physical Pi 3 Mother smoke on 2026-11-01 UTC, exit 0.
The checkout was merged main at `d3e93b57483a9d9839c60aefb765a330e7f3e879`,
including PR #92. No implementation changes were needed.

The run followed the [nicanode0 sequence](nicanode0-install-evidence.md).
Only nicanode1 received host changes. Nicamaster supplied its existing signed
cache through read-only SSH and SCP access. No builds, configuration changes,
or cache listener were started there. No other fleet node was contacted.

All three reboots passed fresh, non-multiplexed dedicated-key SSH login with
strict host-key checking before the next stage. The final TCP listeners were
SSH port 22 on IPv4 and IPv6. No local Rust or package compilation occurred.
The required WASI probe did JIT-compile its small embedded component, as on
nicanode0. This is not a claim of literally zero compilation.

## Host state

| Item | Observed result |
| --- | --- |
| Hardware | Raspberry Pi 3 Model B Rev 1.2, aarch64, four cores, 909 MiB RAM |
| Hostname | `nicaNode1`, unchanged |
| OS | Debian 12 Bookworm, upgraded in place from Debian 11 |
| Kernel | `6.1.21-v8+`, upgraded from `5.15.61-v8+` during Bullseye preparation |
| Kernel and bootloader packages | `1:1.20230405-1` |
| dpkg | `1.22.22~bpo12+rpt1`, upgraded before the Bookworm distribution upgrade |
| systemd | `252.39-1~deb12u2` |
| OpenSSH server | `1:9.2p1-2+deb12u10` |
| dhcpcd5 | `1:8.1.2-1+rpt9`, original wired configuration retained |
| Nix | 2.35.4 daemon, `max-jobs = 0`, empty builders, signatures required |
| Swap | Persistent 2 GiB `/swapfile`; zero used immediately after reboot 3, 512 KiB used at final inspection |
| Final idle memory | 139 MiB used, 770 MiB available |
| Storage | Local ext4, 59 GiB root filesystem, approximately 46 GiB available |
| Installed Debian packages | 553; full inventory retained privately |
| TCP listeners | Port 22 only, IPv4 and IPv6 |
| UDP socket | Existing dhcpcd client on UDP 68, not removed |
| Smoke units and package roots | None loaded, installed, or retained after cleanup |
| Failed system units | None |
| Linger and clock | `Linger=no`, `NTPSynchronized=yes` |

K3s-agent was removed before the upgrades. The reviewed purge removed twelve
Bluetooth, ModemManager, Avahi, triggerhappy, wpasupplicant, and media packages.
The subsequent reviewed autoremove removed 83 unused packages, including
`dphys-swapfile`. SSH, dhcpcd, sudo, systemd, and the boot packages were retained.
NetworkManager remains installed, disabled, and inactive. Tailscale was not
installed or changed. The legacy `raspi-config` version `20231012~bullseye`
remains, as in the nicanode0 evidence.

## Upgrade and reboot evidence

The private backup is `/home/pi/pre-upgrade-backup/config-and-boot.tar`.
It includes boot, network, SSH, authorized-key, apt, fstab, and k3s configuration,
plus the uninstall scripts. Separate private files retain package state,
effective SSH settings, routes, and firewall rules. Tar comparison against the
live configuration passed before removal or upgrade.

The backup archive SHA-256 is:

```text
e27f8cff1057538c31ce5702c42ca9d48a9c40a7fb3dd718fe34a396fc50fab1
```

Both installed k3s scripts matched the previously reviewed nicanode0 scripts.
Uninstallation succeeded, and fresh key login confirmed SSH and dhcpcd remained
active. The Bullseye preparation and each Bookworm stage used persistent logs
under `/home/pi/nic53-node1-work`.

| Reboot | Completed stage | New boot ID | Fresh key login |
| --- | --- | --- | --- |
| 1 | Bullseye dpkg-first preparation upgrade | `2197eca5-92d4-421f-9873-92fa49bffaad` | PASS |
| 2 | Bookworm upgrade and package/access checks | `f62f6271-ad77-4b98-a512-1086de9fdfa0` | PASS |
| 3 | Slimming, persistent swap, Nix, signed cache, Mother profile | `1acf1778-0656-467e-a89e-ec4055f3571f` | PASS |

The initial boot ID was `5e2d39a0-3525-4a12-8ba3-961d8d2b4600`.
Each check disabled SSH connection multiplexing and required a changed boot ID.
The Nix daemon socket and swapfile were active after reboot 3.

The networking maintainer scripts matched those reviewed for nicanode0.
NetworkManager was temporarily masked while dhcpcd retained the wired link.
No transitional `/boot/firmware` configuration files were present.

The initial Bookworm upgrade exited 100 on the documented collision:

```text
trying to overwrite '/usr/bin/camera-bug-report', which is also in package libcamera-apps-lite (1.2.1-1)
```

The same simulated camera-removal and GnuPG dependency repair was reviewed and
applied. It exited 0 without forced overwrite. The resumed upgrade exited 0.
Original failed logs remain. No password-expiry login refusal occurred on this
host. The password-change date remained September 22, 2022, with no expiry.

Final checks found byte-identical boot configuration, network configuration,
and authorized keys. Parsing the original and current SSH configurations with
the upgraded `sshd -T` produced identical settings. `sshd -t` passed and
`dpkg --audit` was empty. No uncovered upgrade issue required a different repair.

Two reporting details are retained in the operational trail. A read-only apt
simulation outlasted its local command timeout; it was observed to finish before
the resumed upgrade started. The first final-inventory script stopped because
`systemctl list-unit-files` returns 1 for zero matches. Correcting that reporting
check and inspecting the root-owned GC directory produced a passing final report.
Neither event required package repair or a repeated signed Mother action.

## Signed cache and no-build evidence

The existing corrected cache archive was copied from nicamaster without opening
a cache listener. Its SHA-256 matched at source and destination:

```text
84cebfe4a9ac4f19d32a65f6e3c46651ddde054db70e1746fc9f19728408c6ee
```

The corresponding public key was scoped to the import and verification commands.
No new global cache trust key was installed. Import and recursive signature/content
verification passed. Recursive verification passed again after reboot, before the
smoke. Recursive content verification passed after the complete privileged
lifecycle, without repair. Signature checks were never disabled for import.

Merged main resolved to these existing outputs:

```text
Mother: /nix/store/43c376cj22mfa100nwfivkcki38iirkh-mct-admin-mother-0.2.0-nic53
Rust derivation: /nix/store/83mbwa651b4vayhzhkpmkqmj17q0l04a-mct-admin-mother-0.2.0-nic53.drv
Smoke: /nix/store/bwwzwz230005rr072z4c7vk4vc18s7gn-mother-install-smoke
```

The reviewed checkout is clean. Mother remains in pi's Nix profile, without a
production service or identity. The realization and execution commands were:

```sh
nix build .#mother-install-smoke --max-jobs 0 --builders "" \
  --out-link /home/pi/nic53-smoke-package
nix run --offline --max-jobs 0 --builders "" .#mother-install-smoke
```

The realization log contains a signed nixpkgs source substitution and a pinned
rust-overlay source unpack, not compiler builds. Neither `rustc` nor `cargo`
is on PATH. The only three local derivation logs are empty `user-environment`
logs from Nix profile/link generation. Their derivation records all name
`builtin:buildenv`, with no compiler command or input derivation. No Rust/package build was permitted on
nicanode1 or requested on nicamaster. OS package configuration and the WASI
probe are not evidence of zero local computation or zero bytecode generation.

## Smoke output

```text
Software test authority only; not a 1Password ceremony or production identity.
PASS install/start the system service as pi:pi
PASS signed status
PASS second writer rejected
PASS revoke
PASS forced restart
PASS revoked key rejected
PASS WASI probe runs
METRIC cold readiness 566.506 ms (n=1, process-cold through signed status; OS cache not purged)
METRIC idle RSS 17344..17344 KiB (n=5); Mother peak RSS 18740 KiB
METRIC component compile 30.940 ms; compile peak RSS 11160 KiB (fresh probe process, n=1)
METRIC durable-write p50=6.965 ms p95=9.285 ms max=11.926 ms (n=40; append+sync_data, verified reopen)
PASS immutable installer inputs
PASS isolated service cleanup; private evidence preserved
exit=0
```

Pre-run load was 2.44/0.85/0.31 on four cores, shortly after reboot and closure
verification. This is one hardware acceptance run, not a speed comparison.
Readiness includes authenticated status. The WASI probe checks its result, and
all forty durable writes are verified after reopen. Compile peak is the probe
process's VmHWM, not build memory or Mother RSS.

Private test authority and raw measurements remain at
`/home/pi/.mct-install-smoke-sodd9lfu`. Operational scripts, logs, reboot checks,
and inventories remain under `/home/pi/nic53-node1-work`. They were not published
wholesale. Only isolated software authority was used.

## Scope and validation limits

This docs-only change records existing merged behavior on a second physical
Pi 3. It changes no Rust, Nix packaging, installer, authority, durability,
confinement, or browser behavior. Host validation covers SSH continuity,
package health, installed closure integrity, the real system service, and
cleanup. Local `bash scripts/ci-tier0.sh specs`, `bash scripts/build-docs.sh`, and
`git diff --check` passed. Full Rust and browser suites were not rerun for this
docs-only change. No independent subagent review was available.

This does not establish full Child workloads, fleet capacity, large-history
startup, AOT safety, replication, or power-loss recovery. The three host reboots
preceded the isolated Mother smoke; they do not prove a provisioned production
Mother survives host reboot. No further node rollout or merge is included.
