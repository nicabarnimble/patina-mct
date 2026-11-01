# NIC-53 nicanode4 acceptance on merged main

## Verdict

Nicanode4 passed the Mother smoke on 2026-11-01 UTC, exit 0, after all three
host reboots. Each reboot passed fresh dedicated-key SSH and DNS checks.
Final TCP listeners were port 22 on IPv4 and IPv6 only. The existing dhcpcd
client retains the allowed UDP 68 socket.

The tested checkout is merged main at
`aa87237008ea2508ea125128b312fc269c67444f`, the same revision used on node3.
It resolved to the existing signed Mother derivation and smoke output.
No implementation changes or local Rust/package builds were needed.
The required tiny WASI probe did JIT-compile its component, as disclosed before
work. This is not a claim of literally zero compilation.

Only nicanode4 received host changes. Nicamaster supplied existing cache files
through read-only SCP, with no build, configuration change, or cache listener.
No other node was operated on. No production Mother identity or service was
provisioned.

## Inventory gate and execution plan

Before contacting the host, the operator checked the local fleet inventory note
`nicabar-vault/🌞 Daily/2023-04-15.md`, line 25. It pairs `nicanode4` with
`10.10.10.24`, matching the expected address. Only that hostname/address pair
was extracted; the note itself remains private. The first SSH connection then
independently confirmed hostname `nicanode4` and static address `10.10.10.24/24`.

`/tmp/nic53-node3-plan.md` was copied to `/tmp/nic53-node4-plan.md`, then its
completed outcomes were reset for this host. Node3's plan was not changed.
The same operational scripts were reused with node4 hostname, address, and path
guards. Node3's results were not counted as node4 evidence.

The new local branch `docs/nic53-nicanode4-acceptance` starts at `origin/main`
`aa87237`. Local `main` remains `d3e93b5`. The node1, node2, and node3 evidence
refs existed at the start but were absent at the pre-commit check. This run issued
no deletion or rename commands for those branches and did not recreate them.
The cause of their disappearance was not established. No push or merge is included.

## Backup, DNS, and pre-upgrade checks

The baseline was a Raspberry Pi 3 Model B Rev 1.2, aarch64 Debian 11, kernel
`5.15.84-v8+`, 909 MiB RAM, and 99 MiB swap. K3s-agent, SSH, and dhcpcd were
active. NetworkManager was disabled. Package audit was empty, with no holds.

The private backup is `/home/pi/pre-upgrade-backup/config-and-boot.tar`.
Tar comparison against live configuration passed before mutation. It includes
boot, SSH, network, apt, fstab, authorized-key, and k3s configuration. Separate
private files retain package state, effective SSH settings, routes, and firewall
rules. Its SHA-256 matched again at final inspection:

```text
ddae507ffd33c1e676cee2b959c5f6142cddaddfa013b430deaf98410b5d33c1
```

Both DNS files initially used router `10.10.10.1`, not the host itself.
Direct queries to Pi-hole `10.10.10.7` and the router both returned successful
answers. Exclusive root-owned mode-0600 backups were created before the DNS change:

```text
/home/pi/pre-upgrade-backup/dhcpcd.conf.before-dns
/home/pi/pre-upgrade-backup/resolv.conf.before-dns
```

Exactly one line in `/etc/dhcpcd.conf` changed:

```diff
-static domain_name_servers=10.10.10.1
+static domain_name_servers=10.10.10.7 10.10.10.1
```

`dhcpcd -n eth0` applied the change to the wired interface. The full IPv4 route
output was byte-identical before and after this operation. Static address
`10.10.10.24/24` and gateway `10.10.10.1` remained unchanged.
`/etc/resolv.conf` then contained the approved resolver order:

```text
nameserver 10.10.10.7
nameserver 10.10.10.1
```

Fresh SSH used `~/.ssh/mother_cluster_ed25519`, `IdentitiesOnly=yes`,
`BatchMode=yes`, `StrictHostKeyChecking=yes`, and disabled multiplexing.
SSH and `getent ahostsv4 deb.debian.org` passed after the DNS change.
Boot files, SSH configuration, and authorized keys still matched the backup.
Time synchronization was active.

The same Bullseye sources used on the earlier nodes omitted the obsolete security
endpoint. Before k3s removal or any package upgrade, the following refresh exited 0:

