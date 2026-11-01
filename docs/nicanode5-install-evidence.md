# NIC-53 nicanode5 acceptance on merged main

## Verdict

Nicanode5 passed the physical Pi 3 Mother smoke on 2026-11-01 UTC, exit 0.
All three reboots passed fresh dedicated-key SSH and DNS checks. The smoke ran
after reboot 3. Final TCP listeners were SSH port 22 on IPv4 and IPv6 only.
The existing DHCP client retained UDP 68. No other UDP listener remained.

The tested checkout is merged main at
`aa87237008ea2508ea125128b312fc269c67444f`, the same revision tested on node3.
Nix resolved this checkout to the existing signed Mother derivation and smoke
output. No implementation changes were needed.

Host changes were restricted to nicanode5, `10.10.10.25`. Nicamaster supplied
existing cache files through read-only SCP. No build, configuration change, or
cache listener was started there. Nicanode4 was not accessed. Evidence work used
`/Users/nicabar/Projects/Patina/pi-fleet/nicanode5/patina-mct` on
`docs/nic53-nicanode5-acceptance`, not the checkout used by the parallel node4
session. No push, merge, or main-branch change was performed.

No Rust or package compilation occurred on the Pi. The required tiny WASI probe
JIT-compiled its embedded component, as disclosed before the run. No production
Mother service or identity was provisioned.

## Backup, DNS, and pre-upgrade gate

Hostname verification returned `nicanode5` before any host change. The baseline
was Pi 3 Model B Rev 1.2, aarch64 Debian 11, kernel `5.15.84-v8+`, 909 MiB RAM,
and 99 MiB swap. Root storage was 59 GiB with approximately 12 GiB available.
K3s-agent, Netdata, SSH, and dhcpcd were active. NetworkManager was inactive and
disabled. The package audit was empty, with no held packages.

The private backup is `/home/pi/pre-upgrade-backup/config-and-boot.tar`.
Tar comparison verified its contents against the live configuration before
mutation. The archive includes boot, SSH, network, apt, fstab, authorized-key,
k3s, and Netdata configuration. Package inventories, effective SSH settings,
routes, and firewall rules remain alongside it. Its SHA-256 passed again after
the smoke:

```text
89c151ea31dbdbcfbbf935b98b10bb9f2977d898d352ffcbb6f69d6fa94e91d8
```

DNS initially pointed only to router `10.10.10.1`. Direct DNS queries to both
approved resolvers succeeded. Exclusive root-owned mode-0600 backups preceded
this single-line change in `/etc/dhcpcd.conf`:

```diff
-static domain_name_servers=10.10.10.1
+static domain_name_servers=10.10.10.7 10.10.10.1
```

`dhcpcd -n eth0` applied the change without restarting cluster interfaces.
Complete IPv4 route output was byte-identical before and after the DNS change.
The address remained `10.10.10.25/24`, with default route through `10.10.10.1`
on `eth0`. Resolver order persisted through all three reboots:

```text
nameserver 10.10.10.7
nameserver 10.10.10.1
```

Fresh SSH connections used `~/.ssh/mother_cluster_ed25519`,
`IdentitiesOnly=yes`, `BatchMode=yes`, `StrictHostKeyChecking=yes`, and disabled
multiplexing. Fresh SSH, `getent ahostsv4 deb.debian.org`, and access-configuration
checks passed before service removal. NTP was synchronized.

The Bullseye source configuration omitted the obsolete security endpoint, as on
node3. The first apt refresh failed because Netdata's edge repository returned
404 and had no Release file. Its source file was moved into the private backup
as `netdata-edge.sources.disabled`. The next refresh passed before k3s removal:

```sh
apt-get update -o APT::Update::Error-Mode=any
```

That successful log and the post-removal refresh contain no `Err:`,
`Temporary failure`, `Failed to fetch`, `W:`, or `E:` lines. The original failed
refresh remains in the operational logs.

## K3s and Netdata removal

Both installed k3s removal scripts matched the previously reviewed node0
scripts by SHA-256. K3s uninstallation succeeded. Its routes disappeared while
the wired address and default route remained unchanged.

The reviewed Netdata purge plan removed fifteen packages. Netdata stopped, but
its post-removal script failed with exit 2 during configuration purge. The
script used `grep -qw /var/lib/netdata`, which also matched the existing child
path `/var/lib/netdata/www`. It then tried to remove a nonexistent parent-path
override with `dpkg-statoverride --remove`.

The override table and original maintainer script were preserved privately.
The repair asserted and removed only these two orphan Netdata overrides:

```text
root netdata 755 /usr/share/netdata/www
root netdata 755 /var/lib/netdata/www
```

