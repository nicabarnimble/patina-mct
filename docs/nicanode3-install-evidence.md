# NIC-53 nicanode3 acceptance on merged main

## Verdict

Nicanode3 passed the physical Pi 3 Mother smoke on 2026-11-01 UTC, exit 0.
All three reboots passed fresh dedicated-key SSH and DNS checks. Final TCP
listeners were SSH port 22 on IPv4 and IPv6 only. The existing DHCP client
retains UDP 68. This is not a claim of zero UDP sockets.

The tested checkout is merged main at
`aa87237008ea2508ea125128b312fc269c67444f`. Since node2's tested `d3e93b5`,
PR #94 changed only verification-skill files. Rust sources, installer scripts,
Nix inputs, and package inputs are unchanged. Nix independently resolved the new
checkout to the existing signed Mother derivation and smoke output.

Only nicanode3 received host changes. Nicamaster supplied existing cache files
through read-only SCP. No build, configuration change, or cache listener was
started there. No other node was operated on, and the other evidence branches
remain unchanged. No implementation changes were needed.

No Rust or package compilation occurred on the Pi. The required tiny WASI
probe did JIT-compile its embedded component, as disclosed before the run.
No production Mother identity or service was provisioned.

## Backup, DNS, and pre-upgrade gate

The baseline was Pi 3 Model B Rev 1.2, aarch64 Debian 11, kernel `5.15.84-v8+`,
909 MiB RAM, and 99 MiB swap. K3s-agent, SSH, and dhcpcd were active.
NetworkManager was inactive and disabled. The package audit was empty, with no
held packages.

The operator created `/home/pi/pre-upgrade-backup/config-and-boot.tar` and
compared its contents against the live configuration before mutation. The private
archive includes boot, SSH, network, apt, fstab, authorized-key, and k3s
configuration. Package inventories, effective SSH settings, routes, and firewall
rules remain alongside it. The archive SHA-256 was verified again at handoff:

```text
a4186397c985a214ab942a5f882c61b4024c0dd7dfcbc73fb365100b465aa06e
```

DNS initially pointed to router `10.10.10.1` in both `/etc/resolv.conf` and
`/etc/dhcpcd.conf`, not to the host itself. Direct queries to both approved
resolvers returned successful answers. Exclusive root-owned mode-0600 backups
were created before changing DNS:

```text
/home/pi/pre-upgrade-backup/dhcpcd.conf.before-dns
/home/pi/pre-upgrade-backup/resolv.conf.before-dns
```

Exactly one dhcpcd line changed:

```diff
-static domain_name_servers=10.10.10.1
+static domain_name_servers=10.10.10.7 10.10.10.1
```

`dhcpcd -n eth0` applied the change to the wired interface without restarting
cluster interfaces while k3s was still present. The complete IPv4 route output
was byte-identical before and after this DNS operation. The address stayed
`10.10.10.23/24`, with default route through `10.10.10.1` on `eth0`.
The resulting resolver file contains:

```text
nameserver 10.10.10.7
nameserver 10.10.10.1
```

A fresh, non-multiplexed SSH connection used
`~/.ssh/mother_cluster_ed25519`, `IdentitiesOnly=yes`, `BatchMode=yes`, and
`StrictHostKeyChecking=yes`. It passed, as did
`getent ahostsv4 deb.debian.org`, exit 0. Boot files, SSH configuration, and
authorized keys still matched the backup. Time synchronization was already active.

The same Bullseye source configuration used on the earlier nodes omitted the
obsolete security endpoint. Before removing k3s or upgrading any package, this
refresh passed:

```sh
apt-get update -o APT::Update::Error-Mode=any
```

Its log contained zero `Err:`, `Temporary failure`, `Failed to fetch`, `W:`, or
`E:` lines. `dns-apt-gate.txt` records the successful SSH, DNS, configuration,
and apt checks before the next stage.

Both installed k3s scripts matched the previously reviewed node0 scripts.
Removal exited 0. A second fresh SSH and DNS check passed, followed by another
clean apt refresh before Bullseye preparation. K3s routes disappeared with the
authorized removal; the wired address and default route remained unchanged.

## Upgrade and reboot evidence

The run reused the [nicanode0 sequence](nicanode0-install-evidence.md) and the
node2 operational scripts, with node3 path, hostname, DNS-baseline, and source
revision checks:

