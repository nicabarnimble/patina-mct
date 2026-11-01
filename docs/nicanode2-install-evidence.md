# NIC-53 nicanode2 acceptance after authorized DNS repair

## Verdict

Nicanode2 passed the physical Pi 3 Mother smoke on 2026-11-01 UTC, exit 0.
The tested checkout is merged main at
`d3e93b57483a9d9839c60aefb765a330e7f3e879`, including PR #92.
No implementation changes were needed.

The first attempt stopped before reboot 1 because DNS pointed at the host
itself. Commit `1102b39` records that blocked attempt. Doug then approved Pi-hole
`10.10.10.7`, with router `10.10.10.1` as secondary. The Pis are independent LAN
computers, not consumers of cluster-local DNS. Only nicanode2 received changes.

| Acceptance requirement | Observed result |
| --- | --- |
| Fresh dedicated-key SSH after DNS change | PASS, strict host-key checking and no multiplexing |
| DNS and apt refresh before resumed upgrades | PASS; getent exit 0, apt refresh exit 0, zero resolution errors |
| Three reboots preserve key SSH | PASS with three new boot IDs and successful DNS checks |
| Mother smoke exits 0 | PASS |
| Only TCP 22 listens | PASS, IPv4 and IPv6 |
| No local Rust or package compilation | PASS; existing signed binaries consumed with local builders disabled |

The required WASI probe JIT-compiled its embedded component. This is not a claim
of literally zero compilation. No production identity or Mother service was
provisioned. Mother remains in pi's Nix profile.

## Read-only DNS inventory

Before changing nicanode2, the operator read `/etc/resolv.conf` and
`/etc/dhcpcd.conf` on all three requested comparison hosts. These hosts were not
changed. Each used the router and resolved `deb.debian.org` successfully.
No self-pointing or dead resolver was observed on them.

| Host | resolv.conf nameserver | dhcpcd static domain_name_servers | getent exit |
| --- | --- | --- | --- |
| nicanode0, `10.10.10.20` | `10.10.10.1` | `10.10.10.1` | 0 |
| nicaNode1, `10.10.10.21` | `10.10.10.1` | `10.10.10.1` | 0 |
| nicamaster, `10.10.10.15` | `10.10.10.1` | `10.10.10.1` | 0 |
| nicanode2 before repair, `10.10.10.22` | `10.10.10.22` | `10.10.10.22` | 2 |

Full file contents and results remain in private `dns-inventory-*.txt` logs.
Nicamaster also supplied its existing corrected signed cache through read-only
SCP during the initial attempt. No build, configuration change, or cache listener
was started there. No other fleet node was contacted.

## Initial blocker and authorized repair

The original backup was verified before k3s removal. Both installed k3s scripts
matched the previously reviewed nicanode0 scripts. Removal succeeded, followed
by a fresh key SSH check.

The original package refresh returned 0 despite failures such as:

```text
Temporary failure resolving 'deb.debian.org'
Temporary failure resolving 'archive.raspberrypi.org'
W: Some index files failed to download. They have been ignored, or old ones used instead.
```

Both simulations and the original preparation script reported zero upgrades.
The preparation script's exit 0 was not accepted as a successful upgrade.
There was no TCP or UDP port 53 listener, although both DNS configuration files
pointed to nicanode2 itself. The original dhcpcd configuration still matched the
backup. DNS behavior before k3s removal was not established.

After Doug's approval, direct DNS queries to both approved resolvers returned
successful answers. The operator created exclusive root-owned mode-0600 backups:

```text
/home/pi/pre-upgrade-backup/dhcpcd.conf.before-dns
/home/pi/pre-upgrade-backup/resolv.conf.before-dns
```

Exactly one line in `/etc/dhcpcd.conf` changed:

```diff
-static domain_name_servers=10.10.10.22
+static domain_name_servers=10.10.10.7 10.10.10.1
```

Restarting dhcpcd generated these entries in `/etc/resolv.conf`:

```text
nameserver 10.10.10.7
nameserver 10.10.10.1
```

The address remained `10.10.10.22/24`. The route output was byte-identical before
and after the restart, including the default route through `10.10.10.1` on
`eth0`. Fresh dedicated-key SSH and `getent ahostsv4 deb.debian.org` passed.
SSH configuration, authorized keys, and passwords were not changed by the repair.