```sh
apt-get update -o APT::Update::Error-Mode=any
```

Its log contained zero `Err:`, `Temporary failure`, `Failed to fetch`, `W:`, or
`E:` lines. `dns-apt-gate.txt` records the checks. Both installed k3s scripts
matched node3's reviewed copies. Removal exited 0, followed by fresh SSH, DNS,
and another clean apt refresh before preparation. K3s routes disappeared with
the authorized removal; the wired address and default route remained unchanged.

## Upgrade and reboot evidence

The run reused the [nicanode0 sequence](nicanode0-install-evidence.md) as exercised
on node3:

1. Apply the reviewed dpkg-first Bullseye preparation. Its 132 planned upgrades
   and no removals matched node3. Verify configuration and package health before
   reboot 1.
2. Switch to Bookworm sources and require a clean apt refresh. Upgrade dpkg first
   to `1.22.22~bpo12+rpt1`. All nine networking maintainer scripts matched node3.
   Temporarily mask the disabled NetworkManager. No transitional
   `/boot/firmware/config.txt` or `cmdline.txt` existed.
3. Run a persistent distribution-upgrade simulation and require exit 0. Its package
   actions matched node3. Apply the upgrade, use the documented camera repair,
   and verify package health and SSH before reboot 2.
4. Purge twelve selected service/media packages and remove eighty reviewed unused
   packages. Both plans matched node3 exactly. Configure persistent 2 GiB swap
   before removing dphys-swapfile. Restore NetworkManager to disabled and inactive,
   not masked.
5. Install Git and pinned Nix binaries. Import and verify the signed cache,
   realize the exact merged-main package with local builders disabled, and retain
   Mother in pi's profile. Perform reboot 3.
6. Confirm persistent swap, the Nix daemon socket, fresh SSH, and DNS. Run the
   offline Mother smoke as pi only after all three reboot checks passed.

| Reboot | Completed stage | New boot ID | Fresh key SSH and DNS |
| --- | --- | --- | --- |
| 1 | Bullseye preparation | `c9b24b6b-4c89-4a60-8b33-d59ebf3af4f2` | PASS |
| 2 | Bookworm upgrade and checks | `2c2f0cf5-6874-4afe-a480-ae420b6cf8df` | PASS |
| 3 | Slimming, swap, Nix, signed cache, Mother profile | `f8f324ae-6506-44f3-8750-da5ba3e0b939` | PASS |

The original boot ID was `0fecedeb-8284-46db-82c6-ee804d898df1`.
Each check required a changed boot ID and a new non-multiplexed dedicated-key
connection before the next stage. Initial connection refusals or timeouts during
reboot were retained in the logs; no reboot was retried.

The initial Bookworm upgrade exited 100 on the known collision:

```text
trying to overwrite '/usr/bin/camera-bug-report', which is also in package libcamera-apps-lite (1.2.1-1)
```

The camera-removal and GnuPG dependency repair actions matched node3 exactly.
The repair and resumed upgrade each exited 0, without forced overwrite.
The original failure log remains. No other upgrade repair was needed.
The first Nix-install observation found the installer still running; the operator
waited for its recorded exit 0 rather than rerunning it or advancing early.

No password-expiry refusal occurred. The password-change date remained February
21, 2023, with no expiry. Final `dpkg --audit` was empty and `sshd -t` passed.
Boot configuration, network interfaces, and authorized keys match the backup.
The dhcpcd comparison permits only the exact approved DNS replacement.
Original and current SSH configuration yield identical effective settings under
the upgraded parser.

## Final host state