1. Review and apply the dpkg-first Bullseye preparation. The plan contained 132
   upgrades and no removals. Package and access checks passed before reboot 1.
2. Switch to Bookworm sources and require another clean apt refresh. Upgrade dpkg
   first to `1.22.22~bpo12+rpt1`. All nine networking maintainer scripts matched
   the earlier node0 review. Temporarily mask the disabled NetworkManager.
   No transitional `/boot/firmware/config.txt` or `cmdline.txt` existed.
3. Review the distribution-upgrade simulation, which exited 0. Its package
   actions matched node2. Apply the upgrade and the documented camera repair,
   then verify package health and SSH before reboot 2.
4. Purge twelve selected Bluetooth, Avahi, modem, triggerhappy, wireless, and media
   packages. Configure persistent 2 GiB swap before removing dphys-swapfile.
   Review and remove eighty unused packages. Both removal plans matched node2.
   Restore NetworkManager to disabled and inactive, not masked.
5. Install Git and the pinned Nix binary distribution. Import the existing signed
   cache, verify it, realize the merged-main package with local builders disabled,
   and retain Mother in pi's profile. Reboot a third time.
6. Verify the Nix daemon socket, swap, fresh SSH, and DNS before the offline smoke.

| Reboot | Completed stage | New boot ID | Fresh key SSH and DNS |
| --- | --- | --- | --- |
| 1 | Bullseye preparation | `d1cc9430-0ec7-4bb5-8bd5-b29098d1ce33` | PASS |
| 2 | Bookworm upgrade and checks | `bbae4746-efb5-4e82-9285-20ad8d28319a` | PASS |
| 3 | Slimming, swap, Nix, signed cache, Mother profile | `4e8c0ccf-01e0-4c10-a141-1b3511916f00` | PASS |

The original boot ID was `189346e8-6bb9-422e-af8e-dd6b4f48fa4f`.
Each check required a changed boot ID and a fresh dedicated-key connection with
SSH multiplexing disabled before the next stage.

The initial Bookworm upgrade exited 100 on the documented collision:

```text
trying to overwrite '/usr/bin/camera-bug-report', which is also in package libcamera-apps-lite (1.2.1-1)
```

The camera-removal and GnuPG dependency repair plan matched node2's package
actions exactly. The repair and resumed upgrade each exited 0, without forced
overwrite. The original failed log remains. No other upgrade repair was needed.

No password-expiry refusal occurred. The password-change date remained February
21, 2023, with no expiry. Final `dpkg --audit` was empty and `sshd -t` passed.
Boot configuration, network interfaces, and authorized keys match the original
backup. The dhcpcd comparison allows only the exact approved DNS replacement.
Original and current SSH configuration produce identical effective settings
under the upgraded SSH parser.

## Final host state

| Item | Observed result |
| --- | --- |
| Hardware | Raspberry Pi 3 Model B Rev 1.2, aarch64, four cores, 909 MiB RAM |
| Hostname | `nicanode3`, unchanged |
| OS and kernel | Debian 12 Bookworm, `6.1.21-v8+` |
| Kernel and bootloader packages | `1:1.20230405-1` |
| dpkg | `1.22.22~bpo12+rpt1` |
| systemd | `252.39-1~deb12u2` |
| OpenSSH server | `1:9.2p1-2+deb12u10` |
| dhcpcd5 | `1:8.1.2-1+rpt9` |
| DNS | Pi-hole `10.10.10.7`, router `10.10.10.1` secondary; persisted across all reboots |
| Nix | 2.35.4 daemon, `max-jobs=0`, empty builders, signatures required |
| Swap | Persistent 2 GiB `/swapfile`; zero used after reboot 3, 768 KiB at final inspection |
| Final memory sample | 138 MiB used, 771 MiB available |
| Storage | Local `/dev/mmcblk0p2` ext4, 59 GiB root filesystem, approximately 46 GiB available |
| Installed Debian packages | 557 |
| TCP listeners | Port 22 only, IPv4 and IPv6 |
| UDP socket | Existing dhcpcd client on UDP 68 |
| Failed units, smoke units, smoke package roots | None |
| Linger and clock | `Linger=no`, `NTPSynchronized=yes` |