A comparison verified that unrelated overrides were unchanged. Retrying the
Netdata purge succeeded without modifying the maintainer script. The package
audit, apt dependency check, SSH check, and fresh DNS lookup passed. `apt-get
clean` reclaimed approximately 32 GiB of stale package downloads, leaving
approximately 46 GiB free before the upgrades.

No Netdata package, systemd unit, or listener remained at final inspection.
Historical unregistered files were not deleted merely to make the filesystem
empty. `/opt/netdata` retained 235 MiB, `/var/log/netdata` retained 105 MiB, and
`/usr/share/netdata` retained 80 KiB. These were not running services.

## Upgrade and reboot evidence

The run reused node3's operational scripts with node5 hostname, address, and
path checks. The sequence followed the [node0 procedure](nicanode0-install-evidence.md):

1. Review the Bullseye dpkg-first preparation and upgrade plan. Its 133 upgrades
   matched node3 plus `libprotobuf23`, with no removals. Preparation exited 0.
2. Verify package health and access configuration, then complete reboot 1.
3. Switch to Bookworm sources and require a clean apt refresh. All downloaded
   networking packages and nine maintainer scripts matched node3. Temporarily
   mask the already-disabled NetworkManager. No transitional
   `/boot/firmware/config.txt` or `cmdline.txt` existed.
4. Upgrade dpkg first to `1.22.22~bpo12+rpt1`. Review the distribution simulation,
   which exited 0. Common target versions and removals matched node3. Node5 added
   upgrades for its existing `debian-keyring`, `libbson-1.0-0`, `libmongoc-1.0-0`,
   and `libmongocrypt0` packages.
5. Apply the distribution upgrade and the documented camera repair. Verify
   package health and access configuration, then complete reboot 2.
6. Purge twelve selected Bluetooth, Avahi, modem, triggerhappy, wireless, and
   media packages. Configure persistent 2 GiB swap before removing dphys-swapfile.
   Remove 86 unused packages after plan review. This was node3's eighty plus
   six unused Netdata dependencies. Restore NetworkManager to disabled and inactive.
7. Install Git and the pinned Nix binary distribution. Import and verify the
   existing signed cache. Resolve the pinned checkout with builders disabled,
   retain Mother in pi's profile, and complete reboot 3.
8. Verify fresh SSH, DNS, persistent swap, and the Nix daemon socket before the
   offline smoke.

| Reboot | Completed stage | New boot ID | Fresh key SSH and DNS |
| --- | --- | --- | --- |
| 1 | Bullseye preparation | `82672e9b-cd01-48ca-91cc-d23cdefafa0c` | PASS |
| 2 | Bookworm upgrade and checks | `ad6292cc-cc7e-4c87-a3d8-3c26c2fc7ec0` | PASS |
| 3 | Slimming, swap, Nix, signed cache, Mother profile | `6a9641e7-d10d-4f73-bda7-d078cd9653e1` | PASS |

The original boot ID was `7e9448a5-6c2b-454f-91ad-67a6c7ae4c49`.
Each reboot check required a changed boot ID before the next stage.

The initial Bookworm distribution upgrade exited 100 on the same collision
recorded for node3:

```text
trying to overwrite '/usr/bin/camera-bug-report', which is also in package libcamera-apps-lite (1.2.1-1)
```

The camera-removal and GnuPG repair plan matched node3's package actions exactly.
The repair and resumed upgrade each exited 0, without forced overwrite. Original
failed logs remain. No password or account expiry change was needed.

Boot configuration, network interfaces, and authorized keys match the original
backup. The dhcpcd comparison allows only the approved DNS replacement. Original
and current SSH configuration produce identical effective settings under the
upgraded SSH parser. Final `dpkg --audit` was empty and `sshd -t` passed.

## Final host state

| Item | Observed result |
| --- | --- |
| Hardware | Pi 3 Model B Rev 1.2, aarch64, four cores, 909 MiB RAM |
| Hostname | `nicanode5`, unchanged |
| OS and kernel | Debian 12 Bookworm, `6.1.21-v8+` |
| Kernel and bootloader packages | `1:1.20230405-1` |
| dpkg | `1.22.22~bpo12+rpt1` |
| systemd | `252.39-1~deb12u2` |
| OpenSSH server | `1:9.2p1-2+deb12u10` |
| dhcpcd5 | `1:8.1.2-1+rpt9` |
| DNS and gateway | `10.10.10.7`, then `10.10.10.1`; gateway `10.10.10.1` |
| Nix | 2.35.4 daemon, `max-jobs=0`, empty builders, signatures required |
| Swap | Persistent 2 GiB `/swapfile`; zero used after reboot 3, 512 KiB at final inspection |
| Final memory sample | 138 MiB used, 771 MiB available |
| Storage | Local `/dev/mmcblk0p2` ext4, 59 GiB root filesystem, approximately 42 GiB available |
| Installed Debian packages | 558 |
| TCP listeners | Port 22 only, IPv4 and IPv6 |
| UDP socket | Existing dhcpcd client on port 68 only |
| Failed units, smoke units, smoke package roots | None |
| Linger and clock | `Linger=no`, `NTPSynchronized=yes` |