| Item | Observed result |
| --- | --- |
| Hardware | Raspberry Pi 3 Model B Rev 1.2, aarch64, four cores, 909 MiB RAM |
| Hostname | `nicanode4`, unchanged |
| OS and kernel | Debian 12 Bookworm, `6.1.21-v8+` |
| Kernel and bootloader packages | `1:1.20230405-1` |
| dpkg | `1.22.22~bpo12+rpt1` |
| systemd | `252.39-1~deb12u2` |
| OpenSSH server | `1:9.2p1-2+deb12u10` |
| dhcpcd5 | `1:8.1.2-1+rpt9` |
| Address, gateway, DNS | `10.10.10.24/24`, `10.10.10.1`, resolvers `10.10.10.7 10.10.10.1` |
| Nix | 2.35.4 daemon, `max-jobs=0`, empty builders, signatures required |
| Swap | Persistent 2 GiB `/swapfile`; zero used after reboot 3, 768 KiB at final inspection |
| Final memory sample | 142 MiB used, 766 MiB available |
| Storage | Local `/dev/mmcblk0p2` ext4, 59 GiB root filesystem, approximately 46 GiB available |
| Installed Debian packages | 557 |
| TCP listeners | Port 22 only, IPv4 and IPv6 |
| UDP socket | Allowed dhcpcd client on UDP 68 |
| Failed units, smoke units, smoke package roots | None |
| Linger and clock | `Linger=no`, `NTPSynchronized=yes` |

Tailscale is absent and was not installed or changed. NetworkManager remains
disabled and inactive. Legacy `raspi-config` remains `20231012~bullseye`.
This is not a claim that every optional utility or the legacy kernel is current.
Final fresh SSH and DNS checks passed after the smoke.

## Signed cache and no-build evidence

The existing cache archive and public key matched the recorded SHA-256 values:

```text
nic53-node0-cache-fixed.tar
84cebfe4a9ac4f19d32a65f6e3c46651ddde054db70e1746fc9f19728408c6ee

nic53-node0-cache.pub
5af40c5f01b8b7866a842c01b14603205feddfd976e4c367364c42f73065a44d
```

The pinned Nix installer SHA-256 matched:

```text
118c3dfa04bdc946b3f577365b9a295607142c9b920e2c07b184e266d62b1ee1
```

Import required signatures, with the cache key scoped to import and verification
commands. The global trusted key remains the default `cache.nixos.org` key.
Recursive signature/content verification passed before execution. Recursive
content verification passed after the complete privileged lifecycle, exit 0.
No store repair or import-signature bypass was used.

The clean `aa87237` checkout resolved to these existing outputs:

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

Realization logs show a signed nixpkgs source substitution and a pinned overlay
source unpack, not compiler builds. Neither `rustc` nor `cargo` is on PATH.
The only three local derivation logs are empty `user-environment` logs.
Their version-4 records use `builtin:buildenv`, with no compiler command or input
derivation. Nix profile generation, OS configuration, and the required WASI JIT
probe are not zero local computation. No build was requested on nicamaster.

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
METRIC cold readiness 789.452 ms (n=1, process-cold through signed status; OS cache not purged)
METRIC idle RSS 17408..17408 KiB (n=5); Mother peak RSS 19424 KiB
METRIC component compile 28.624 ms; compile peak RSS 10912 KiB (fresh probe process, n=1)
METRIC durable-write p50=6.077 ms p95=7.835 ms max=18.694 ms (n=40; append+sync_data, verified reopen)
PASS immutable installer inputs
PASS isolated service cleanup; private evidence preserved
exit=0
```

Pre-run load was 2.56/0.75/0.26 on four cores shortly after reboot and closure
verification. This is one acceptance run, not a speed comparison. Readiness
includes authenticated status, the WASI result is checked, and all forty durable
writes are verified after reopen. Compile peak is the probe process's VmHWM,
not Mother RSS or build memory.

## Retained evidence and validation limits

Private software authority and measurements remain at
`/home/pi/.mct-install-smoke-u730xjgp`. Operational scripts, inventories, original
failures, DNS gates, reboot logs, and `RUN.md` remain under
`/home/pi/nic53-node4-work`. The backup, inventory note, credentials, and private
authority contents were not printed or published.

This repository change adds only this evidence document. It changes no authority,
durability, confinement, packaging, or browser behavior. Workstation checks include
`bash scripts/ci-tier0.sh specs`, `bash scripts/build-docs.sh`, and
`git diff --check`. Full Rust and browser suites were not rerun for this docs-only
change. No independent subagent review was available.

This proves bounded admin-only acceptance, not full Child workloads, fleet
capacity, replication, AOT safety, large-history startup, or power-loss recovery.
The three host reboots preceded the isolated smoke and do not prove a provisioned
production Mother survives reboot. Mother remains in pi's profile without a
production service or identity. No additional host rollout, push, or merge is
included.