Tailscale is absent and was not installed or changed. NetworkManager remains
disabled and inactive. Legacy `raspi-config` remains `20231012~bullseye`, as on
the earlier hosts. This is not a claim that every optional utility or the legacy
kernel is current. A final fresh SSH and DNS check passed after the smoke.

## Signed cache and no-build evidence

Nicamaster supplied its existing corrected cache archive and public key.
Their SHA-256 values matched the previously recorded values:

```text
nic53-node0-cache-fixed.tar
84cebfe4a9ac4f19d32a65f6e3c46651ddde054db70e1746fc9f19728408c6ee

nic53-node0-cache.pub
5af40c5f01b8b7866a842c01b14603205feddfd976e4c367364c42f73065a44d
```

The pinned Nix installer SHA-256 also matched:

```text
118c3dfa04bdc946b3f577365b9a295607142c9b920e2c07b184e266d62b1ee1
```

Import required signatures. The cache signing key was scoped to import and
verification commands; the global trusted key remains the default
`cache.nixos.org` key. Recursive signature and content verification passed before
execution. Recursive content verification passed after the complete privileged
lifecycle, exit 0. No store repair or import-signature bypass was used.

The clean checkout at `aa87237` resolved to these existing outputs:

```text
Mother: /nix/store/43c376cj22mfa100nwfivkcki38iirkh-mct-admin-mother-0.2.0-nic53
Rust derivation: /nix/store/83mbwa651b4vayhzhkpmkqmj17q0l04a-mct-admin-mother-0.2.0-nic53.drv
Smoke: /nix/store/bwwzwz230005rr072z4c7vk4vc18s7gn-mother-install-smoke
```

The realization and execution commands were:

```sh
nix build .#mother-install-smoke --max-jobs 0 --builders "" \
  --out-link /home/pi/nic53-smoke-package
nix run --offline --max-jobs 0 --builders "" .#mother-install-smoke
```

The realization log shows a signed nixpkgs source substitution and a pinned
overlay source unpack, not compiler builds. Neither `rustc` nor `cargo` is on
PATH. The only three local derivation logs are empty `user-environment` logs.
Their version-4 records name `builtin:buildenv`, with no compiler command or
input derivation. Nix profile generation, OS package configuration, and the
WASI JIT probe are not zero local computation.

The new verification skill's build-oriented launch path was not run on the Pi.
The actual native smoke above is the acceptance evidence. Mother remains in pi's
profile without a production service or identity.

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
METRIC cold readiness 548.603 ms (n=1, process-cold through signed status; OS cache not purged)
METRIC idle RSS 17960..17960 KiB (n=5); Mother peak RSS 19328 KiB
METRIC component compile 29.525 ms; compile peak RSS 10756 KiB (fresh probe process, n=1)
METRIC durable-write p50=24.743 ms p95=73.393 ms max=119.636 ms (n=40; append+sync_data, verified reopen)
PASS immutable installer inputs
PASS isolated service cleanup; private evidence preserved
exit=0
```

Pre-run load was 2.23/0.68/0.24 on four cores shortly after reboot and closure
verification. This is one acceptance run, not a speed comparison. Readiness
includes authenticated status. The WASI result is checked, and all forty durable
writes are verified after reopen. Compile peak is the probe process's VmHWM,
not Mother RSS or build memory. No cause for the observed write latencies is
inferred from this single run.

## Evidence and validation limits

Private software authority and measurements remain at
`/home/pi/.mct-install-smoke-xmfev695`. Operational scripts, original failures,
inventories, DNS gates, reboot logs, and `RUN.md` remain under
`/home/pi/nic53-node3-work`. The private backup and test authority were not
published or deleted.

This repository change adds only this evidence document. It changes no authority,
durability, confinement, packaging, or browser behavior. Workstation validation
includes `bash scripts/ci-tier0.sh specs`, `bash scripts/build-docs.sh`, and
`git diff --check`. Full Rust and browser suites were not rerun for this docs-only
change. No independent subagent review was available.

This establishes bounded admin-only acceptance, not full Child workloads, fleet
capacity, replication, AOT safety, large-history startup, or power-loss recovery.
The host reboots preceded the isolated smoke and do not prove a provisioned
production Mother survives reboot. No further rollout, push, or merge is included.