The first post-DNS apt refresh exited 100 because the Raspberry Pi Release file
was not valid yet against the stale host clock. The existing NTP service then
synchronized automatically. No manual time override or disabled apt validity
check was used. The repeated refresh used:

```sh
apt-get update -o APT::Update::Error-Mode=any
```

It exited 0. The saved log contained zero `Err:`, `Temporary failure`,
`Failed to fetch`, `W:`, or `E:` lines. This check and the successful getent result
were recorded in `dns-clock-gate.txt` before the resumed preparation started.
The subsequent Bookworm refresh passed the same checks before its upgrade.
The original DNS and clock failure logs remain intact.

## Upgrade and reboot evidence

After the DNS gate, the run followed the
[nicanode0 sequence](nicanode0-install-evidence.md):

1. Upgrade Bullseye dpkg first, then the preparation package set. The reviewed
   plan contained 132 upgrades and no removals. Verify configuration and package
   health before reboot 1.
2. Switch to Bookworm repositories, verify refresh, and upgrade dpkg first to
   `1.22.22~bpo12+rpt1`. All nine networking maintainer scripts matched the node0
   review. Temporarily mask the disabled, inactive NetworkManager. No transitional
   `/boot/firmware/config.txt` or `cmdline.txt` existed.
3. Apply the distribution upgrade. Resolve the documented camera collision with
   the same reviewed camera-removal and GnuPG dependency repair, without forced
   overwrite. Verify the resumed upgrade before reboot 2.
4. Purge twelve selected service and media packages. Configure persistent 2 GiB
   swap before removing dphys-swapfile. Review and remove eighty unused packages.
   Restore NetworkManager to disabled and inactive, not masked.
5. Install Git and the pinned Nix binary distribution. Import and verify the
   existing signed cache. Realize the exact merged-main package with local
   builders disabled and retain Mother in pi's profile. Reboot a third time.
6. Verify persistent swap, the Nix daemon socket, SSH, and DNS before running the
   offline Mother smoke as pi.

| Reboot | Completed stage | New boot ID | Fresh key SSH and DNS |
| --- | --- | --- | --- |
| 1 | Bullseye preparation | `aa30c0ba-fd24-44cd-8da2-1bc57f51138d` | PASS |
| 2 | Bookworm upgrade and checks | `61aefb2a-53e7-4a75-b3ef-1365a14b1675` | PASS |
| 3 | Slimming, swap, Nix, signed cache, Mother profile | `67a8d739-aa43-4143-9214-062200bbd092` | PASS |

The original boot ID was `cb5862bd-5f95-4c73-ae2b-59a89b5c6819`.
Each reboot check required a changed boot ID and a new non-multiplexed SSH
connection using the dedicated key and strict host-key checking.

The initial Bookworm upgrade exited 100 on the expected collision:

```text
trying to overwrite '/usr/bin/camera-bug-report', which is also in package libcamera-apps-lite (1.2.1-1)
```

The repair and resumed upgrade each exited 0. No password-expiry refusal occurred.
The password-change date remained February 21, 2023, with no expiry. Final
`dpkg --audit` was empty and `sshd -t` passed. Boot configuration, network
interfaces, and authorized keys match the original backup. The comparison permits
only the exact approved DNS replacement in dhcpcd. Original and current SSH
configuration produce identical effective settings under the upgraded parser.

A read-only distribution simulation exceeded its local 120-second timeout. Its
process was observed to finish, and its complete output was reviewed before
starting the upgrade. A later provenance checker initially expected the older
Nix JSON shape. Reading its version-4 output and correcting the checker produced
a passing result. Neither reporting issue required host repair or a repeated
signed Mother action. Original reporting evidence remains private.

## Final host state