The final package inventory matches node3's versions, with only the additional
`debian-keyring` package. NetworkManager remains disabled and inactive.
`raspi-config` remains `20231012~bullseye`, as on the earlier nodes. This is not
a claim that every optional utility or the legacy kernel is current.

## Signed cache and no-build evidence

The existing archive and public key were read from nicamaster's `/home/pi`.
Their hashes matched node3's recorded values and passed again during node5 import:

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

Import required signatures. The cache key was scoped to import and verification
commands. Global trust retained only the default `cache.nixos.org` key.
Recursive signature and content verification passed before execution. Recursive
content verification passed after the complete privileged lifecycle, exit 0.
No store repair or signature bypass was used.

The clean checkout resolved to these existing outputs:

```text
Mother: /nix/store/43c376cj22mfa100nwfivkcki38iirkh-mct-admin-mother-0.2.0-nic53
Rust derivation: /nix/store/83mbwa651b4vayhzhkpmkqmj17q0l04a-mct-admin-mother-0.2.0-nic53.drv
Smoke: /nix/store/bwwzwz230005rr072z4c7vk4vc18s7gn-mother-install-smoke
```

Realization and execution used:

```sh
nix build .#mother-install-smoke --max-jobs 0 --builders "" \
  --out-link /home/pi/nic53-smoke-package
nix run --offline --max-jobs 0 --builders "" .#mother-install-smoke
```

The realization log shows a signed nixpkgs source substitution and pinned
overlay source unpack, not compiler builds. Neither `rustc` nor `cargo` is on
PATH. The three local derivation logs are empty `user-environment` logs. Their
version-4 records name `builtin:buildenv`, with no compiler command or input
derivation. Profile generation, package configuration, and the WASI JIT probe
are not zero local computation. Mother remains in pi's profile without a
production service or identity.

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
METRIC cold readiness 440.225 ms (n=1, process-cold through signed status; OS cache not purged)
METRIC idle RSS 16676..16676 KiB (n=5); Mother peak RSS 18820 KiB
METRIC component compile 35.207 ms; compile peak RSS 10912 KiB (fresh probe process, n=1)
METRIC durable-write p50=5.593 ms p95=6.955 ms max=9.139 ms (n=40; append+sync_data, verified reopen)
PASS immutable installer inputs
PASS isolated service cleanup; private evidence preserved
exit=0
```

The smoke began around 21:39 UTC. Pre-run load was 2.84/0.82/0.28 on four cores
shortly after reboot and closure verification. This is one bounded acceptance
run, not a speed comparison or a throughput claim. Readiness includes signed
status. The WASI result is checked against 42, and all forty durable writes are
verified after reopen. Compile peak is the probe process's VmHWM, not Mother RSS
or package-build memory. No latency cause or limiting resource is inferred.

## Evidence and validation limits

Private software authority and measurements remain at
`/home/pi/.mct-install-smoke-n3ufrs12`. Operational scripts, original failures,
inventories, reboot logs, and `RUN.md` remain under `/home/pi/nic53-node5-work`.
The workstation trail is `/tmp/nic53-node5-plan.md`, with inspection artifacts
and `decisions.tsv` under `/tmp/nic53-node5-inspect`. Private backups and test
authority were not published or deleted.

Temporary SSH timeouts occurred during Bullseye preparation and Nix realization.
Fresh connections recovered before the next reboot gate. An evidence-copy
command also contained the mistyped address `10.10.25` and timed out. No
successful access or mutation at that address was observed. The copy was
repeated against the verified `10.10.10.25` address.

This repository change adds only this evidence document. Workstation validation
includes `bash scripts/ci-tier0.sh specs`, `bash scripts/build-docs.sh`, and
`git diff --check`. An automated evidence check compares the exact smoke output,
exit files, boot IDs, listener checks, and provenance against the recorded logs.
Full Rust and browser suites were not rerun for this docs-only change. No
independent subagent review was available.

This establishes bounded admin-only acceptance, not full Child workloads,
fleet capacity, replication, AOT safety, large-history startup, or power-loss
recovery. The three host reboots preceded the isolated smoke and do not prove
that a provisioned production Mother survives reboot. No further rollout,
push, or merge is included.