| Item | Observed result |
| --- | --- |
| Hardware | Raspberry Pi 3 Model B Rev 1.2, aarch64, four cores, 909 MiB RAM |
| Hostname | `nicanode2`, unchanged |
| OS and kernel | Debian 12 Bookworm, `6.1.21-v8+`, upgraded from Debian 11 and `5.15.84-v8+` |
| Kernel and bootloader packages | `1:1.20230405-1` |
| dpkg | `1.22.22~bpo12+rpt1` |
| systemd | `252.39-1~deb12u2` |
| OpenSSH server | `1:9.2p1-2+deb12u10` |
| dhcpcd5 | `1:8.1.2-1+rpt9` |
| DNS | Pi-hole `10.10.10.7`, router `10.10.10.1` secondary; persisted across all three reboots |
| Nix | 2.35.4 daemon, `max-jobs=0`, empty builders, signatures required |
| Swap | Persistent 2 GiB `/swapfile`; zero used after reboot 3, 1 MiB used at final inspection |
| Final memory sample | 136 MiB used, 772 MiB available |
| Storage | Local `/dev/mmcblk0p2` ext4, 59 GiB root filesystem, approximately 46 GiB available |
| Installed Debian packages | 557 |
| TCP listeners | Port 22 only, IPv4 and IPv6 |
| UDP socket | Existing dhcpcd client on UDP 68 |
| Failed units, smoke units, smoke package roots | None |
| Linger and clock | `Linger=no`, `NTPSynchronized=yes` |

NetworkManager remains installed but disabled and inactive. Tailscale is absent
and was not installed or changed. Legacy `raspi-config` remains
`20231012~bullseye`, as on the earlier hosts. This is not a claim that every
optional utility or the legacy kernel is current. No claim of zero UDP sockets
is made.

## Signed cache and no-build evidence

The pinned Nix installer SHA-256 matched:

```text
118c3dfa04bdc946b3f577365b9a295607142c9b920e2c07b184e266d62b1ee1
```

The transferred cache archive and public key matched the previously recorded
hashes. The signing key was scoped to import and verification commands. The only
global trusted public key remains the default `cache.nixos.org` key. Import
required signatures, and recursive signature and content verification passed
before execution. Recursive content verification passed after the complete
privileged lifecycle and again at final confirmation. Nothing was repaired in
the store, and import signature checks were not disabled.

Merged main resolved to the existing outputs:

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
input derivation. OS package configuration, Nix profile generation, and the tiny
WASI JIT probe are not zero local computation. No Rust or package build was run
on nicanode2 or requested on nicamaster.

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
METRIC cold readiness 573.812 ms (n=1, process-cold through signed status; OS cache not purged)
METRIC idle RSS 16380..17544 KiB (n=5); Mother peak RSS 19564 KiB
METRIC component compile 30.648 ms; compile peak RSS 10640 KiB (fresh probe process, n=1)
METRIC durable-write p50=6.121 ms p95=8.522 ms max=13.483 ms (n=40; append+sync_data, verified reopen)
PASS immutable installer inputs
PASS isolated service cleanup; private evidence preserved
exit=0
```

Pre-run load was 2.04/0.62/0.22 on four cores shortly after reboot and closure
verification. This is one acceptance run, not a speed comparison. Readiness
includes authenticated status. The WASI result is checked, and all forty durable
writes are verified after reopen. Compile peak is the probe process's VmHWM,
not Mother RSS or build memory.

## Retained evidence and limits

Private test authority and raw measurements remain at
`/home/pi/.mct-install-smoke-xvb0ilfj`. Operational scripts, inventories, original
failures, DNS checks, and reboot logs remain under `/home/pi/nic53-node2-work`.
`RUN.md` records the initial stop. `RESUME.md` records the authorized continuation.
These private files were not published wholesale.

The original private backup is
`/home/pi/pre-upgrade-backup/config-and-boot.tar`. Retained artifact hashes are:

```text
config-and-boot.tar
5a5eccf00e55533afe1198c83fa437414c28d743225c4b4310e8f3232cb810e8

nic53-node0-cache-fixed.tar
84cebfe4a9ac4f19d32a65f6e3c46651ddde054db70e1746fc9f19728408c6ee

nic53-node0-cache.pub
5af40c5f01b8b7866a842c01b14603205feddfd976e4c367364c42f73065a44d
```

This documentation-only repository change leaves authority, durability,
confinement, packaging, and browser behavior unchanged. Workstation validation
includes `bash scripts/ci-tier0.sh specs`, `bash scripts/build-docs.sh`, and
`git diff --check`. Full Rust and browser suites were not rerun. No independent
subagent review was available.

This proves bounded admin-only acceptance, not full Child workloads, fleet
capacity, AOT safety, replication, large-history startup, or power-loss recovery.
The three host reboots preceded the isolated smoke and do not prove a provisioned
production Mother survives reboot. Nicanode1's evidence branch remains at
`09bf5fbc6d25cb30892a9967f355609abb13f1fa`. No push or merge is included.
